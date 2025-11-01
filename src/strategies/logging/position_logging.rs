use async_trait::async_trait;
use log::info;

use crate::{
    exchange_listeners::{
        crypto_models::CryptoPriceUpdate,
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth},
        poly_models::{LegacyPriceChange, Listener, PriceChange},
        Crypto, Exchange, Instrument,
    },
    strategies::{Strategy, StrategyContext},
};

pub struct PositionLoggingStrategy;

impl PositionLoggingStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Strategy for PositionLoggingStrategy {
    fn name(&self) -> &'static str {
        "PositionLogger"
    }

    async fn poly_handle_user_trade(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TradePayload,
    ) {
        // info!("Just received a message from {}", _exchange);
        for entry in _ctx.poly_state.positions.iter() {
            let asset_id = entry.key();
            let position_lock = entry.value();
            if let Ok(position) = position_lock.read() {
                info!(
                    "[PositionLogger] Asset: {}, Size: {}",
                    asset_id,
                    position.size
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
