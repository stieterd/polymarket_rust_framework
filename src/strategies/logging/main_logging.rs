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

pub struct MainLoggingStrategy;

impl MainLoggingStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Strategy for MainLoggingStrategy {
    fn name(&self) -> &'static str {
        "MainLogger"
    }

    async fn crypto_handle_price_update(
        &self,
        _ctx: &StrategyContext,
        _exchange: Exchange,
        _instrument: Instrument,
        _crypto: Crypto,
        _depth: OrderbookDepth,
        _price_update: &CryptoPriceUpdate,
    ) {
        // info!("Just received a message from {}", _exchange);
    }
}
