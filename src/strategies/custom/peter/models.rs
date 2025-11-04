pub const MAX_VOLUME: u32 = 2000_000;
pub const TARGET_ORDER_SIZE: u32 = 200_000;
pub struct OrderBookContext {
    pub midpoint: u32,
    pub spread: u32,
}
