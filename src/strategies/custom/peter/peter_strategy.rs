use log::{error, info};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, RwLock},
};

use crate::{
    exchange_listeners::{
        crypto_models::{
            get_crypto_orderbook_map, get_crypto_prices_map, CryptoPrice, CryptoPriceUpdate,
        },
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth, OrderbookLevel},
        poly_client::PolyClient,
        poly_models::{LegacyPriceChange, Listener, OrderSide, PriceChange},
    },
    strategies::{
        custom::peter::models::{OrderBookContext, MAX_VOLUME},
        Strategy, StrategyContext,
    },
};

pub struct PeterStrategy {
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

    fn poly_handle_market_price_change(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &PriceChange,
    ) {
        let asset_id = &_payload.asset_id;

        let mut order_to_place: Option<(u32, u32, String)> = None;

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

                // If price change in best bid or best ask
                if _payload.size == "0" {
                    // Collect Orderbook Context in our queue
                    let orderbook_context = OrderBookContext {
                        midpoint: orderbook.get_midpoint(),
                        spread: orderbook.get_spread(),
                    };

                    if let Ok(mut queue) = self.orderbook_context_queue.lock() {
                        queue.push_back(orderbook_context);
                        while queue.len() > 1000 {
                            queue.pop_front();
                        }
                    }
                }

                let negrisk = _ctx
                    .poly_state
                    .markets
                    .get(asset_id)
                    .and_then(|m| m.negRisk.clone())
                    .unwrap_or(false);

                if let Some((bid_price, bid_size)) = best_bid {
                    if bid_size > MAX_VOLUME {
                        if let Ok(rate_limit) = _ctx.poly_state.rate_limit.read() {
                            if !rate_limit.should_wait() {
                                let exists = _ctx
                                    .poly_state
                                    .open_orders
                                    .get(asset_id)
                                    .map(|orders| {
                                        orders.order_exists(OrderSide::Buy, bid_price, bid_size)
                                    })
                                    .unwrap_or(false);
                                if !exists {
                                    order_to_place = Some((
                                        bid_price,
                                        bid_size,
                                        orderbook.get_tick_size().to_string(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some((price, size, tick_size)) = order_to_place {
            println!("We are placing an order");
            let negrisk = _ctx
                .poly_state
                .markets
                .get(asset_id)
                .and_then(|m| m.negRisk.clone())
                .unwrap_or(false);

            let poly_state = Arc::clone(&_ctx.poly_state);
            let asset_id_owned = asset_id.clone();
            let tick_size_owned = tick_size.clone();

            if let Some(existing_orders) = _ctx.poly_state.open_orders.get(asset_id) {
                let poly_state_cancel = Arc::clone(&_ctx.poly_state);
                let asset_id_cancel = asset_id.to_string();
                let orders_to_cancel: Vec<(OrderSide, u32, u32)> = existing_orders
                    .bids
                    .iter()
                    .map(|entry| (OrderSide::Buy, entry.key().0, entry.key().1))
                    .chain(
                        existing_orders
                            .asks
                            .iter()
                            .map(|entry| (OrderSide::Sell, entry.key().0, entry.key().1)),
                    )
                    .collect();
                if !orders_to_cancel.is_empty() {
                    tokio::spawn(async move {
                        for (side, price, size) in orders_to_cancel {
                            let _ = PolyClient::cancel_limit_order(
                                Arc::clone(&poly_state_cancel),
                                &asset_id_cancel,
                                side,
                                price,
                                size,
                            );
                        }
                    });
                }
            }

            if let Err(err) = PolyClient::place_limit_order(
                poly_state,
                &asset_id_owned,
                OrderSide::Buy,
                price,
                10000,
                tick_size_owned.as_str(),
                negrisk,
            ) {
                error!(
                    "[PeterStrategy] Failed to initiate limit order for {} at {}x{}: {}",
                    asset_id_owned, price, size, err
                );
            }
        }
    }
}
