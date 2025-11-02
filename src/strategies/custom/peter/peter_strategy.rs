use log::error;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
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

struct PlannedOrder {
    price: u32,
    size: u32,
    tick_size: String,
}

impl PeterStrategy {
    const TARGET_ORDER_SIZE: u32 = 10_000;

    pub fn new() -> Self {
        Self {
            orderbook_context_queue: Mutex::new(VecDeque::with_capacity(1000)), // 1000 is the max size of the queue
        }
    }

    fn parse_price_millis(price: &str) -> Result<u32, String> {
        price
            .parse::<f64>()
            .map(|price_f| (price_f * 1000.0).round() as u32)
            .map_err(|err| format!("{}", err))
    }

    fn price_matches_top_of_book(orderbook: &OrderBook, price: u32) -> bool {
        let bid_matches = orderbook
            .best_bid()
            .map(|(bid_price, _)| bid_price == price)
            .unwrap_or(false);
        let ask_matches = orderbook
            .best_ask()
            .map(|(ask_price, _)| ask_price == price)
            .unwrap_or(false);

        bid_matches || ask_matches
    }

    fn record_orderbook_context(&self, orderbook: &OrderBook) {
        let context = OrderBookContext {
            midpoint: orderbook.get_midpoint(),
            spread: orderbook.get_spread(),
        };

        if let Ok(mut queue) = self.orderbook_context_queue.lock() {
            queue.push_back(context);
            while queue.len() > 1000 {
                queue.pop_front();
            }
        }
    }

    fn plan_order(
        &self,
        ctx: &StrategyContext,
        asset_id: &str,
        orderbook: &OrderBook,
    ) -> Option<PlannedOrder> {
        let (bid_price, bid_size) = orderbook.best_bid()?;
        if bid_size <= MAX_VOLUME {
            return None;
        }

        if let Ok(rate_limit) = ctx.poly_state.rate_limit.read() {
            if rate_limit.should_wait() {
                return None;
            }
        }

        let order_exists = ctx
            .poly_state
            .open_orders
            .get(asset_id)
            .map(|orders| {
                orders.order_exists(
                    OrderSide::Buy,
                    bid_price,
                    Self::TARGET_ORDER_SIZE,
                )
            })
            .unwrap_or(false);

        if order_exists {
            return None;
        }

        Some(PlannedOrder {
            price: bid_price,
            size: Self::TARGET_ORDER_SIZE,
            tick_size: orderbook.get_tick_size().to_string(),
        })
    }

    fn execute_order_plan(&self, ctx: &StrategyContext, asset_id: &str, plan: PlannedOrder) {
        println!("We are placing an order");

        let negrisk = Self::neg_risk_for_asset(ctx, asset_id);
        let poly_state = Arc::clone(&ctx.poly_state);
        let asset_id_owned = asset_id.to_string();
        let tick_size_owned = plan.tick_size.clone();

        let orders_to_cancel = Self::collect_orders_to_cancel(ctx, asset_id);
        if !orders_to_cancel.is_empty() {
            let poly_state_cancel = Arc::clone(&ctx.poly_state);
            let asset_id_cancel = asset_id.to_string();
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

        if let Err(err) = PolyClient::place_limit_order(
            poly_state,
            &asset_id_owned,
            OrderSide::Buy,
            plan.price,
            plan.size,
            tick_size_owned.as_str(),
            negrisk,
        ) {
            error!(
                "[PeterStrategy] Failed to initiate limit order for {} at {}x{}: {}",
                asset_id_owned, plan.price, plan.size, err
            );
        }
    }

    fn collect_orders_to_cancel(
        ctx: &StrategyContext,
        asset_id: &str,
    ) -> Vec<(OrderSide, u32, u32)> {
        ctx.poly_state
            .open_orders
            .get(asset_id)
            .map(|orders| {
                orders
                    .bids
                    .iter()
                    .map(|entry| (OrderSide::Buy, entry.key().0, entry.key().1))
                    .chain(
                        orders
                            .asks
                            .iter()
                            .map(|entry| (OrderSide::Sell, entry.key().0, entry.key().1)),
                    )
                    .collect()
            })
            .unwrap_or_default()
    }

    fn neg_risk_for_asset(ctx: &StrategyContext, asset_id: &str) -> bool {
        ctx.poly_state
            .markets
            .get(asset_id)
            .and_then(|m| m.negRisk.clone())
            .unwrap_or(false)
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

        let price_u32 = match Self::parse_price_millis(&_payload.price) {
            Ok(price) => price,
            Err(err) => {
                error!(
                    "[PeterStrategy] Failed to parse price '{}' for {}: {}",
                    _payload.price, asset_id, err
                );
                return;
            }
        };

        if let Some(orderbook_entry) = _ctx.poly_state.orderbooks.get(asset_id) {
            if let Ok(orderbook) = orderbook_entry.read() {
                if !Self::price_matches_top_of_book(&orderbook, price_u32) {
                    return;
                }

                if _payload.size == "0" {
                    self.record_orderbook_context(&orderbook);
                }

                if let Some(plan) = self.plan_order(_ctx, asset_id, &orderbook) {
                    drop(orderbook);
                    self.execute_order_plan(_ctx, asset_id, plan);
                }
            }
        }
    }
}
