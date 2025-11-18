use log::{error, warn};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use crate::credentials::ADDRESS_STR;
use crate::exchange_listeners::poly_models::{AggOrderbook, Listener, OrderSide, PriceChange, TradeRole, TradeStatus};
use crate::strategies::strategy_utils::{StrategyAsset, parse_millis};
use crate::strategies::StrategyContext;
use crate::Strategy;

#[derive(Default)]
pub struct TradeLoggingStrategy;

#[derive(Default)]
struct MakerTradeTotals {
    buy_amount: u32,
    buy_price_sum: u128,
    sell_amount: u32,
    sell_price_sum: u128,
}

impl TradeLoggingStrategy {
    pub fn new() -> Self {
        Self
    }

    fn write_orderbook_line(
        asset_id: &str,
        price: u32,
        size: u32,
        order_type: &str,
        order_side: &str,
        timestamp: &String,
    ) -> io::Result<()> {
        fs::create_dir_all("output")?;
        let file_path = Path::new("output").join(format!("trades.ndjson"));

        let line_value = json!({
            "asset_id": asset_id,
            "price": price,
            "size": size,
            "type": order_type,
            "side": order_side,
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

    fn log_trade_event(
        strategy_name: &str,
        asset_id: &str,
        price: u32,
        size: u32,
        order_type: &str,
        side: OrderSide,
        timestamp: &String,
    ) {
        if let Err(err) =
            Self::write_orderbook_line(asset_id, price, size, order_type, side.as_str(), timestamp)
        {
            error!(
                "[{}] Failed to write {} {} trade for asset {}: {}",
                strategy_name,
                order_type,
                side.as_str(),
                asset_id,
                err
            );
        }
    }

}

impl Strategy for TradeLoggingStrategy {
    fn name(&self) -> &'static str {
        "TradeLoggingStrategy"
    }

    fn poly_handle_user_trade(
            &self,
            _ctx: Arc<StrategyContext>,
            _listener: Listener,
            _payload: &crate::exchange_listeners::poly_models::TradePayload,
        ) {
        
        if _payload.status != TradeStatus::Matched {
            return;
        }

        match _payload.trade_role {
            TradeRole::Taker => {
                let asset_id = _payload.asset_id.clone();

                let size_u32 = match parse_millis(&_payload.size) {
                    Ok(size) => size,
                    Err(err) => {
                        warn!(
                            "[{}] Failed to parse size '{}' for asset {}: {}",
                            self.name(),
                            _payload.size,
                            asset_id,
                            err
                        );
                        return;
                    }
                };

                let side = match OrderSide::from_str(_payload.side.as_str()) {
                    Some(s) => s,
                    None => {
                        warn!(
                            "[{}] Unknown side '{}' for asset {}; skipping position update",
                            self.name(),
                            _payload.side,
                            asset_id
                        );
                        return;
                    }
                };

                let price_u32 = match parse_millis(&_payload.price) {
                    Ok(price) => price,
                    Err(err) => {
                        warn!(
                            "[{}] Failed to parse price '{}' for asset {}: {}",
                            self.name(),
                            _payload.price,
                            asset_id,
                            err
                        );
                        return;
                    }
                };

                Self::log_trade_event(
                    self.name(),
                    &asset_id,
                    price_u32,
                    size_u32,
                    "TAKER",
                    side,
                    &_payload.timestamp,
                );
            }
            
            TradeRole::Maker => {
                let mut per_asset: HashMap<String, MakerTradeTotals> = HashMap::new();
                for maker_order in &_payload.maker_orders {
                    if !maker_order.maker_address.eq_ignore_ascii_case(ADDRESS_STR) {
                        continue;
                    }

                    let matched_u32 = match parse_millis(&maker_order.matched_amount) {
                        Ok(size) => size,
                        Err(err) => {
                            warn!(
                                "[{}] Failed to parse maker matched_amount '{}' for asset {}: {}",
                                self.name(),
                                maker_order.matched_amount,
                                maker_order.asset_id,
                                err
                            );
                            continue;
                        }
                    };

                    let maker_side = match OrderSide::from_str(maker_order.side.as_str()) {
                        Some(side) => side,
                        None => {
                            warn!(
                                "[{}] Unknown maker side '{}' for asset {}; skipping maker slice",
                                self.name(),
                                maker_order.side,
                                maker_order.asset_id
                            );
                            continue;
                        }
                    };

                    let price_u32 = match parse_millis(&maker_order.price) {
                        Ok(price) => price,
                        Err(err) => {
                            warn!(
                                "[{}] Failed to parse maker price '{}' for asset {}: {}",
                                self.name(),
                                maker_order.price,
                                maker_order.asset_id,
                                err
                            );
                            continue;
                        }
                    };

                    let entry = per_asset
                        .entry(maker_order.asset_id.clone())
                        .or_insert_with(MakerTradeTotals::default);
                    match maker_side {
                        OrderSide::Buy => {
                            entry.buy_price_sum += (price_u32 as u128) * (matched_u32 as u128);
                            entry.buy_amount = entry.buy_amount.saturating_add(matched_u32);
                        }
                        OrderSide::Sell => {
                            entry.sell_price_sum += (price_u32 as u128) * (matched_u32 as u128);
                            entry.sell_amount = entry.sell_amount.saturating_add(matched_u32);
                        }
                    }
                }

                if per_asset.is_empty() {
                    return;
                }

                for (asset_id, totals) in per_asset {
                    if totals.buy_amount > 0 {
                        let avg_price =
                            (totals.buy_price_sum / totals.buy_amount as u128) as u32;
                        Self::log_trade_event(
                            self.name(),
                            &asset_id,
                            avg_price,
                            totals.buy_amount,
                            "MAKER",
                            OrderSide::Buy,
                            &_payload.timestamp,
                        );
                    }

                    if totals.sell_amount > 0 {
                        let avg_price =
                            (totals.sell_price_sum / totals.sell_amount as u128) as u32;
                        Self::log_trade_event(
                            self.name(),
                            &asset_id,
                            avg_price,
                            totals.sell_amount,
                            "MAKER",
                            OrderSide::Sell,
                            &_payload.timestamp,
                        );
                    }
                }
            }
            TradeRole::Unknown => {}
        }
        
    }

    
}
