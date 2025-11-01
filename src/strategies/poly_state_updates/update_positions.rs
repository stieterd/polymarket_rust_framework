use std::sync::{Arc, RwLock};

use crate::{
    exchange_listeners::{
        orderbooks::poly_orderbook::OrderBook,
        poly_models::{Listener, OrderSide, Position},
    },
    strategies::Strategy,
};
use dashmap::mapref::entry::Entry;
use log::warn;

pub struct UpdatePositionStrategy;

impl UpdatePositionStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Strategy for UpdatePositionStrategy {
    fn name(&self) -> &'static str {
        "UpdatePositions"
    }

    fn poly_handle_user_trade(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TradePayload,
    ) {
        if let Ok(trade_size) = _payload.size.parse::<f64>() {
            let asset_id = _payload.asset_id.clone();
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
            let signed_size = match side {
                OrderSide::Buy => trade_size,
                OrderSide::Sell => -trade_size,
            };
            match _ctx.poly_state.positions.entry(asset_id.clone()) {
                Entry::Occupied(mut entry) => {
                    if let Ok(mut position) = entry.get().write() {
                        position.size += signed_size;
                    } else {
                        warn!(
                            "[{}] Failed to write position lock for asset {}",
                            _listener.as_str(),
                            asset_id
                        );
                    }
                }
                Entry::Vacant(entry) => {
                    entry.insert(Arc::new(RwLock::new(Position::new(
                        asset_id.clone(),
                        signed_size,
                    ))));
                }
            }
        } else {
            warn!(
                "[{}] Failed to update position size '{}' for asset {}",
                self.name(),
                _payload.size,
                _payload.asset_id
            );
        }
    }
}
