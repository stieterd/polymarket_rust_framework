use crate::config::RATE_LIMIT_WAIT_TIME;

use super::orderbooks::poly_orderbook::OrderBook;
use dashmap::DashMap;
use serde::Deserialize;
use std::{
    fmt,
    sync::{Arc, Mutex, RwLock},
};

// 300 requests per 10 seconds
#[derive(Debug, Clone)]
pub struct RateLimit {
    pub timestamp: u128,
    pub wait_time: u32,
}
impl Default for RateLimit {
    fn default() -> Self {
        Self {
            timestamp: 0,
            wait_time: RATE_LIMIT_WAIT_TIME, // amount of milliseconds between requests
        }
    }
}

impl RateLimit {
    pub fn should_wait(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        now - self.timestamp < self.wait_time as u128
    }

    pub fn update_timestamp(&mut self) {
        self.timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Listener {
    PolyMarket,
    PolyMarketLegacy,
    PolyUser,
    PolyUserLegacy,
}

impl Listener {
    pub const fn as_str(self) -> &'static str {
        match self {
            Listener::PolyMarket => "PolyMarket_Market",
            Listener::PolyMarketLegacy => "PolyMarket_Market_Legacy",
            Listener::PolyUser => "PolyMarket_User",
            Listener::PolyUserLegacy => "PolyMarket_User_Legacy",
        }
    }

    pub const fn is_legacy(self) -> bool {
        matches!(self, Listener::PolyMarketLegacy | Listener::PolyUserLegacy)
    }

    pub const fn is_market(self) -> bool {
        matches!(self, Listener::PolyMarketLegacy | Listener::PolyMarket)
    }

    pub const fn is_user(self) -> bool {
        matches!(self, Listener::PolyUser | Listener::PolyUserLegacy)
    }
}

impl std::fmt::Display for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Position {
    pub asset_id: String,
    pub size: u32,
}

impl Position {
    pub fn new(asset_id: impl Into<String>, size: u32) -> Self {
        Self {
            asset_id: asset_id.into(),
            size,
        }
    }
}

impl Default for Position {
    fn default() -> Self {
        Self {
            asset_id: String::new(),
            size: 0,
        }
    }
}

// --- WebSocket Subscription Structures (Unchanged) ---
#[derive(serde::Serialize)]
pub struct SubscriptionRequest<'a> {
    pub action: &'a str,
    pub subscriptions: Vec<Subscription<'a>>,
}
#[derive(serde::Serialize)]
pub struct Subscription<'a> {
    pub topic: &'a str,
    #[serde(rename = "type")]
    pub sub_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clob_auth: Option<ClobAuth<'a>>,
}
#[derive(serde::Serialize)]
pub struct ClobAuth<'a> {
    pub key: &'a str,
    pub secret: &'a str,
    pub passphrase: &'a str,
}

// --- Incoming WebSocket Message Structures (Unchanged) ---
#[derive(Deserialize, Debug)]
pub struct PolymarketMessageWrapper {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub payload: simd_json::value::owned::Value,
    pub topic: Option<String>,
    pub connection_id: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct PolymarketMessageWrapperOld {
    pub event_type: String,
    #[serde(default)]
    pub timestamp: Option<String>,
    #[serde(default)]
    pub old_tick_size: Option<String>,
    #[serde(default)]
    pub new_tick_size: Option<String>,
    #[serde(default)]
    pub market: Option<String>,
    #[serde(default)]
    pub asset_id: Option<String>,
    #[serde(default)]
    pub hash: Option<String>,
    #[serde(default)]
    pub price_changes: Vec<LegacyPriceChange>,
    #[serde(default)]
    pub bids: Vec<OrderbookEntry>,
    #[serde(default)]
    pub asks: Vec<OrderbookEntry>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LegacyPriceChange {
    pub asset_id: String,
    pub price: String,
    pub size: String,
    pub side: String,
    pub hash: Option<String>,
    pub best_bid: Option<String>,
    pub best_ask: Option<String>,
}

// --- Market Data Structures (Unchanged) ---
#[derive(Deserialize, Debug, Clone)]
pub struct OrderbookEntry {
    pub price: String,
    pub size: String,
}
#[derive(Deserialize, Debug, Clone)]
pub struct AggOrderbook {
    pub asset_id: String,
    pub bids: Vec<OrderbookEntry>,
    pub asks: Vec<OrderbookEntry>,
    pub timestamp: String,
    pub hash: String,
}
#[derive(Deserialize, Debug, Clone)]
pub struct PriceChangePayload {
    pub pc: Vec<PriceChange>,
    #[serde(rename = "t")]
    pub timestamp: String,
}
#[derive(Deserialize, Debug, Clone)]
pub struct PriceChange {
    #[serde(rename = "a")]
    pub asset_id: String,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "s")]
    pub size: String,
    #[serde(rename = "si")]
    pub side: String,
    // pub hash: String,
    // pub best_bid: String,
    // pub best_ask: String
}
#[derive(Deserialize, Debug, Clone)]
pub struct TickSizeChangePayload {
    pub asset_id: String,
    pub new_tick_size: String,
}

// --- User Data Structures (Corrected) ---

#[derive(Deserialize, Debug)]
pub struct TradePayload {
    pub asset_id: String,
    pub event_type: String,
    pub id: String,
    pub last_update: String,
    pub maker_orders: Vec<MakerOrder>,
    pub market: String,
    #[serde(rename = "match_time")]
    pub match_time: String,
    pub outcome: String,
    pub owner: String,
    pub price: String,
    pub side: String,
    pub size: String,
    pub status: TradeStatus,
    pub taker_order_id: String,
    pub timestamp: String,
    #[serde(rename = "trader_side")]
    pub trade_role: TradeRole,
    pub trade_owner: String,
    #[serde(rename = "type")]
    pub message_type: String,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeStatus {
    Matched,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TradeRole {
    Taker,
    Maker,
    #[serde(other)]
    Unknown,
}

#[derive(Deserialize, Debug)]
pub struct MakerOrder {
    pub maker_address: String,
    pub order_id: String,
    pub asset_id: String,
    pub price: String,
    pub matched_amount: String,
    pub outcome: String,
    pub side: String,
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderEventType {
    PLACEMENT,
    UPDATE,
    CANCELLATION,
}

impl OrderEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            OrderEventType::PLACEMENT => "PLACEMENT",
            OrderEventType::UPDATE => "UPDATE",
            OrderEventType::CANCELLATION => "CANCELLATION",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "PLACEMENT" => Some(OrderEventType::PLACEMENT),
            "UPDATE" => Some(OrderEventType::UPDATE),
            "CANCELLATION" => Some(OrderEventType::CANCELLATION),
            _ => None,
        }
    }
}

impl fmt::Display for OrderEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AssetSide {
    YES,
    NO,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub const fn as_str(self) -> &'static str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        match value {
            "BUY" => Some(OrderSide::Buy),
            "SELL" => Some(OrderSide::Sell),
            _ => None,
        }
    }
}

impl fmt::Display for OrderSide {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Deserialize, Debug)]
pub struct OrderPayload {
    pub id: String,
    pub asset_id: String,
    pub associate_trades: Vec<String>,
    pub event_type: String,
    pub market: String,
    pub order_owner: String,
    #[serde(rename = "type")]
    pub order_event_type: String,
    pub outcome: String,
    pub owner: String,
    pub price: String,
    pub side: String,
    pub original_size: String,
    pub size_matched: String,
    pub timestamp: String,
    pub status: String,
}

#[derive(Debug, Clone)]
pub struct AssetOrders {
    pub bids: Arc<DashMap<(u32, u32), Arc<Mutex<OpenOrder>>>>,
    pub asks: Arc<DashMap<(u32, u32), Arc<Mutex<OpenOrder>>>>,
}

impl AssetOrders {
    pub fn new(
        bids: DashMap<(u32, u32), Arc<Mutex<OpenOrder>>>,
        asks: DashMap<(u32, u32), Arc<Mutex<OpenOrder>>>,
    ) -> Self {
        Self {
            bids: Arc::new(bids),
            asks: Arc::new(asks),
        }
    }

    pub fn order_exists(&self, side: OrderSide, price: u32, size: u32) -> bool {
        let key = (price, size);
        match side {
            OrderSide::Buy => self.bids.contains_key(&key),
            OrderSide::Sell => self.asks.contains_key(&key),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderState {
    Live,
    Unconfirmed,
    ToBeCanceled,
}

#[derive(Debug, Clone)]
pub struct OpenOrder {
    id: Option<String>,
    asset: String,
    state: OrderState,
    price: u32,
    size: u32,
    size_filled: u32,
}

impl OpenOrder {
    pub fn new(asset: String, price: u32, size: u32, size_filled: u32, id: Option<String>) -> Self {
        let mut order = Self {
            id: None,
            asset,
            state: OrderState::Unconfirmed,
            price,
            size,
            size_filled,
        };
        order.set_id(id);
        order
    }

    pub fn id(&self) -> Option<&String> {
        self.id.as_ref()
    }

    pub fn asset(&self) -> &str {
        &self.asset
    }

    pub fn state(&self) -> OrderState {
        self.state
    }

    pub fn price(&self) -> u32 {
        self.price
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn size_filled(&self) -> u32 {
        self.size_filled
    }

    pub fn set_id(&mut self, id: Option<String>) {
        self.id = id;
        self.state = if self.id.is_some() {
            OrderState::Live
        } else {
            OrderState::Unconfirmed
        };
    }

    pub fn set_state(&mut self, state: OrderState) {
        self.state = state;
    }

    pub fn set_size_filled(&mut self, size_filled: u32) {
        self.size_filled = size_filled;
    }
}




#[derive(Debug, Deserialize)]
struct ApiPosition {
    asset: String,
    size: String,
}

fn parse_position_size(size: &str) -> Option<u32> {
    size
        .parse::<f64>()
        .ok()
        .map(|value| (value * 1000.0).round() as u32)
}

pub async fn get_positions(
    user: &str,
) -> DashMap<String, Arc<RwLock<Position>>> {
    let client = reqwest::Client::new();
    let positions = DashMap::new();
    let mut offset = 0;
    let mut position_length = 500;
    while position_length >= 500 {
        let url = format!(
            "https://data-api.polymarket.com/positions?user={}&limit=500&offset={}",
            user, offset
        );
        let resp = client.get(&url).send().await;
        let returned_positions: Vec<ApiPosition> = match resp {
            Ok(r) => match r.json::<Vec<ApiPosition>>().await {
                Ok(json) => json,
                Err(_) => break,
            },
            Err(_) => break,
        };
        position_length = returned_positions.len();
        offset += position_length;
        for pos in returned_positions {
            if let Some(size) = parse_position_size(&pos.size) {
                let asset_id = pos.asset;
                let position = Arc::new(RwLock::new(Position::new(asset_id.clone(), size)));
                positions.insert(asset_id, position);
            }
        }
    }
    positions
}
