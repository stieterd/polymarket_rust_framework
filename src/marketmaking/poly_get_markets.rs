use super::poly_market_struct::EventJson;
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct Position {
    asset: String,
    size: String, // We'll parse this to f64
                  // ... other fields if needed
}

pub async fn fetch_neg_risk_markets() -> Result<Vec<EventJson>, Box<dyn std::error::Error>> {
    let client = Client::new();
    let mut offset = 0;
    let mut length = 100;
    let mut response_list: Vec<EventJson> = Vec::new();

    while length == 100 {
        let url = format!("https://gamma-api.polymarket.com/events?limit=100&active=true&archived=false&closed=false&order=volume24hr&ascending=false&offset={}", offset);
        let response = client
            .get(&url)
            .header("Host", "gamma-api.polymarket.com")
            .header("User-Agent", "Mozilla/5.0")
            .header("Accept", "application/json, text/plain, */*")
            .send()
            .await?;

        let json: Vec<EventJson> = response.json().await?;

        length = json.len();

        response_list.extend(json);
        offset += length;
    }

    let neg_risk_markets = filter_neg_risk_markets(response_list);
    Ok(neg_risk_markets)
}

fn filter_neg_risk_markets(events: Vec<EventJson>) -> Vec<EventJson> {
    events
        .into_iter()
        .filter(|event| event.negRisk.unwrap_or(false)) // Filter by negRisk
        .filter(|event| event.enableNegRisk.unwrap_or(false))
        .map(|mut event| {
            if let Some(ref mut markets) = event.markets {
                // Retain markets that do not have the specified slug
                markets.retain(|market| {
                    market.active.unwrap_or(true) && market.acceptingOrders.unwrap_or(true)
                });
            }
            event
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("epl-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("bitcoin-price-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("ethereum-price-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("solana-price-")
        })
        // .filter(|event| !event.slug.clone().unwrap_or("".to_string()).contains("open-winner"))
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("-stanley-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("europa-league-")
        })
        // .filter(|event| !event.slug.clone().unwrap_or("".to_string()).contains("superbowl-champion-"))
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("champions-league-winner-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("uefa-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("nfl-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("afc-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("uel-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("liga-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("-vs-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("-temperature-")
        })
        .filter(|event| {
            !event
                .slug
                .clone()
                .unwrap_or("".to_string())
                .contains("fifa")
        })
        .filter(|event| event.markets.clone().unwrap().len() < 30)
        .filter(|event| {
            if let Some(ref tags) = event.tags {
                let mut contains_sports = false;
                for tag in tags.iter() {
                    let slug = tag.slug.clone().unwrap_or_default();
                    if slug.contains("sports") {
                        contains_sports = true;
                        break;
                    }
                }
                !contains_sports
            } else {
                true
            }
        })
        .collect()
}

pub async fn get_positions(user: &str) -> HashMap<String, f64> {
    let client = reqwest::Client::new();
    let mut all_positions: Vec<Position> = Vec::new();
    let mut offset = 0;
    let mut position_length = 500;
    while position_length >= 500 {
        let url = format!(
            "https://data-api.polymarket.com/positions?user={}&limit=500&offset={}",
            user, offset
        );
        let resp = client.get(&url).send().await;
        let returned_positions: Vec<Position> = match resp {
            Ok(r) => match r.json::<Vec<Position>>().await {
                Ok(json) => json,
                Err(_) => break,
            },
            Err(_) => break,
        };
        position_length = returned_positions.len();
        offset += position_length;
        all_positions.extend(returned_positions);
    }
    // Build a lookup for asset -> size
    let mut asset_to_size: HashMap<String, f64> = HashMap::new();
    for pos in &all_positions {
        if let Ok(size) = pos.size.parse::<f64>() {
            asset_to_size.insert(pos.asset.clone(), size);
        }
    }
    asset_to_size
}

// Fallback version for error case
pub fn get_positions_fallback() -> HashMap<String, f64> {
    let mut map = HashMap::new();
    map.insert("yes".to_string(), 0.0);
    map.insert("no".to_string(), 0.0);
    map
}
