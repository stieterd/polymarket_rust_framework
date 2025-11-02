use std::sync::{Arc, RwLock};

use crate::{
    exchange_listeners::{
        crypto_models::{
            get_crypto_orderbook_map, get_crypto_prices_map, CryptoPrice, CryptoPriceUpdate,
        },
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth, OrderbookLevel},
        poly_models::{LegacyPriceChange, Listener, PriceChange},
    },
    strategies::Strategy,
};

pub struct UpdateCryptoPriceStrategy;

impl UpdateCryptoPriceStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Strategy for UpdateCryptoPriceStrategy {
    fn name(&self) -> &'static str {
        "UpdateCryptoPrices"
    }

    fn crypto_handle_price_update(
        &self,
        ctx: Arc<crate::strategies::StrategyContext>,
        _exchange: crate::exchange_listeners::Exchange,
        _instrument: crate::exchange_listeners::Instrument,
        _crypto: crate::exchange_listeners::Crypto,
        _depth: OrderbookDepth,
        _price_update: &CryptoPriceUpdate,
    ) {
        let orderbook_map = get_crypto_orderbook_map(ctx.app_state.clone(), _crypto);
        let prices_map = get_crypto_prices_map(ctx.app_state.clone(), _crypto);

        let key = (_exchange, _instrument, _depth);

        // Apply the price update to the orderbook, only for L1 (best bid/ask).
        if let OrderbookDepth::L1 = _depth {
            let mut orderbook = orderbook_map
                .entry(key)
                .or_insert_with(|| CryptoOrderbook::new(_depth));

            let midpoint = orderbook.get_midpoint();
            let mut crypto_price = prices_map.entry(key).or_insert_with(|| CryptoPrice::new());
            crypto_price.price = midpoint;
            crypto_price.midpoint = midpoint;
        }
    }
}
