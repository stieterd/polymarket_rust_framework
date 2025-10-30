use std::sync::{Arc, RwLock};

use crate::{
    exchange_listeners::{orderbooks::poly_orderbook::OrderBook, poly_models::Listener},
    strategies::Strategy,
};

pub struct HourlyBtcStrategy;

impl HourlyBtcStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Strategy for HourlyBtcStrategy {
    fn name(&self) -> &'static str {
        "UpdateOrderbooks"
    }

    fn poly_handle_market_agg_orderbook(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _snapshot: &crate::exchange_listeners::poly_models::AggOrderbook,
    ) {
    }

    fn poly_handle_market_price_change(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: crate::exchange_listeners::poly_models::Listener,
        _payload: &crate::exchange_listeners::poly_models::PriceChange,
    ) {
    }
}
