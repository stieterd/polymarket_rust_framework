use async_trait::async_trait;
use std::sync::Arc;

use crate::exchange_listeners::crypto_models::{CryptoPriceUpdate, RateKind};
use crate::exchange_listeners::orderbooks::{CryptoOrderbook, OrderbookDepth, OrderbookLevel};
use crate::exchange_listeners::poly_models::{LegacyPriceChange, Listener, PriceChange};
use crate::exchange_listeners::{
    poly_models::{
        AggOrderbook, OrderPayload, PriceChangePayload, TickSizeChangePayload, TradePayload,
    },
    AppState, Crypto, Exchange, Instrument, PolyMarketState,
};

#[derive(Clone)]
pub struct StrategyContext {
    pub app_state: Arc<AppState>,
    pub poly_state: Arc<PolyMarketState>,
}

impl StrategyContext {
    pub fn new(app_state: Arc<AppState>, poly_state: Arc<PolyMarketState>) -> Self {
        Self {
            app_state,
            poly_state,
        }
    }
}

#[async_trait]
pub trait Strategy: Send + Sync {
    fn name(&self) -> &'static str;

    // Gets called by market socket on a market trade
    async fn poly_handle_market_agg_orderbook(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _snapshot: &AggOrderbook,
    ) {
    }

    // Gets called by market socket on a new limit order placement
    async fn poly_handle_market_price_change(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &PriceChange,
    ) {
    }

    // Gets called by market socket whenever the tick size changes
    async fn poly_handle_market_tick_size_change(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &TickSizeChangePayload,
    ) {
    }

    // Gets called by market socket on a pong message
    async fn poly_handle_market_pong(&self, _ctx: &StrategyContext, _listener: Listener) {}

    // Gets called by user socket on a pong message
    async fn poly_handle_user_pong(&self, _ctx: &StrategyContext, _listener: Listener) {}

    // Gets called by user socket on a new trade message
    async fn poly_handle_user_trade(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &TradePayload,
    ) {
    }

    // Gets called by user socket on a new placed order message [LIVE, CANCELLED]
    async fn poly_handle_user_order(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &OrderPayload,
    ) {
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
    }

    async fn crypto_handle_l2_snapshot(
        &self,
        _ctx: &StrategyContext,
        _exchange: Exchange,
        _instrument: Instrument,
        _crypto: Crypto,
        _bids: &[OrderbookLevel],
        _asks: &[OrderbookLevel],
    ) {
    }

    async fn crypto_handle_l2_update(
        &self,
        _ctx: &StrategyContext,
        _exchange: Exchange,
        _instrument: Instrument,
        _crypto: Crypto,
        _bids: &[OrderbookLevel],
        _asks: &[OrderbookLevel],
    ) {
    }

    async fn crypto_handle_price_clear(
        &self,
        _ctx: &StrategyContext,
        _exchange: Exchange,
        _instrument: Instrument,
        _crypto: Crypto,
        _depth: OrderbookDepth,
    ) {
    }
}
