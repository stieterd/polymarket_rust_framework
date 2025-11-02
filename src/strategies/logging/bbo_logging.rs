use log::error;
use serde_json::{json, Map, Value};
use std::sync::Arc;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;

use crate::exchange_listeners::poly_models::{AggOrderbook, Listener, PriceChange};
use crate::strategies::StrategyContext;
use crate::Strategy;

#[derive(Default)]
pub struct BBOLoggingStrategy;

impl BBOLoggingStrategy {
    pub fn new() -> Self {
        Self
    }

    fn write_orderbook_line(
        asset_id: &str,
        bids: &[(u32, u32)],
        asks: &[(u32, u32)],
        timestamp: &String,
    ) -> io::Result<()> {
        fs::create_dir_all("output")?;
        let file_path = Path::new("output").join(format!("{}.ndjson", asset_id));

        let bids_json = Self::side_to_json(bids, true);
        let asks_json = Self::side_to_json(asks, false);
        let line_value = json!({
            "bids": bids_json,
            "asks": asks_json,
            "timestamp": timestamp,
        });

        let line = serde_json::to_string(&line_value)
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file_path)?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
        Ok(())
    }

    fn side_to_json(entries: &[(u32, u32)], descending: bool) -> Value {
        let mut sorted = entries.to_vec();
        sorted.sort_by_key(|(price, _)| *price);
        if descending {
            sorted.reverse();
        }

        let mut side_map = Map::new();
        for (price, size) in sorted {
            let price_key = format!("{:.3}", price as f64 / 1000.0);
            let size_value = (size as f64) / 1000.0;
            side_map.insert(price_key, Value::from(size_value));
        }
        Value::Object(side_map)
    }
}

impl Strategy for BBOLoggingStrategy {
    fn name(&self) -> &'static str {
        "BBOLoggingStrategy"
    }

    fn poly_handle_market_agg_orderbook(
        &self,
        _ctx: Arc<StrategyContext>,
        _listener: Listener,
        _snapshot: &AggOrderbook,
    ) {
    }

    fn poly_handle_market_price_change(
        &self,
        ctx: Arc<StrategyContext>,
        _listener: Listener,
        _payload: &PriceChange,
    ) {
        let asset_id = &_payload.asset_id;

        if let Some(orderbook_entry) = ctx.poly_state.orderbooks.get(asset_id) {
            if let Ok(orderbook) = orderbook_entry.read() {
                let price_u32 = match _payload.price.parse::<f64>() {
                    Ok(price_f) => (price_f * 1000.0).round() as u32,
                    Err(err) => {
                        error!(
                            "[BBOLoggingStrategy] Failed to parse price '{}' for {}: {}",
                            _payload.price, asset_id, err
                        );
                        return;
                    }
                };

                let best_bid = orderbook.best_bid();
                let best_ask = orderbook.best_ask();

                let matches_best = best_bid
                    .map(|(price, _)| price == price_u32)
                    .unwrap_or(false)
                    || best_ask
                        .map(|(price, _)| price == price_u32)
                        .unwrap_or(false);

                if !matches_best {
                    return;
                }

                let bids: Vec<(u32, u32)> = orderbook
                    .get_bid_map()
                    .iter()
                    .map(|entry| (*entry.key(), *entry.value()))
                    .collect();
                let asks: Vec<(u32, u32)> = orderbook
                    .get_ask_map()
                    .iter()
                    .map(|entry| (*entry.key(), *entry.value()))
                    .collect();

                let timestamp_ms = chrono::Utc::now().timestamp_millis().to_string();
                if let Err(err) = Self::write_orderbook_line(asset_id, &bids, &asks, &timestamp_ms)
                {
                    error!(
                        "[BBOLoggingStrategy] Failed to write orderbook for {}: {}",
                        asset_id, err
                    );
                }
            }
        }
    }
}
