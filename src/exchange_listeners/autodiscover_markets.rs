use anyhow::{anyhow, Context, Result};
use chrono::{Datelike, Duration, Timelike};
use chrono_tz::US::Eastern;
use log::{error, info};
use reqwest::Client;
use serde_json::Value;

/// Configuration describing the discovered Polymarket market.
#[derive(Debug, Clone)]
pub struct MarketConfig {
    pub name: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub end_time_et: String,
    pub binance_symbol: String,
    pub strike_price: f64,
}

/// Attempts to discover the active hourly `{CRYPTO}/USD` market and accompanying metadata.
pub async fn autodiscover_market_config(
    auto_market: &str,
    crypto: &str,
) -> Result<Option<MarketConfig>> {
    info!("--- Autodiscovering current crypto market ---");

    let client = Client::new();

    // 1. Determine the market slug from the current time (US/Eastern).
    let now_et = chrono::Utc::now().with_timezone(&Eastern);
    let start_hour_et = now_et
        .with_minute(0)
        .and_then(|dt| dt.with_second(0))
        .and_then(|dt| dt.with_nanosecond(0))
        .ok_or_else(|| anyhow!("failed to normalize Eastern time to start of hour"))?;
    let end_hour_et = start_hour_et + Duration::hours(1);

    let hour_str = start_hour_et.format("%I%p").to_string();
    let hour_str = hour_str.trim_start_matches('0').to_lowercase();
    let raw_day = start_hour_et.format("%d").to_string();
    let trimmed_day = raw_day.trim_start_matches('0');
    let day_str = if trimmed_day.is_empty() {
        "0"
    } else {
        trimmed_day
    }
    .to_string();

    let market_slug = start_hour_et
        .format(&format!(
            "{}-up-or-down-%B-{}-{}-et",
            auto_market, day_str, hour_str
        ))
        .to_string()
        .to_lowercase()
        .replace(' ', "-");
    info!("--> Target market slug: {}", market_slug);

    // 2. Fetch markets from Polymarket to find token IDs.
    let mut offset = 0usize;
    let mut target_event: Option<Value> = None;

    loop {
        let url = format!("https://gamma-api.polymarket.com/events?limit=100&active=true&archived=false&closed=false&order=volume24hr&ascending=false&offset={}", offset);
        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("failed to fetch events from {}", url))?;
        let events: Value = response
            .json()
            .await
            .context("failed to parse events response JSON")?;
        let events = events
            .as_array()
            .ok_or_else(|| anyhow!("events response is not an array"))?;

        if events.is_empty() {
            break;
        }

        if let Some(event) = events.iter().find(|event| {
            event
                .get("slug")
                .and_then(Value::as_str)
                .map(|slug| slug.eq_ignore_ascii_case(&market_slug))
                .unwrap_or(false)
        }) {
            target_event = Some(event.clone());
            break;
        }

        offset += events.len();
        if events.len() < 100 {
            break;
        }
    }

    let target_event = match target_event {
        Some(event) => event,
        None => {
            error!(
                "ERROR: Market with slug '{}' not found on Polymarket.",
                market_slug
            );
            return Ok(None);
        }
    };

    let markets = target_event
        .get("markets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("target event has no markets array"))?;

    let mut yes_token_id: Option<String> = None;
    let mut no_token_id: Option<String> = None;

    for market in markets {
        let token_ids: Vec<String> = match market.get("clobTokenIds") {
            Some(Value::String(s)) => {
                serde_json::from_str(s).context("failed to parse clobTokenIds JSON string")?
            }
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect(),
            _ => continue,
        };

        let outcomes: Vec<String> = match market.get("outcomes") {
            Some(Value::String(s)) => {
                serde_json::from_str(s).context("failed to parse outcomes JSON string")?
            }
            Some(Value::Array(arr)) => arr
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string)
                .collect(),
            _ => continue,
        };

        for (token_id, outcome) in token_ids.into_iter().zip(outcomes.into_iter()) {
            match outcome.to_lowercase().as_str() {
                "up" => {
                    yes_token_id = Some(token_id);
                }
                "down" => {
                    no_token_id = Some(token_id);
                }
                _ => {}
            }
        }

        if yes_token_id.is_some() && no_token_id.is_some() {
            break;
        }
    }

    let yes_token_id = match yes_token_id {
        Some(id) => id,
        None => {
            error!(
                "ERROR: Could not find 'Up' token for slug '{}'.",
                market_slug
            );
            return Ok(None);
        }
    };
    let no_token_id = match no_token_id {
        Some(id) => id,
        None => {
            error!(
                "ERROR: Could not find 'Down' token for slug '{}'.",
                market_slug
            );
            return Ok(None);
        }
    };

    info!("--> Found YES Token ID: {}", yes_token_id);
    info!("--> Found NO Token ID: {}", no_token_id);

    // 3. Fetch the strike price (open price of the 1H candle) from Binance.
    let start_timestamp_ms = start_hour_et.timestamp_millis();
    let binance_symbol = format!("{}USDT", crypto.to_uppercase());
    let binance_url = format!(
        "https://api.binance.com/api/v3/klines?symbol={}&interval=1h&startTime={}&limit=1",
        binance_symbol, start_timestamp_ms
    );

    let response = client
        .get(&binance_url)
        .send()
        .await
        .with_context(|| format!("failed to fetch klines from {}", binance_url))?;
    let data: Value = response
        .json()
        .await
        .context("failed to parse Binance kline response JSON")?;

    let strike_price = data
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|entry| entry.get(1))
        .and_then(Value::as_str)
        .and_then(|price| price.parse::<f64>().ok())
        .ok_or_else(|| {
            anyhow!(
                "could not fetch 1H candle data from Binance for timestamp {}",
                start_timestamp_ms
            )
        })?;

    info!("--> Found Strike Price: {}", strike_price);

    let config = MarketConfig {
        name: market_slug.clone(),
        yes_token_id,
        no_token_id,
        end_time_et: end_hour_et.format("%Y-%m-%d %H:%M:%S").to_string(),
        binance_symbol,
        strike_price,
    };

    info!("--- Market autodiscovery successful ---");
    Ok(Some(config))
}
