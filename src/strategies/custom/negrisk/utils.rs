#[derive(Debug, Clone)]
pub struct MarketMakingCalculated {
    pub k_1_no_empty: i32,

    pub bot_best_bid: i32,
    pub market_best_bid: i32,

    pub price_to_buy: i32,
    pub size_to_buy: i32,
    pub market_name: String,
    pub asset_id: String,

    pub tick_size: String,
}
