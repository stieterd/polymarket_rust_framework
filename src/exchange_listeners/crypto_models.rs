use crate::exchange_listeners::orderbooks::{CryptoOrderbook, OrderbookDepth};
use crate::exchange_listeners::AppState;
use atomic_float::AtomicF64;
use dashmap::DashMap;
use serde::Deserialize;
use simd_json;
use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub fn get_crypto_orderbook_map(
    app_state: Arc<AppState>,
    crypto: Crypto,
) -> Arc<dashmap::DashMap<(Exchange, Instrument, OrderbookDepth), CryptoOrderbook>> {
    match crypto {
        Crypto::BTC => app_state.btc_orderbooks.clone(),
        // Crypto::ETH => &self.app_state.eth_orderbooks,
        // Crypto::XRP => &self.app_state.xrp_orderbooks,
        // Crypto::SOL => &self.app_state.sol_orderbooks,
        _ => app_state.btc_orderbooks.clone(),
    }
}

pub fn get_crypto_prices_map(
    app_state: Arc<AppState>,
    crypto: Crypto,
) -> Arc<dashmap::DashMap<(Exchange, Instrument, OrderbookDepth), CryptoPrice>> {
    match crypto {
        Crypto::BTC => app_state.btc_prices.clone(),
        // Crypto::ETH => &self.app_state.eth_orderbooks,
        // Crypto::XRP => &self.app_state.xrp_orderbooks,
        // Crypto::SOL => &self.app_state.sol_orderbooks,
        _ => app_state.btc_prices.clone(),
    }
}

#[derive(Debug, Clone, Copy)]
pub enum RateKind {
    UsdcUsdtBinance,
    UsdUsdtCoinbase,
}

#[derive(Debug, Clone, Copy)]
pub struct CryptoPrice {
    pub midpoint: f64,
    pub price: f64,
}

impl CryptoPrice {
    pub fn new() -> Self {
        Self {
            midpoint: 0.0,
            price: 0.0,
        }
    }
}

// --- Core Data Structures ---
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub enum Crypto {
    BTC,
    ETH,
    XRP,
    SOL,
}

impl fmt::Display for Crypto {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub enum Exchange {
    Binance,
    CoinbaseLegacy,
    CoinbaseAdvanced,
    Bitstamp,
    Deribit,
    Bitmex,
    Bybit,
    Okx,
    Kraken,
}

impl Exchange {
    pub const fn as_str(self) -> &'static str {
        match self {
            Exchange::Binance => "Binance",
            Exchange::CoinbaseLegacy => "CoinbaseLegacy",
            Exchange::CoinbaseAdvanced => "CoinbaseAdvanced",
            Exchange::Bitstamp => "Bitstamp",
            Exchange::Deribit => "Deribit",
            Exchange::Bitmex => "Bitmex",
            Exchange::Bybit => "Bybit",
            Exchange::Okx => "Okx",
            Exchange::Kraken => "Kraken",
        }
    }
}

impl std::fmt::Display for Exchange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Hash)]
pub enum Instrument {
    Spot,
    Perpetual,
}

#[derive(Deserialize, Clone, Debug)]
pub struct CryptoPriceUpdate {
    pub symbol: Option<String>,
    pub best_bid_price: f64,
    pub best_bid_vol: f64,
    pub best_ask_price: f64,
    pub best_ask_vol: f64,
}

// --- WebSocket Message Structs (Unchanged) ---
// ... (all message structs remain the same)
#[derive(Deserialize)]
pub struct BinanceBookTicker {
    #[serde(rename = "u")]
    pub update_id: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "b")]
    pub best_bid: String,
    #[serde(rename = "B")]
    pub best_bid_qty: String,
    #[serde(rename = "a")]
    pub best_ask: String,
    #[serde(rename = "A")]
    pub best_ask_qty: String,
}

#[derive(Deserialize)]
pub struct CoinbaseTicker<'a> {
    pub product_id: &'a str,
    pub price: Option<&'a str>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct CoinbaseHeartbeat<'a> {
    pub product_id: &'a str,
    pub sequence: i64,
    pub time: &'a str,
}
#[derive(Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum CoinbaseLegacyMsg<'a> {
    Ticker(CoinbaseTicker<'a>),
    Heartbeat(CoinbaseHeartbeat<'a>),
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct CoinbaseAdvancedUpdate<'a> {
    pub side: &'a str,
    pub price_level: &'a str,
    pub new_quantity: &'a str,
}
#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct CoinbaseAdvancedEvent<'a> {
    #[serde(rename = "type")]
    pub event_type: &'a str,
    pub updates: Vec<CoinbaseAdvancedUpdate<'a>>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct CoinbaseL2DataMsg<'a> {
    pub events: Vec<CoinbaseAdvancedEvent<'a>>,
}
#[derive(Deserialize)]
#[allow(dead_code)]
#[serde(bound(deserialize = "'de: 'a"))]
pub struct CoinbaseHeartbeatsMsg<'a> {
    pub client_id: &'a str,
    pub timestamp: &'a str,
    pub sequence_num: i64,
}
#[derive(Deserialize)]
#[serde(tag = "channel")]
#[serde(rename_all = "snake_case")]
#[serde(bound(deserialize = "'de: 'a"))]
pub enum CoinbaseAdvancedMsg<'a> {
    L2Data(CoinbaseL2DataMsg<'a>),
    Heartbeats(CoinbaseHeartbeatsMsg<'a>),
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
pub struct DeribitBookData {
    pub bids: Vec<(String, f64, f64)>,
    pub asks: Vec<(String, f64, f64)>,
    pub prev_change_id: Option<u64>,
    pub change_id: u64,
}
#[derive(Deserialize, Debug)]
pub struct DeribitSubscriptionParams {
    pub data: DeribitBookData,
}
#[derive(Deserialize, Debug)]
pub struct DeribitSubscriptionMsg {
    pub params: DeribitSubscriptionParams,
}
#[derive(Deserialize, Debug)]
pub struct DeribitHeartbeatParams {
    #[serde(rename = "type")]
    pub heartbeat_type: String,
}
#[derive(Deserialize, Debug)]
pub struct DeribitHeartbeatMsg {
    pub params: DeribitHeartbeatParams,
}
#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum DeribitMsg {
    // This now correctly handles the .raw stream's data structure.
    Subscription(DeribitSubscriptionMsg),
    Heartbeat(DeribitHeartbeatMsg),
    Other(simd_json::value::owned::Value),
}

#[derive(Deserialize)]
pub struct BitstampData {
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}
#[derive(Deserialize)]
pub struct BitstampBook {
    pub data: BitstampData,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum BitstampMsg {
    Data(BitstampBook),
    Other(simd_json::value::owned::Value),
}

#[derive(Deserialize)]
pub struct BitmexQuote {
    #[serde(rename = "bidPrice")]
    pub bid_price: f64,
    #[serde(rename = "askPrice")]
    pub ask_price: f64,
}
#[derive(Deserialize)]
#[allow(dead_code)]
pub struct BitmexData {
    pub table: Option<String>,
    #[serde(default)]
    pub data: Vec<BitmexQuote>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum BitmexMsg {
    Data(BitmexData),
    Other(simd_json::value::owned::Value),
}

#[derive(Deserialize)]
pub struct BybitData {
    #[serde(rename = "b")]
    pub bids: Vec<[String; 2]>,
    #[serde(rename = "a")]
    pub asks: Vec<[String; 2]>,
}
#[derive(Deserialize)]
pub struct BybitBook {
    pub data: BybitData,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum BybitMsg {
    Orderbook(BybitBook),
    Other(simd_json::value::owned::Value),
}

#[derive(Deserialize)]
pub struct OkxTick {
    pub bids: Vec<[String; 4]>,
    pub asks: Vec<[String; 4]>,
}
#[derive(Deserialize)]
pub struct OkxData {
    #[serde(default)]
    pub data: Vec<OkxTick>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum OkxMsg {
    Data(OkxData),
    Other(simd_json::value::owned::Value),
}

#[derive(Deserialize, Debug)]
pub struct KrakenLevel {
    pub price: f64,
    pub qty: f64,
}

#[derive(Deserialize, Debug)]
pub struct KrakenBookSnapshot {
    pub product_id: String,
    pub bids: Vec<KrakenLevel>,
    pub asks: Vec<KrakenLevel>,
}

#[derive(Deserialize, Debug)]
pub struct KrakenBookUpdate {
    pub product_id: String,
    pub side: String,
    pub price: f64,
    pub qty: f64,
}

#[derive(Deserialize, Debug)]
pub struct KrakenSubscribedMsg {
    pub feed: String,
    pub product_ids: Vec<String>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
pub enum KrakenMsg {
    // These variants now contain the field that distinguishes them ("feed" or "event")
    BookSnapshot {
        feed: String, // Expects "book_snapshot"
        #[serde(flatten)]
        data: KrakenBookSnapshot,
    },
    BookUpdate {
        feed: String, // Expects "book"
        #[serde(flatten)]
        data: KrakenBookUpdate,
    },
    Subscribed {
        event: String, // Expects "subscribed"
        #[serde(flatten)]
        data: KrakenSubscribedMsg,
    },
    // Generic struct to catch other event-based messages like info/alerts
    Event {
        event: String,
    },
    // Fallback for any other message shapes
    Other(serde_json::Value),
}
