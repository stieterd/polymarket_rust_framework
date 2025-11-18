use dashmap::DashMap;
use log::error;
use std::{
    cmp::min,
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

use crate::{
    clob_client::constants::{FRAC_CENTS, FULL_CENTS},
    exchange_listeners::{
        orderbooks::poly_orderbook::OrderBook,
        poly_models::{Listener, OrderSide, PriceChange},
    },
    
    
    
    strategies::{
        strategy_utils::{StrategyAsset, StrategyClient},
        Strategy, StrategyContext,
    },
    
    strategies::custom::negrisk::maker_taker_config::{
        ALPHA, MARKET_LIMIT, MAX_SMALL_MARKET_VOLUME, MAX_VOLUME, MIN_AUTO_BUY_VOLUME,
        RATE_LIMIT, SECONDARY_ALPHA, SMALL_MARKET_AMOUNT,
    },
    strategies::custom::negrisk::utils::MarketMakingCalculated,
};

pub struct NegRiskNoMakerStrategy {
    event_asset_cache: DashMap<String, Arc<Vec<String>>>,
    last_order_time: Mutex<SystemTime>,
}

impl NegRiskNoMakerStrategy {
    pub fn new() -> Self {
        Self {
            event_asset_cache: DashMap::new(),
            last_order_time: Mutex::new(SystemTime::UNIX_EPOCH),
        }
    }

    fn handle_price_change(&self, ctx: Arc<StrategyContext>, payload: &PriceChange) {
        let asset_id = &payload.asset_id;
        if !StrategyAsset::is_negrisk(ctx.as_ref(), asset_id) {
            return;
        }

        let market = match ctx.poly_state.markets.get(asset_id) {
            Some(market) => Arc::clone(market),
            None => return,
        };

        let neg_risk_assets = match self.resolve_event_assets(ctx.as_ref(), asset_id, &market) {
            Some(assets) => assets,
            None => return,
        };

        let orderbooks = self.collect_orderbook_stats(ctx.as_ref(), &neg_risk_assets);
        if orderbooks.is_empty() {
            return;
        }

        let open_bids = self.collect_open_bid_orders(ctx.as_ref(), asset_id);
        let bot_best_bid = open_bids
            .iter()
            .map(|(price, _)| *price as i32)
            .max()
            .unwrap_or(0);
        let has_open_bids = !open_bids.is_empty();
        let k1_total = Self::compute_k1_total(&orderbooks);

        let slug = market.slug.clone().unwrap_or_default();
        let signal = self.determine_signal(
            &slug,
            asset_id,
            &orderbooks,
            k1_total,
            bot_best_bid,
            has_open_bids,
        );

        let mut open_bids_option = Some(open_bids);
        match signal {
            MarketSignal::Place(calc) => {
                if let Some(open_bids_snapshot) = open_bids_option.take() {
                    self.process_place_signal(Arc::clone(&ctx), calc, open_bids_snapshot);
                }
            }
            MarketSignal::CancelBids => {
                if let Some(open_bids_snapshot) = open_bids_option.take() {
                    self.cancel_bid_orders_with_snapshot(
                        Arc::clone(&ctx),
                        asset_id,
                        open_bids_snapshot,
                    );
                }
            }
            MarketSignal::NoAction => {}
        }
    }

    fn resolve_event_assets(
        &self,
        ctx: &StrategyContext,
        asset_id: &str,
        market: &Arc<crate::marketmaking::poly_market_struct::Market>,
    ) -> Option<Arc<Vec<String>>> {
        let neg_risk_id = market.negRiskMarketID.as_ref()?.clone();
        if let Some(entry) = self.event_asset_cache.get(&neg_risk_id) {
            return Some(Arc::clone(entry.value()));
        }

        let mut asset_ids = HashSet::new();
        for (other_asset, other_market) in ctx.poly_state.markets.iter() {
            if other_market
                .negRiskMarketID
                .as_ref()
                .map(|id| id == &neg_risk_id)
                .unwrap_or(false)
            {
                asset_ids.insert(other_asset.clone());
            }
        }

        if !asset_ids.contains(asset_id) || asset_ids.is_empty() {
            return None;
        }

        let assets_vec: Vec<String> = asset_ids.into_iter().collect();
        let assets_arc = Arc::new(assets_vec);
        self.event_asset_cache
            .insert(neg_risk_id, Arc::clone(&assets_arc));
        Some(assets_arc)
    }

    fn collect_orderbook_stats(
        &self,
        ctx: &StrategyContext,
        asset_ids: &Vec<String>,
    ) -> Vec<OrderBookStats> {
        let mut stats = Vec::with_capacity(asset_ids.len());
        for asset_id in asset_ids {
            if let Some(orderbook_ref) = ctx.poly_state.orderbooks.get(asset_id) {
                if let Ok(orderbook) = orderbook_ref.value().read() {
                    stats.push(OrderBookStats::from_orderbook(asset_id, &orderbook));
                }
            }
        }
        stats
    }

    fn collect_open_bid_orders(&self, ctx: &StrategyContext, asset_id: &str) -> Vec<(u32, u32)> {
        ctx.poly_state
            .open_orders
            .get(asset_id)
            .map(|orders| {
                orders
                    .bids
                    .iter()
                    .map(|entry| *entry.key())
                    .collect::<Vec<(u32, u32)>>()
            })
            .unwrap_or_default()
    }

    fn compute_k1_total(orderbooks: &[OrderBookStats]) -> i32 {
        let mut k1 = -1000;
        for ob in orderbooks {
            if ob.best_feasible_price() < 1000 {
                k1 += 1000;
            }
        }
        k1
    }

    fn determine_signal(
        &self,
        slug: &str,
        asset_id: &str,
        orderbooks: &[OrderBookStats],
        k1_total: i32,
        bot_best_bid: i32,
        has_open_bids: bool,
    ) -> MarketSignal {
        if k1_total <= 0 {
            return Self::cancel_signal(has_open_bids);
        }

        let relevant_books = if Self::should_apply_lowest_volume(slug) {
            Self::apply_lowest_volume(orderbooks, asset_id)
        } else {
            orderbooks.to_vec()
        };

        let mut sum_best_asks = 0_i32;
        let mut k1_no_empty = -1000;
        let mut volumes = HashMap::new();
        for ob in &relevant_books {
            let price = ob.best_feasible_price();
            if price < 1000 {
                sum_best_asks += price as i32;
                k1_no_empty += 1000;
                volumes.insert(ob.asset_id.clone(), ob.best_feasible_size());
            }
        }

        if k1_no_empty <= 0 {
            return Self::cancel_signal(has_open_bids);
        }

        if k1_no_empty > MARKET_LIMIT as i32 {
            return MarketSignal::NoAction;
        }

        let orderbook = match relevant_books
            .iter()
            .find(|ob| ob.asset_id.as_str() == asset_id)
        {
            Some(ob) => ob,
            None => return MarketSignal::NoAction,
        };

        let best_ask_price = orderbook.best_feasible_price();
        if best_ask_price >= 1000 {
            return Self::cancel_signal(has_open_bids);
        }

        let auto_buy_at_ask_threshold = k1_no_empty - (sum_best_asks - best_ask_price as i32);

        let other_assets: Vec<String> = volumes
            .keys()
            .filter(|key| key.as_str() != asset_id)
            .cloned()
            .collect();

        if other_assets.is_empty() {
            return Self::cancel_signal(has_open_bids);
        }

        let bottleneck = other_assets
            .iter()
            .filter_map(|key| volumes.get(key).copied())
            .min()
            .unwrap_or(0);

        if bottleneck == 0 {
            return Self::cancel_signal(has_open_bids);
        }

        let auto_buy_volume = if k1_no_empty <= SMALL_MARKET_AMOUNT {
            min(
                bottleneck as i32,
                (MAX_SMALL_MARKET_VOLUME / k1_total) * 1000,
            )
        } else {
            min(bottleneck as i32, (MAX_VOLUME / k1_total) * 1000)
        };

        if auto_buy_volume <= 0 {
            return Self::cancel_signal(has_open_bids);
        }

        let alpha = if bottleneck > 3000 {
            ALPHA as i32
        } else {
            SECONDARY_ALPHA as i32
        };

        if auto_buy_at_ask_threshold - alpha < 1 || auto_buy_at_ask_threshold > 999 {
            return Self::cancel_signal(has_open_bids);
        }

        let market_best_bid = orderbook.best_bid_price() as i32;
        let beating_offset = if market_best_bid == bot_best_bid {
            0
        } else {
            10
        };

        let mut price = auto_buy_at_ask_threshold - alpha;
        if orderbook.tick_size == FRAC_CENTS {
            price = price
                .min(market_best_bid + beating_offset / 10)
                .min(999)
                .min(best_ask_price as i32 - 1);
            if price < 1 {
                return Self::cancel_signal(has_open_bids);
            }
        } else if orderbook.tick_size == FULL_CENTS {
            price = price
                .min(market_best_bid + beating_offset)
                .min(990)
                .min(best_ask_price as i32 - 10);
            price = (price / 10) * 10;
            if price < 10 {
                return Self::cancel_signal(has_open_bids);
            }
        } else {
            return Self::cancel_signal(has_open_bids);
        }

        let calc = MarketMakingCalculated {
            k_1_no_empty: k1_no_empty,
            bot_best_bid,
            market_best_bid,
            price_to_buy: price,
            size_to_buy: auto_buy_volume,
            market_name: slug.to_string(),
            asset_id: asset_id.to_string(),
            tick_size: orderbook.tick_size.clone(),
        };

        MarketSignal::Place(calc)
    }

    fn should_apply_lowest_volume(slug: &str) -> bool {
        let slug_lower = slug.to_ascii_lowercase();
        slug_lower.contains("fed-decision")
            || slug_lower.contains("largest-company")
            || slug_lower.contains("-temperature-")
            || slug_lower.contains("ballon-dor")
    }

    fn apply_lowest_volume(
        orderbooks: &[OrderBookStats],
        focus_asset: &str,
    ) -> Vec<OrderBookStats> {
        let mut adjusted = orderbooks.to_vec();
        if adjusted.len() <= 1 {
            return adjusted;
        }

        let lowest_ask_volume = adjusted
            .iter()
            .filter(|ob| ob.asset_id != focus_asset)
            .filter_map(|ob| {
                let price = ob.best_ask_price();
                let volume = ob.best_ask_volume();
                if volume != 0 && price != 1000 {
                    Some(volume)
                } else {
                    None
                }
            })
            .next();

        let lowest = match lowest_ask_volume {
            Some(v) => v,
            None => return adjusted,
        };

        for ob in &mut adjusted {
            if ob.asset_id == focus_asset {
                continue;
            }
            let price = ob.best_ask_price();
            if price == 1000 {
                continue;
            }
            let volume = ob.best_ask_volume();
            if volume == 0 {
                continue;
            }
            let new_volume = volume as i32 - lowest as i32;
            if new_volume <= 0 {
                ob.best_ask = None;
                if ob
                    .best_feasible_ask
                    .map(|(ask_price, _)| ask_price == price)
                    .unwrap_or(false)
                {
                    ob.best_feasible_ask = None;
                }
            } else {
                let updated = new_volume as u32;
                ob.best_ask = Some((price, updated));
                if let Some((ask_price, _)) = ob.best_feasible_ask {
                    if ask_price == price {
                        ob.best_feasible_ask = Some((ask_price, updated));
                    }
                }
            }
        }

        adjusted
    }

    fn cancel_signal(has_open_bids: bool) -> MarketSignal {
        if has_open_bids {
            MarketSignal::CancelBids
        } else {
            MarketSignal::NoAction
        }
    }

    fn cancel_bid_orders_with_snapshot(
        &self,
        ctx: Arc<StrategyContext>,
        asset_id: &str,
        open_bids: Vec<(u32, u32)>,
    ) {
        if open_bids.is_empty() {
            return;
        }

        let orders: Vec<(OrderSide, u32, u32)> = open_bids
            .into_iter()
            .map(|(price, size)| (OrderSide::Buy, price, size))
            .collect();

        if let Err(err) = StrategyClient::cancel_orders(ctx, asset_id, orders) {
            error!(
                "[{}] Failed to cancel open bids for {}: {}",
                self.name(),
                asset_id,
                err
            );
        }
    }

    fn process_place_signal(
        &self,
        ctx: Arc<StrategyContext>,
        calc: MarketMakingCalculated,
        open_bids: Vec<(u32, u32)>,
    ) {
        if calc.size_to_buy < MIN_AUTO_BUY_VOLUME {
            self.cancel_bid_orders_with_snapshot(ctx, &calc.asset_id, open_bids);
            return;
        }

        let elapsed = {
            let guard = self.last_order_time.lock().unwrap();
            SystemTime::now()
                .duration_since(*guard)
                .unwrap_or(Duration::ZERO)
                .as_millis()
        };

        if open_bids.is_empty() && elapsed > RATE_LIMIT {
            if let Err(err) = StrategyClient::place_limit_order(
                Arc::clone(&ctx),
                &calc.asset_id,
                OrderSide::Buy,
                calc.price_to_buy as u32,
                calc.size_to_buy as u32,
                &calc.tick_size,
                true,
            ) {
                error!(
                    "[{}] Failed to initiate neg-risk order for {} at {}x{}: {}",
                    self.name(),
                    calc.asset_id,
                    calc.price_to_buy,
                    calc.size_to_buy,
                    err
                );
                return;
            }

            if let Ok(mut guard) = self.last_order_time.lock() {
                *guard = SystemTime::now();
            }
            return;
        }

        let cancel_conditions = open_bids
            .iter()
            .any(|(price, _)| (*price as i32) > calc.price_to_buy)
            || open_bids
                .iter()
                .any(|(_, size)| (*size as i32) > calc.size_to_buy + 200_000);

        if cancel_conditions {
            self.cancel_bid_orders_with_snapshot(ctx, &calc.asset_id, open_bids);
        }
    }
}

impl Strategy for NegRiskNoMakerStrategy {
    fn name(&self) -> &'static str {
        "NegRiskNoMakerStrategy"
    }

    fn poly_handle_market_price_change(
        &self,
        ctx: Arc<StrategyContext>,
        _listener: Listener,
        payload: &PriceChange,
    ) {
        self.handle_price_change(ctx, payload);
    }
}

#[derive(Clone)]
struct OrderBookStats {
    asset_id: String,
    tick_size: String,
    best_bid: Option<(u32, u32)>,
    best_feasible_ask: Option<(u32, u32)>,
    best_ask: Option<(u32, u32)>,
}

impl OrderBookStats {
    fn from_orderbook(asset_id: &str, orderbook: &OrderBook) -> Self {
        Self {
            asset_id: asset_id.to_string(),
            tick_size: orderbook.get_tick_size().to_string(),
            best_bid: orderbook.best_bid(),
            best_feasible_ask: orderbook.best_feasible_ask(),
            best_ask: orderbook.best_ask(),
        }
    }

    fn best_feasible_price(&self) -> u32 {
        self.best_feasible_ask
            .map(|(price, _)| price)
            .unwrap_or(1000)
    }

    fn best_feasible_size(&self) -> u32 {
        self.best_feasible_ask.map(|(_, size)| size).unwrap_or(0)
    }

    fn best_bid_price(&self) -> u32 {
        self.best_bid.map(|(price, _)| price).unwrap_or(0)
    }

    fn best_ask_price(&self) -> u32 {
        self.best_ask.map(|(price, _)| price).unwrap_or(1000)
    }

    fn best_ask_volume(&self) -> u32 {
        self.best_ask.map(|(_, volume)| volume).unwrap_or(0)
    }
}

enum MarketSignal {
    Place(MarketMakingCalculated),
    CancelBids,
    NoAction,
}
