use log::error;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    exchange_listeners::{
        crypto_models::{
            CryptoPrice, CryptoPriceUpdate, get_crypto_orderbook_map, get_crypto_prices_map
        },
        orderbooks::{CryptoOrderbook, OrderbookDepth, OrderbookLevel, poly_orderbook::OrderBook},
        poly_models::{LegacyPriceChange, Listener, OrderSide, PriceChange},
    },
    strategies::{
        Strategy, StrategyContext, custom::peter::models::{MAX_VOLUME, OrderBookContext}, strategy_utils::{StrategyAsset, StrategyClient, StrategyOpenOrder, StrategyOrderBook, parse_millis}
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

        let exists = StrategyOpenOrder::order_exists(ctx, asset_id, OrderSide::Buy, bid_price, Self::TARGET_ORDER_SIZE);

        if exists {
            return None;
        }

        Some(PlannedOrder {
            price: bid_price,
            size: Self::TARGET_ORDER_SIZE,
            tick_size: orderbook.get_tick_size().to_string(),
        })
    }

    fn execute_order_plan(&self, ctx: Arc<StrategyContext>, asset_id: &str, plan: PlannedOrder) {

        let negrisk = StrategyAsset::is_negrisk(ctx.as_ref(), asset_id);
        let asset_id_owned = asset_id.to_string();
        let tick_size_owned = plan.tick_size.clone();

        let orders_to_cancel = StrategyOpenOrder::collect_orders_asset(ctx.as_ref(), asset_id);
        if let Err(err) = StrategyClient::cancel_orders(Arc::clone(&ctx), asset_id, orders_to_cancel)
        {
            error!(
                "[{}] Failed to cancel existing orders for {}: {}",
                self.name(),
                asset_id,
                err
            );
            return;
        }

        if let Err(err) = StrategyClient::place_limit_order(
            ctx,
            &asset_id_owned,
            OrderSide::Buy,
            plan.price,
            plan.size,
            tick_size_owned.as_str(),
            negrisk,
        ) {
            error!(
                "[{}] Failed to initiate limit order for {} at {}x{}: {}",
                self.name(), asset_id_owned, plan.price, plan.size, err
            );
        }
    }

}

impl Strategy for PeterStrategy {
    fn name(&self) -> &'static str {
        "PeterStrategy"
    }

    fn poly_handle_market_price_change(
        &self,
        ctx: Arc<StrategyContext>,
        _listener: Listener,
        _payload: &PriceChange,
    ) {
        let asset_id = &_payload.asset_id;

        let price_u32 = match parse_millis(&_payload.price) {
            Ok(price) => price,
            Err(err) => {
                error!(
                    "[{}] Failed to parse price '{}' for {}: {}",
                    self.name(), _payload.price, asset_id, err
                );
                return;
            }
        };

        let size_u32 = match parse_millis(&_payload.size) {
            Ok(size) => size,
            Err(err) => {
                error!(
                    "[{}] Failed to parse size '{}' for {}: {}",
                    self.name(), _payload.size, asset_id, err
                );
                return;
            }
        };

        if let Some(orderbook_entry) = ctx.poly_state.orderbooks.get(asset_id) {
            if let Ok(orderbook) = orderbook_entry.read() {
                if !StrategyOrderBook::price_matches_top_of_book(&orderbook, price_u32) {
                    return;
                }

                // The price dropped
                if size_u32 == 0 {
                    self.record_orderbook_context(&orderbook);
                }

                if let Some(plan) = self.plan_order(ctx.as_ref(), asset_id, &orderbook) {
                    drop(orderbook);
                    self.execute_order_plan(Arc::clone(&ctx), asset_id, plan);
                }
            }
        }
    }
}
