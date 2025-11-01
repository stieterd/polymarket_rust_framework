use async_trait::async_trait;
use std::sync::{Arc, RwLock};

use crate::{
    exchange_listeners::{
        orderbooks::poly_orderbook::OrderBook,
        poly_models::{Listener, Position},
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

#[async_trait]
impl Strategy for UpdatePositionStrategy {
    fn name(&self) -> &'static str {
        "UpdatePositions"
    }

    async fn poly_handle_user_trade(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TradePayload,
    ) {
        if let Ok(trade_size) = _payload.size.parse::<f64>() {
            let asset_id = _payload.asset_id.clone();
            match _ctx.poly_state.positions.entry(asset_id.clone()) {
                Entry::Occupied(mut entry) => {
                    if let Ok(mut position) = entry.get().write() {
                        position.size += trade_size;
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
                        trade_size,
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
