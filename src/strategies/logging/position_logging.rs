use log::info;
use std::sync::Arc;

use crate::{
    exchange_listeners::{
        Crypto, Exchange, Instrument, crypto_models::CryptoPriceUpdate, orderbooks::{CryptoOrderbook, OrderbookDepth, poly_orderbook::OrderBook}, poly_models::{LegacyPriceChange, Listener, PriceChange, TradeStatus}
    },
    strategies::{Strategy, StrategyContext},
};

pub struct PositionLoggingStrategy;

impl PositionLoggingStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Strategy for PositionLoggingStrategy {
    fn name(&self) -> &'static str {
        "PositionLogger"
    }

    fn poly_handle_user_trade(
        &self,
        ctx: Arc<StrategyContext>,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TradePayload,
    ) {
        if _payload.status != TradeStatus::Matched {
            return;
        }
        // info!("Just received a message from {}", _exchange);
        for entry in ctx.poly_state.positions.iter() {
            let asset_id = entry.key();
            let position_lock = entry.value();
            if let Ok(position) = position_lock.read() {
                info!(
                    "[PositionLogger] Asset: {}, Size: {}",
                    asset_id, position.size
                );
            } else {
                info!(
                    "[PositionLogger] Failed to read position for asset {}",
                    asset_id
                );
            }
        }
    }
}
