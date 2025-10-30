use dashmap::DashMap;
use std::sync::{Arc, RwLock};

use crate::exchange_listeners::{
    crypto_models::CryptoPrice,
    orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth},
    poly_models::{AssetOrders, OpenOrder, Position},
    Exchange, Instrument,
};

// --- Shared State Structure (Unchanged) ---
#[derive(Debug, Default)]
pub struct PolyMarketState {
    pub orderbooks: Arc<DashMap<String, Arc<RwLock<OrderBook>>>>,
    pub positions: Arc<DashMap<String, Arc<RwLock<Position>>>>,
    pub open_orders: Arc<DashMap<String, AssetOrders>>,
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
