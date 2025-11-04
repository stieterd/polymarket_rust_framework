use log::{error, info};
use std::sync::Arc;

use crate::{
    exchange_listeners::{
        poly_models::{Listener, OrderSide, PriceChange},
    },
    strategies::{Strategy, StrategyContext, strategy_utils::{parse_millis, StrategyAsset, StrategyClient}},
};

pub struct KoenStrategy {
    max_spread: f64,
    price_lower_bound: f64,
    price_upper_bound: f64,
    hedging_cost: f64,
    // Linear model weights (dummy parameters for now)
    model_coef_gap: f64,
    model_coef_f: f64,
    model_coef_swmid_final: f64,
}

impl KoenStrategy {
    pub fn new() -> Self {
        Self {
            max_spread: 0.10, // Maximum spread (10%)
            price_lower_bound: 0.05, // Lower price bound (0.01)
            price_upper_bound: 0.95, // Upper price bound (0.99)
            hedging_cost: 0.001, // Hedging cost (0.5%)
            // Dummy linear model coefficients
            model_coef_gap: 0.1,
            model_coef_f: 0.2,
            model_coef_swmid_final: 0.15,
        }
    }
}

impl Strategy for KoenStrategy {
    fn name(&self) -> &'static str {
        "KoenStrategy"
    }

    fn poly_handle_market_price_change(
        &self,
        ctx: Arc<StrategyContext>,
        _listener: Listener,
        _payload: &PriceChange,
    ) {
        let asset_id = &_payload.asset_id;

        if let Some(orderbook_entry) = ctx.poly_state.orderbooks.get(asset_id) {
            if let Ok(orderbook) = orderbook_entry.read() {
                // Parse the incoming price
                let price_u32 = match parse_millis(&_payload.price) {
                    Ok(price) => price,
                    Err(err) => {
                        error!(
                            "[{}] Failed to parse price '{}' for {}: {}",
                            self.name(), _payload.price, asset_id, err
                        );
                        return;
                    }
                };

                // Only process if this price change affects top of book
                let (best_bid_price, best_ask_price) = match (orderbook.best_bid(), orderbook.best_ask()) {
                    (Some((bid_p, _)), Some((ask_p, _))) => (bid_p, ask_p),
                    _ => return, // No valid top of book
                };

                if price_u32 != best_bid_price && price_u32 != best_ask_price {
                    return;
                }

                // Calculate and check mid_price early to avoid unnecessary work
                let bid_price_f = best_bid_price as f64 / 1000.0;
                let ask_price_f = best_ask_price as f64 / 1000.0;
                let mid_price = (bid_price_f + ask_price_f) / 2.0;

                // Check price is within bounds
                if mid_price < self.price_lower_bound || mid_price > self.price_upper_bound {
                    return;
                }

                // Extract top 2 bids and asks
                let b1 = orderbook.best_bid();           // Best bid (b1_price, b1_volume)
                let a1 = orderbook.best_ask();           // Best ask (a1_price, a1_volume)

                if let (Some(b1), Some(a1)) = (b1, a1) {
                    let (b1_price, b1v) = b1;
                    let (a1_price, a1v) = a1;
                    
                    // Extract second best bid and ask by sorting the maps
                    let mut bids_sorted: Vec<(u32, u32)> = orderbook.get_bid_map()
                        .iter()
                        .map(|entry| (*entry.key(), *entry.value()))
                        .collect();
                    bids_sorted.sort_by(|a, b| b.cmp(a)); // Sort descending (best first)
                    
                    let mut asks_sorted: Vec<(u32, u32)> = orderbook.get_ask_map()
                        .iter()
                        .map(|entry| (*entry.key(), *entry.value()))
                        .collect();
                    asks_sorted.sort(); // Sort ascending (best first)
                    
                    // Check that second best bids and asks exist
                    let (b2_price, b2v) = match bids_sorted.get(1) {
                        Some(entry) => *entry,
                        None => return, // No second best bid
                    };
                    let (a2_price, a2v) = match asks_sorted.get(1) {
                        Some(entry) => *entry,
                        None => return, // No second best ask
                    };

                    // Convert to f64 (divide by 1000 to get back to decimal)
                    let b1_price_f = b1_price as f64 / 1000.0;
                    let b1v_f = b1v as f64 / 1000.0;
                    let a1_price_f = a1_price as f64 / 1000.0;
                    let a1v_f = a1v as f64 / 1000.0;
                    let b2_price_f = b2_price as f64 / 1000.0;
                    let b2v_f = b2v as f64 / 1000.0;
                    let a2_price_f = a2_price as f64 / 1000.0;
                    let a2v_f = a2v as f64 / 1000.0;

                    // Calculate spread
                    let spread = a1_price_f - b1_price_f;

                    // Check spread is within limit
                    if spread > self.max_spread {
                        return;
                    }

                    // Calculate intermediate variables
                    let ratio = ((1.0 + b1v_f) / (1.0 + a1v_f)).ln();
                    let swmid = (b1_price_f * a1v_f + a1_price_f * b1v_f) / (a1v_f + b1v_f);
                    let swmid_diff = swmid - mid_price;
                    let swmid_f1 = swmid_diff / spread;
                    let swmid_final = (1.0 / 3.0) * (swmid_diff + swmid_f1 + ratio);

                    // Calculate f1 and f2
                    let f1 = (b2v_f / (b1v_f + a1v_f)).asinh();
                    let f2 = (a2v_f / (b1v_f + a1v_f)).asinh();

                    // Calculate f
                    let f = f1 - f2;

                    // Calculate gaps
                    let gap_1 = a2_price_f - a1_price_f;
                    let gap_2 = b1_price_f - b2_price_f;
                    let gap = (gap_1 - gap_2).clamp(-0.1, 0.1);

                    // Log the extracted features
                    info!(
                        "[{}] {} - B1: {:.3}x{:.3}, B2: {:.3}x{:.3}, A1: {:.3}x{:.3}, A2: {:.3}x{:.3}, f: {:.6}, gap: {:.6}, swmid_final: {:.6}",
                        self.name(), asset_id,
                        b1_price_f, b1v_f, b2_price_f, b2v_f,
                        a1_price_f, a1v_f, a2_price_f, a2v_f,
                        f, gap, swmid_final
                    );

                    // Run linear regression model to predict return
                    let predicted_delta = self.model_coef_gap * gap 
                        + self.model_coef_f * f 
                        + self.model_coef_swmid_final * swmid_final;
                    
                    // Calculate predicted price
                    let predicted_price = mid_price + predicted_delta;

                    // Check buy signal: predicted price > ask + hedging_cost
                    if predicted_price > a1_price_f + self.hedging_cost {
                        let size = 100.0; // 100 shares in decimal
                        let price = a1_price_f;
                        let negrisk = StrategyAsset::is_negrisk(&ctx, asset_id);
                        let tick_size = orderbook.get_tick_size();

                        info!(
                            "[{}] BUY signal - Predicted: {:.6}, Ask: {:.6}, Delta: {:.6}",
                            self.name(), predicted_price, a1_price_f, predicted_delta
                        );

                        if let Err(err) = StrategyClient::place_limit_order(
                            Arc::clone(&ctx),
                            asset_id,
                            OrderSide::Buy,
                            (price * 1000.0_f64).round() as u32,
                            (size * 1000.0_f64).round() as u32,
                            tick_size,
                            negrisk,
                        ) {
                            error!(
                                "[{}] Failed to place BUY order: {}",
                                self.name(), err
                            );
                        }
                    }

                    // Check sell signal: predicted price < bid - hedging_cost
                    if predicted_price < b1_price_f - self.hedging_cost {
                        let size = 100.0; // 100 shares in decimal
                        let price = b1_price_f;
                        let negrisk = StrategyAsset::is_negrisk(&ctx, asset_id);
                        let tick_size = orderbook.get_tick_size();

                        info!(
                            "[{}] SELL signal - Predicted: {:.6}, Bid: {:.6}, Delta: {:.6}",
                            self.name(), predicted_price, b1_price_f, predicted_delta
                        );

                        if let Err(err) = StrategyClient::place_limit_order(
                            Arc::clone(&ctx),
                            asset_id,
                            OrderSide::Sell,
                            (price * 1000.0_f64).round() as u32,
                            (size * 1000.0_f64).round() as u32,
                            tick_size,
                            negrisk,
                        ) {
                            error!(
                                "[{}] Failed to place SELL order: {}",
                                self.name(), err
                            );
                        }
                    }
                }
            }
        }
    }
}
