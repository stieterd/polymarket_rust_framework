use dashmap::DashMap;
use log::{error, info};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::{
    exchange_listeners::poly_models::{Listener, OrderSide, PriceChange},
    strategies::{
        strategy_utils::{parse_millis, StrategyAsset, StrategyClient, StrategyPosition},
        Strategy, StrategyContext,
    },
};

pub struct KoenStrategy {
    max_spread: f64,
    price_lower_bound: f64,
    price_upper_bound: f64,
    predicted_move: f64,
    predicted_move_hedge: f64,
    max_order_size: f64,
    max_counterparty_size: f64,
    min_same_side_liquidity: f64,
    trade_cooldown: Duration,
    cancel_after: Duration,
    last_trade: DashMap<String, Instant>,
}

impl KoenStrategy {
    pub fn new() -> Self {
        Self {
            max_spread: 0.011,
            price_lower_bound: 0.025,
            price_upper_bound: 0.975,
            predicted_move: 0.15,
            predicted_move_hedge: 0.5,
            max_order_size: 50.0,
            max_counterparty_size: 50.0,
            min_same_side_liquidity: 150.0,
            trade_cooldown: Duration::from_secs(1 * 60),
            cancel_after: Duration::from_secs(1),
            last_trade: DashMap::new(),
        }
    }

    fn calc_gap(b1_price: f64, b2_price: f64, a1_price: f64, a2_price: f64) -> f64 {
        let bid_gap = b1_price - b2_price;
        let ask_gap = a2_price - a1_price;
        ask_gap - bid_gap
    }

    fn price_to_int(price: f64) -> u32 {
        (price * 1000.0).round() as u32
    }

    fn size_to_int(size: f64) -> u32 {
        (size * 1000.0).round() as u32
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
                if let Some(entry) = self.last_trade.get(asset_id) {
                    if entry.elapsed() < self.trade_cooldown {
                        return;
                    }
                }
                let price_u32 = match parse_millis(&_payload.price) {
                    Ok(price) => price,
                    Err(err) => {
                        error!(
                            "[{}] Failed to parse price '{}' for {}: {}",
                            self.name(),
                            _payload.price,
                            asset_id,
                            err
                        );
                        return;
                    }
                };

                let (best_bid_price, best_ask_price) =
                    match (orderbook.best_bid(), orderbook.best_ask()) {
                        (Some((bid_p, _)), Some((ask_p, _))) => (bid_p, ask_p),
                        _ => return,
                    };

                if price_u32 != best_bid_price && price_u32 != best_ask_price {
                    return;
                }

                let bid_price_f = best_bid_price as f64 / 1000.0;
                let ask_price_f = best_ask_price as f64 / 1000.0;
                let mid_price = (bid_price_f + ask_price_f) / 2.0;

                if mid_price < self.price_lower_bound || mid_price > self.price_upper_bound {
                    return;
                }

                let b1 = orderbook.best_bid();
                let a1 = orderbook.best_ask();

                if let (Some((b1_price, b1_size)), Some((a1_price, a1_size))) = (b1, a1) {
                    let mut bids_sorted: Vec<(u32, u32)> = orderbook
                        .get_bid_map()
                        .iter()
                        .map(|entry| (*entry.key(), *entry.value()))
                        .collect();
                    bids_sorted.sort_by(|a, b| b.cmp(a));

                    let mut asks_sorted: Vec<(u32, u32)> = orderbook
                        .get_ask_map()
                        .iter()
                        .map(|entry| (*entry.key(), *entry.value()))
                        .collect();
                    asks_sorted.sort();

                    let (b2_price, b2_size) = match bids_sorted.get(1) {
                        Some(entry) => *entry,
                        None => return,
                    };
                    let (a2_price, a2_size) = match asks_sorted.get(1) {
                        Some(entry) => *entry,
                        None => return,
                    };

                    let b1_price_f = b1_price as f64 / 1000.0;
                    let b2_price_f = b2_price as f64 / 1000.0;
                    let a1_price_f = a1_price as f64 / 1000.0;
                    let a2_price_f = a2_price as f64 / 1000.0;
                    let b1_size_f = b1_size as f64 / 1000.0;
                    let b2_size_f = b2_size as f64 / 1000.0;
                    let a1_size_f = a1_size as f64 / 1000.0;
                    let a2_size_f = a2_size as f64 / 1000.0;

                    if a1_size_f >= self.max_counterparty_size {
                        return;
                    }

                    if b1_size_f <= a1_size_f {
                        return;
                    }

                    if b1_size_f < self.min_same_side_liquidity {
                        return;
                    }

                    let spread = a1_price_f - b1_price_f;
                    if spread > self.max_spread {
                        return;
                    }

                    let gap = Self::calc_gap(b1_price_f, b2_price_f, a1_price_f, a2_price_f);
                    if gap == 0.0 {
                        return;
                    }

                    // Check position to determine if we should use hedge parameter
                    let market_assets = StrategyAsset::get_yes_and_no(&ctx, asset_id);
                    let positions = StrategyPosition::assets_position_map(&ctx, &market_assets);
                    let is_yes_market = StrategyAsset::is_yes_market(&ctx, asset_id);

                    // Calculate net position (yes - no)
                    let yes_asset_id = market_assets.get(0);
                    let no_asset_id = market_assets.get(1);
                    let yes_position = yes_asset_id
                        .and_then(|id| positions.get(id))
                        .copied()
                        .unwrap_or(0);
                    let no_position = no_asset_id
                        .and_then(|id| positions.get(id))
                        .copied()
                        .unwrap_or(0);
                    let net_position_yes = yes_position as i32 - no_position as i32;
                    let net_pos_contracts = net_position_yes as f64 / 1000.0;

                    // Determine which predicted_move to use
                    // If we're in no market and have >50 yes shares net, use hedge parameter
                    // (1000 position units = 1 contract, so 50000 = 50 contracts)
                    let use_hedge = if !is_yes_market && net_position_yes > 50000 {
                        true
                    } else if is_yes_market && net_position_yes > 50000 {
                        // If we're in yes market and already long yes, we're not hedging by buying more yes
                        false
                    } else {
                        false
                    };

                    let predicted_move_coef = if use_hedge {
                        self.predicted_move_hedge
                    } else {
                        self.predicted_move
                    };

                    let predicted_delta = gap * predicted_move_coef;
                    let predicted_price = mid_price + predicted_delta;
                    let tick_size = orderbook.get_tick_size();
                    let negrisk = StrategyAsset::is_negrisk(&ctx, asset_id);

                    // Require at least 20 bps (0.002) of edge: predicted price must be at least 0.002 higher than buy price
                    let min_edge = 0.002;
                    if predicted_delta > 0.0 && predicted_price >= a1_price_f && (predicted_price - a1_price_f) >= min_edge {
                        let price_int = Self::price_to_int(a1_price_f);
                        let trade_size = self.max_order_size;
                        if trade_size <= 0.0 {
                            return;
                        }
                        let size_int = Self::size_to_int(trade_size);

                        // Net position of the asset we're buying: if buying YES, it's yes_position; if buying NO, it's -net_position_yes
                        let asset_net_position = if is_yes_market {
                            yes_position as i32
                        } else {
                            -net_position_yes
                        };
                        let asset_net_pos_contracts = asset_net_position as f64 / 1000.0;

                        if let Err(err) = StrategyClient::place_limit_order(
                            Arc::clone(&ctx),
                            asset_id,
                            OrderSide::Buy,
                            price_int,
                            size_int,
                            tick_size,
                            negrisk,
                        ) {
                            error!("[{}] Failed to place BUY order: {}", self.name(), err);
                        } else {
                            let ctx_for_cancel = Arc::clone(&ctx);
                            let asset_for_cancel = asset_id.to_string();
                            let cancel_delay = self.cancel_after;
                            let hedge_flag = if use_hedge { " [HEDGE]" } else { "" };
                            info!(
                                "[{}] BUY executed{} asset={} gap={:.4} mid={:.3} predicted={:.3} size={:.3} asset_net_pos={:.3} asset_net_pos_units={} price={:.3} | bids: {:.3}@{:.3}, {:.3}@{:.3} | asks: {:.3}@{:.3}, {:.3}@{:.3}",
                                self.name(),
                                hedge_flag,
                                asset_id,
                                gap,
                                mid_price,
                                predicted_price,
                                trade_size,
                                asset_net_pos_contracts,
                                asset_net_position,
                                a1_price_f,
                                b1_price_f,
                                b1_size_f,
                                b2_price_f,
                                b2_size_f,
                                a1_price_f,
                                a1_size_f,
                                a2_price_f,
                                a2_size_f
                            );
                            self.last_trade.insert(asset_id.to_string(), Instant::now());
                            tokio::spawn(async move {
                                sleep(cancel_delay).await;
                                let still_open = ctx_for_cancel
                                    .poly_state
                                    .open_orders
                                    .get(&asset_for_cancel)
                                    .map(|orders| {
                                        orders.order_exists(OrderSide::Buy, price_int, size_int)
                                    })
                                    .unwrap_or(false);
                                if still_open {
                                    if let Err(err) = StrategyClient::cancel_orders(
                                        Arc::clone(&ctx_for_cancel),
                                        &asset_for_cancel,
                                        vec![(OrderSide::Buy, price_int, size_int)],
                                    ) {
                                        error!(
                                            "[{}] Failed to cancel stale BUY order for asset {}: {}",
                                            "KoenStrategy", asset_for_cancel, err
                                        );
                                    } else {
                                        info!(
                                            "[KoenStrategy] Canceled unfilled BUY order asset={} price={:.3} size={:.3}",
                                            asset_for_cancel,
                                            price_int as f64 / 1000.0,
                                            size_int as f64 / 1000.0
                                        );
                                    }
                                }
                            });
                        }
                    }
                }
            }
        }
    }
}
