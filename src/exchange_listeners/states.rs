use dashmap::DashMap;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use crate::{
    exchange_listeners::{
        crypto_models::CryptoPrice,
        orderbooks::{
            poly_orderbook::{OrderBook, OrderBookSnapshot},
            CryptoOrderbook, OrderbookDepth,
        },
        poly_client::PolyClient,
        poly_models::{AssetOrders, OpenOrder, Position, RateLimit},
        Exchange, Instrument,
    },
    marketmaking::poly_market_struct::Market,
};

// --- Shared State Structure (Unchanged) ---
#[derive(Debug, Default)]
pub struct PolyMarketState {
    pub orderbooks: Arc<DashMap<String, Arc<RwLock<OrderBook>>>>,
    pub prev_orderbooks: Arc<DashMap<String, OrderBookSnapshot>>,
    pub positions: Arc<DashMap<String, Arc<RwLock<Position>>>>,
    pub open_orders: Arc<DashMap<String, AssetOrders>>,
    pub markets: Arc<HashMap<String, Arc<Market>>>,
    pub rate_limit: Arc<RwLock<RateLimit>>,
}

/// The main application state, holding final, converted USDT prices.
#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub btc_orderbooks: Arc<DashMap<(Exchange, Instrument, OrderbookDepth), CryptoOrderbook>>,
    // pub eth_orderbooks: Arc<DashMap<(Exchange, Instrument, OrderbookDepth), CryptoOrderbook>>,
    // pub xrp_orderbooks: Arc<DashMap<(Exchange, Instrument, OrderbookDepth), CryptoOrderbook>>,
    // pub sol_orderbooks: Arc<DashMap<(Exchange, Instrument, OrderbookDepth), CryptoOrderbook>>,
    pub btc_prices: Arc<DashMap<(Exchange, Instrument, OrderbookDepth), CryptoPrice>>,
}
