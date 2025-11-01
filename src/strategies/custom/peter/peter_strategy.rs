use std::{collections::VecDeque, sync::{Arc, RwLock, Mutex}};
use log::error;

use crate::{
    exchange_listeners::{
        crypto_models::{
            CryptoPrice, CryptoPriceUpdate, get_crypto_orderbook_map, get_crypto_prices_map
        }, orderbooks::{CryptoOrderbook, OrderbookDepth, OrderbookLevel, poly_orderbook::OrderBook}, poly_client::PolyClient, poly_models::{LegacyPriceChange, Listener, OrderSide, PriceChange}
    },
    strategies::{Strategy, StrategyContext, custom::peter::models::{MAX_VOLUME, OrderBookContext}},
};

pub struct PeterStrategy
{
    orderbook_context_queue: Mutex<std::collections::VecDeque<OrderBookContext>>,
}

impl PeterStrategy {
    pub fn new() -> Self {
        Self {
            orderbook_context_queue: Mutex::new(VecDeque::with_capacity(1000)), // 1000 is the max size of the queue
        }
    }
}

impl Strategy for PeterStrategy {
    fn name(&self) -> &'static str {
        "PeterStrategy"
    }

    async fn poly_handle_market_price_change(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &PriceChange,
    ) {
        let asset_id = &_payload.asset_id;

        if let Some(orderbook_entry) = _ctx.poly_state.orderbooks.get(asset_id) {
            if let Ok(orderbook) = orderbook_entry.read() {
                let price_u32 = match _payload.price.parse::<f64>() {
                    Ok(price_f) => (price_f * 1000.0).round() as u32,
                    Err(err) => {
                        error!(
                            "[PeterStrategy] Failed to parse price '{}' for {}: {}",
                            _payload.price, asset_id, err
                        );
                        return;
                    }
                };

                // (price, volume) wrapped in an Option
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


                // There has been a change in best bid or best ask below

                // Collect Orderbook Context in our queue
                let orderbook_context = OrderBookContext {
                    midpoint: orderbook.get_midpoint(),
                    spread: orderbook.get_spread(),
                };

                if let Ok(mut queue) = self.orderbook_context_queue.lock() {
                    queue.push_back(orderbook_context);
                }

                let tick_size = orderbook.get_tick_size();

                // If volume of best ask is less than MAX_VOLUME
                if best_ask.unwrap().1 < MAX_VOLUME {
                    // we trade here
                    PolyClient::place_limit_order(&_ctx.poly_state, asset_id, OrderSide::Buy, best_ask.unwrap().0, best_ask.unwrap().1, tick_size).await;
                }

            }
        }
    }

}