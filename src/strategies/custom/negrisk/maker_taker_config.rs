// Orderbook configuration mirrors the legacy market-making bot defaults.
pub const IGNORING_VOLUME: u32 = 2000 * 1000;
pub const SEARCH_DEPTH: u32 = 30;

// Main configuration
pub const MARKET_LIMIT: u32 = 30 * 1000;

pub const ALPHA: u32 = 2;
pub const SECONDARY_ALPHA: u32 = 2;

pub const SMALL_MARKET_AMOUNT: i32 = 4 * 1000;
pub const MAX_SMALL_MARKET_VOLUME: i32 = 20000 * 1000;
pub const MAX_VOLUME: i32 = 20000 * 1000;

pub const MIN_TAKER_VOLUME: u32 = 300 * 1000;

// Maker leg quotes
pub const REFRESH_TIME: u64 = 3 * 60 * 60;
pub const RATE_LIMIT: u128 = 200; // milliseconds between orders

pub const MIN_AUTO_BUY_VOLUME: i32 = 20 * 1000;
pub const MIDPOINT_DISTANCE: i32 = 40;
