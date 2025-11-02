use log::error;
use std::sync::{Arc, RwLock};

use crate::{
    exchange_listeners::{
        crypto_models::{
            get_crypto_orderbook_map, get_crypto_prices_map, CryptoPrice, CryptoPriceUpdate,
        },
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth, OrderbookLevel},
        poly_client::PolyClient,
        poly_models::{LegacyPriceChange, Listener, PriceChange},
    },
    strategies::{Strategy, StrategyContext},
};

pub struct KoenStrategy;

impl KoenStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Strategy for KoenStrategy {
    fn name(&self) -> &'static str {
        "KoenStrategy"
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
                            "[KoenStrategy] Failed to parse price '{}' for {}: {}",
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

                let client = PolyClient;

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
            }
        }
    }
}
