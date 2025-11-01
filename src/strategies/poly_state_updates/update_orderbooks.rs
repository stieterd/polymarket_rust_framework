use async_trait::async_trait;
use std::sync::{Arc, RwLock};

use crate::{
    exchange_listeners::{
        orderbooks::poly_orderbook::OrderBook,
        poly_models::{LegacyPriceChange, Listener, PriceChange},
    },
    strategies::Strategy,
};

pub struct UpdateOrderbookStrategy;

impl UpdateOrderbookStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Strategy for UpdateOrderbookStrategy {
    fn name(&self) -> &'static str {
        "UpdateOrderbooks"
    }

    async fn poly_handle_market_agg_orderbook(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _snapshot: &crate::exchange_listeners::poly_models::AggOrderbook,
    ) {
        let asset_id = _snapshot.asset_id.clone();
        let orderbook = OrderBook::new(_snapshot);
        _ctx.poly_state
            .orderbooks
            .insert(asset_id, Arc::new(RwLock::new(orderbook)));
    }

    async fn poly_handle_market_price_change(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: crate::exchange_listeners::poly_models::Listener,
        _payload: &PriceChange,
    ) {
        if let Some(orderbook_entry) = _ctx.poly_state.orderbooks.get(&_payload.asset_id) {
            if let Ok(book) = orderbook_entry.write() {
                // Pass an epoch timestamp string as the second argument.
                // For now, use chrono to get the current epoch as a string.
                let now_epoch = chrono::Utc::now().timestamp().to_string();
                book.apply_price_change(_payload, &now_epoch);
            }
        }
    }

    async fn poly_handle_market_tick_size_change(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TickSizeChangePayload,
    ) {
        if let Some(orderbook_entry) = _ctx.poly_state.orderbooks.get(&_payload.asset_id) {
            if let Ok(mut book) = orderbook_entry.write() {
                book.set_tick_size(_payload.new_tick_size.clone());
            }
        }
    }
}
