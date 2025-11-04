use std::{collections::HashMap, sync::{Arc, RwLock}};

use crate::{
    credentials::ADDRESS_STR,
    exchange_listeners::{
        orderbooks::poly_orderbook::OrderBook,
        poly_models::{Listener, OrderSide, Position, TradeRole, TradeStatus},
    },
    strategies::{Strategy, StrategyContext, strategy_utils::parse_millis},
};
use dashmap::mapref::entry::Entry;
use log::warn;

pub struct UpdatePositionStrategy;

impl UpdatePositionStrategy {
    pub fn new() -> Self {
        Self
    }

    fn apply_position_delta(
        ctx: &StrategyContext,
        listener: &Listener,
        asset_id: &str,
        side: OrderSide,
        amount: u32,
        context: Option<&str>,
    ) {
        match ctx.poly_state.positions.entry(asset_id.to_string()) {
            Entry::Occupied(entry) => {
                if let Ok(mut position) = entry.get().write() {
                    match side {
                        OrderSide::Buy => {
                            position.size = position.size.saturating_add(amount);
                        }
                        OrderSide::Sell => {
                            position.size = position.size.saturating_sub(amount);
                        }
                    }
                } else {
                    warn!(
                        "[{}] Failed to write position lock for asset {}",
                        listener.as_str(),
                        asset_id
                    );
                }
            }
            Entry::Vacant(entry) => {
                let initial_size = match side {
                    OrderSide::Buy => amount,
                    OrderSide::Sell => {
                        let source = context.unwrap_or("taker");
                        warn!(
                            "[{}] Received {} sell trade for asset {} without existing position; defaulting to zero",
                            listener.as_str(),
                            source,
                            asset_id
                        );
                        0
                    }
                };
                entry.insert(Arc::new(RwLock::new(Position::new(
                    asset_id.to_string(),
                    initial_size,
                ))));
            }
        }
    }
}

impl Strategy for UpdatePositionStrategy {
    fn name(&self) -> &'static str {
        "UpdatePositions"
    }

    fn poly_handle_user_trade(
        &self,
        ctx: Arc<crate::strategies::StrategyContext>,
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

                Self::apply_position_delta(ctx.as_ref(), &_listener, &asset_id, side, size_u32, None);
            }
            TradeRole::Maker => {
                let mut per_asset: HashMap<String, (u32, u32)> = HashMap::new();
                for maker_order in &_payload.maker_orders {
                    if !maker_order
                        .maker_address
                        .eq_ignore_ascii_case(ADDRESS_STR)
                    {
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

                    let entry = per_asset
                        .entry(maker_order.asset_id.clone())
                        .or_insert((0, 0));
                    match maker_side {
                        OrderSide::Buy => entry.0 = entry.0.saturating_add(matched_u32),
                        OrderSide::Sell => entry.1 = entry.1.saturating_add(matched_u32),
                    }
                }

                if per_asset.is_empty() {
                    return;
                }

                for (asset_id, (buy_amount, sell_amount)) in per_asset {
                    if buy_amount > 0 {
                        Self::apply_position_delta(
                            ctx.as_ref(),
                            &_listener,
                            asset_id.as_str(),
                            OrderSide::Buy,
                            buy_amount,
                            Some("maker"),
                        );
                    }

                    if sell_amount > 0 {
                        Self::apply_position_delta(
                            ctx.as_ref(),
                            &_listener,
                            asset_id.as_str(),
                            OrderSide::Sell,
                            sell_amount,
                            Some("maker"),
                        );
                    }
                }
            }
            TradeRole::Unknown => {}
        }
    }
}
