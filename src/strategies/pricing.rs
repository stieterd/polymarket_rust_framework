// // src/strategies/pricing.rs

// use anyhow::{anyhow, Result};
// use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
// use futures_util::stream::StreamExt;
// use log::{error, info, warn};
// use roots::{find_root_brent, SimpleConvergency};
// use rust_decimal::prelude::*;
// use rust_decimal::Decimal;
// use rust_decimal_macros::dec;
// use serde::Deserialize;
// use serde_json::Value;
// use statrs::distribution::{Continuous, ContinuousCDF, Normal};
// use std::collections::{HashMap, VecDeque};
// use std::sync::Arc;
// use tap::pipe::Pipe;
// use tokio::sync::Mutex;

// // --- GENERAL CONFIGURATION (from Python) ---
// const A: f64 = 9.0;
// const B: f64 = 0.35;
// const POLYMARKET_VWAP_LEVELS: usize = 5;
// const IV_SPREAD_STD_DEVS: Decimal = dec!(0.025);
// const GAMMA_PUNISHER: Decimal = dec!(0.00060);
// const QUARTIC_TAIL_PUNISHER: Decimal = dec!(50);
// const TOXIC_REGION_LOWER_BOUND: f64 = 0.45;
// const TOXIC_REGION_UPPER_BOUND: f64 = 0.55;
// const IV_CLIP_PERCENTAGE: f64 = 0.25; // Clip raw IV if it deviates by more than 25%

// // --- REALIZED VOL PROJECTOR CONFIG (FOR INITIALIZATION ONLY) ---
// const RV_WINDOW_MINUTES: i64 = 60;
// const KLINE_WINDOW_SIZE: usize = (RV_WINDOW_MINUTES * 60 + 1) as usize;
// const BINANCE_SPOT_KLINES_URL: &str = "https://api.binance.com/api/v3/klines";
// const BINANCE_SPOT_WEBSOCKET_1S_KLINE_URL: &str =
//     "wss://stream.binance.com:9443/ws/btcusdt@kline_1s";
// const BINANCE_SPOT_WEBSOCKET_TICKER_URL: &str =
//     "wss://stream.binance.com:9443/ws/btcusdt@bookTicker";
// const PROFILE_FILENAME: &str = "daily_half_hourly_variance_profiles_1m.json";

// // --- IV SMOOTHING CONFIG ---
// const MARKET_IV_WINDOW_SIZE: usize = 30;
// const SECONDS_IN_YEAR: Decimal = dec!(31_557_600); // 365.25 * 24 * 3600

// // --- PROJECT STRUCTURES ---
// #[derive(Debug, Clone)]
// pub struct MarketConfig {
//     pub name: String,
//     pub strike_price: f64,
//     pub end_time_et: String, // e.g., "2025-12-31T16:00:00"
// }

// #[derive(Debug, Default, Clone)]
// pub struct OrderBook {
//     pub bids: HashMap<String, String>, // Price -> Quantity
//     pub asks: HashMap<String, String>, // Price -> Quantity
// }

// impl OrderBook {
//     fn highest_bid(&self) -> Option<f64> {
//         self.bids
//             .keys()
//             .filter_map(|p| p.parse::<f64>().ok())
//             .fold(f64::NEG_INFINITY, f64::max)
//             .into()
//     }
//     fn lowest_ask(&self) -> Option<f64> {
//         self.asks
//             .keys()
//             .filter_map(|p| p.parse::<f64>().ok())
//             .fold(f64::INFINITY, f64::min)
//             .into()
//     }
// }

// // --- SHARED STATE & DATA STRUCTURES ---
// #[derive(Debug, Clone, Deserialize)]
// struct BinanceKlineStreamData {
//     #[serde(rename = "c")]
//     close: String,
//     #[serde(rename = "x")]
//     is_final: bool,
// }

// #[derive(Debug, Clone, Deserialize)]
// struct BinanceWebsocketKline {
//     #[serde(rename = "k")]
//     kline: BinanceKlineStreamData,
// }

// #[derive(Debug, Clone, Deserialize)]
// pub struct BinanceBookTicker {
//     #[serde(rename = "u")]
//     pub update_id: u64,
//     #[serde(rename = "s")]
//     pub symbol: String,
//     #[serde(rename = "b")]
//     pub bid_price: String,
//     #[serde(rename = "B")]
//     pub bid_qty: String,
//     #[serde(rename = "a")]
//     pub ask_price: String,
//     #[serde(rename = "A")]
//     pub ask_qty: String,
// }

// #[derive(Debug, Default)]
// struct SharedData {
//     polymarket_up_book: Option<OrderBook>,
//     binance_tob: Option<BinanceBookTicker>,
//     kline_closes: VecDeque<Decimal>,
//     is_kline_window_ready: bool,
//     system_status: String,
// }

// type SharedState = Arc<Mutex<SharedData>>;

// // --- VARIANCE PROFILE & RV PROJECTOR HELPERS ---
// type VarianceProfile = HashMap<String, HashMap<String, Decimal>>;

// fn load_variance_profile(filename: &str) -> Result<VarianceProfile> {
//     let data = std::fs::read_to_string(filename)?;
//     let json: Value = serde_json::from_str(&data)?;
//     let profiles_json = json["average_profiles"]
//         .as_object()
//         .ok_or_else(|| anyhow!("'average_profiles' key not found in profile json"))?;

//     let mut profiles: VarianceProfile = HashMap::new();
//     for (day, slots_json) in profiles_json {
//         let mut slots: HashMap<String, Decimal> = HashMap::new();
//         if let Some(slot_map) = slots_json.as_object() {
//             for (slot, val_str) in slot_map {
//                 if let Some(s) = val_str.as_str() {
//                     slots.insert(slot.clone(), Decimal::from_str(s)?);
//                 }
//             }
//         }
//         profiles.insert(day.clone(), slots);
//     }
//     info!("✅ [Data] Loaded variance profiles from '{}'", filename);
//     Ok(profiles)
// }

// fn calculate_expected_variance_in_window(
//     start_dt: DateTime<Utc>,
//     end_dt: DateTime<Utc>,
//     weekly_profiles: &VarianceProfile,
// ) -> Decimal {
//     if start_dt >= end_dt {
//         return dec!(0.0);
//     }
//     let mut total_variance = dec!(0.0);
//     let mut current_time = start_dt;
//     let day_names = [
//         "Monday",
//         "Tuesday",
//         "Wednesday",
//         "Thursday",
//         "Friday",
//         "Saturday",
//         "Sunday",
//     ];

//     while current_time < end_dt {
//         use chrono::Datelike;
//         let day_of_week = day_names[current_time.weekday().num_days_from_monday() as usize];
//         let day_profile = match weekly_profiles.get(day_of_week) {
//             Some(p) => p,
//             None => {
//                 current_time += Duration::minutes(30);
//                 continue;
//             }
//         };

//         use chrono::Timelike;
//         let slot_key = format!(
//             "{:.1}",
//             current_time.hour() as f64
//                 + if current_time.minute() >= 30 {
//                     0.5
//                 } else {
//                     0.0
//                 }
//         );
//         let slot_start_time = Utc
//             .with_ymd_and_hms(
//                 current_time.year(),
//                 current_time.month(),
//                 current_time.day(),
//                 current_time.hour(),
//                 if current_time.minute() >= 30 { 30 } else { 0 },
//                 0,
//             )
//             .unwrap();
//         let slot_end_time = slot_start_time + Duration::minutes(30);

//         if let Some(slot_variance) = day_profile.get(&slot_key) {
//             let overlap_start = start_dt.max(slot_start_time);
//             let overlap_end = end_dt.min(slot_end_time);
//             let overlap_seconds = (overlap_end - overlap_start).num_seconds();

//             if overlap_seconds > 0 {
//                 let overlap_fraction = Decimal::from(overlap_seconds) / dec!(1800.0);
//                 total_variance += *slot_variance * overlap_fraction;
//             }
//         }
//         current_time = slot_end_time;
//     }
//     total_variance
// }

// fn calculate_realized_variance(closes_deque: &VecDeque<Decimal>) -> Option<Decimal> {
//     if closes_deque.len() < 2 {
//         return None;
//     }
//     let closes: Vec<f64> = closes_deque.iter().filter_map(|d| d.to_f64()).collect();
//     if closes.len() != closes_deque.len() {
//         return None;
//     } // Ensure no conversion errors
//     let log_returns: Vec<f64> = closes.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
//     let unannualized_variance: f64 = log_returns.iter().map(|&r| r.powi(2)).sum();
//     Decimal::from_f64(unannualized_variance)
// }

// // --- IMPLIED VOLATILITY & BLACK-SCHOLES HELPERS ---
// fn get_itm_prob(s: f64, k: f64, t: f64, sigma_percent: f64) -> f64 {
//     let sigma = sigma_percent / 100.0;
//     if t <= 0.0 || sigma <= 0.0 {
//         return if s > k { 1.0 } else { 0.0 };
//     }
//     let d2 = ((s / k).ln() - (0.5 * sigma.powi(2)) * t) / (sigma * t.sqrt());
//     Normal::new(0.0, 1.0).unwrap().cdf(d2)
// }

// fn calculate_market_implied_iv(
//     spot_price: f64,
//     strike_price: f64,
//     t_expiry: f64,
//     market_prob: f64,
// ) -> Option<f64> {
//     if !(0.001..0.999).contains(&market_prob) || t_expiry <= 0.0 {
//         return None;
//     }
//     let mut solver = SimpleConvergency {
//         eps: 1e-6,
//         max_iter: 100,
//     };
//     let f = |s: f64| get_itm_prob(spot_price, strike_price, t_expiry, s * 100.0) - market_prob;
//     find_root_brent(1e-4, 10.0, &f, &mut solver).ok()
// }

// fn get_expiry_datetime(end_time_et: &str) -> Result<DateTime<Utc>> {
//     let naive_dt = NaiveDateTime::parse_from_str(end_time_et, "%Y-%m-%dT%H:%M:%S")?;
//     chrono_tz::US::Eastern
//         .from_local_datetime(&naive_dt)
//         .unwrap()
//         .with_timezone(&Utc)
//         .pipe(Ok)
// }

// // --- ASYNC DATA LISTENERS & BOOTSTRAP ---
// async fn bootstrap_historical_klines(shared_state: SharedState) -> Result<bool> {
//     info!(
//         "--- [Bootstrap] Fetching initial {}-minute kline data for RV...",
//         RV_WINDOW_MINUTES
//     );
//     let end_time = Utc::now();
//     let start_time = end_time - Duration::minutes(RV_WINDOW_MINUTES + 2);
//     let mut last_ts = start_time.timestamp_millis();
//     let client = reqwest::Client::new();
//     let mut all_klines: Vec<Vec<Value>> = vec![];

//     // Binance API allows fetching up to 1000 klines per request. Loop if needed.
//     for _ in 0..(((RV_WINDOW_MINUTES + 2) * 60) / 1000 + 1) {
//         let params = [
//             ("symbol", "BTCUSDT"),
//             ("interval", "1s"),
//             ("limit", "1000"),
//             ("startTime", &last_ts.to_string()),
//         ];
//         let resp = client
//             .get(BINANCE_SPOT_KLINES_URL)
//             .query(&params)
//             .send()
//             .await?
//             .json::<Vec<Vec<Value>>>()
//             .await?;
//         if resp.is_empty() {
//             break;
//         }
//         if let Some(last_kline) = resp.last() {
//             if let Some(ts) = last_kline[0].as_i64() {
//                 last_ts = ts + 1;
//             } else {
//                 break;
//             }
//         } else {
//             break;
//         }
//         all_klines.extend(resp);
//     }

//     // Sort and dedup klines
//     all_klines.sort_by_key(|k| k[0].as_i64().unwrap_or(0));
//     all_klines.dedup_by_key(|k| k[0].as_i64().unwrap_or(0));

//     if all_klines.len() < KLINE_WINDOW_SIZE {
//         error!(
//             "❌ FATAL: Bootstrap failed. Needed {} klines, got {}.",
//             KLINE_WINDOW_SIZE,
//             all_klines.len()
//         );
//         return Ok(false);
//     }

//     let recent_klines = all_klines.iter().rev().take(KLINE_WINDOW_SIZE).rev();
//     let mut data = shared_state.lock().await;
//     for kline in recent_klines {
//         if let Some(close_str) = kline[4].as_str() {
//             if let Ok(close_val) = Decimal::from_str(close_str) {
//                 data.kline_closes.push_back(close_val);
//             }
//         }
//     }

//     data.is_kline_window_ready = true;
//     info!(
//         "✅ [Bootstrap] Successfully filled RV data window with {} prices.",
//         data.kline_closes.len()
//     );
//     Ok(true)
// }

// async fn binance_1s_kline_listener(shared_state: SharedState) {
//     info!("--- [WebSocket] Connecting to Binance 1s kline stream for RV...");
//     loop {
//         match tokio_tungstenite::connect_async(BINANCE_SPOT_WEBSOCKET_1S_KLINE_URL).await {
//             Ok((ws_stream, _)) => {
//                 info!("✅ [Data] RV WebSocket connected. Listening for live 1s klines.");
//                 let mut ws_stream = ws_stream;
//                 while let Some(msg) = ws_stream.next().await {
//                     match msg {
//                         Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
//                             if let Ok(kline_data) =
//                                 serde_json::from_str::<BinanceWebsocketKline>(&text)
//                             {
//                                 if kline_data.kline.is_final {
//                                     if let Ok(close_val) =
//                                         Decimal::from_str(&kline_data.kline.close)
//                                     {
//                                         let mut data = shared_state.lock().await;
//                                         if data.kline_closes.len() >= KLINE_WINDOW_SIZE {
//                                             data.kline_closes.pop_front();
//                                         }
//                                         data.kline_closes.push_back(close_val);
//                                     }
//                                 }
//                             }
//                         }
//                         Err(e) => {
//                             error!("[RV WebSocket] Error receiving message: {}", e);
//                             break;
//                         }
//                         _ => {}
//                     }
//                 }
//             }
//             Err(e) => error!(
//                 "[RV WebSocket] Connection error: {}. Reconnecting in 5s...",
//                 e
//             ),
//         }
//         tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
//     }
// }

// async fn binance_spot_listener(shared_state: SharedState) {
//     loop {
//         match tokio_tungstenite::connect_async(BINANCE_SPOT_WEBSOCKET_TICKER_URL).await {
//             Ok((ws_stream, _)) => {
//                 info!("✅ [Data] Connected to Binance Spot Ticker WebSocket (BTC/USDT).");
//                 let mut ws_stream = ws_stream;
//                 while let Some(msg) = ws_stream.next().await {
//                     match msg {
//                         Ok(tokio_tungstenite::tungstenite::Message::Text(text)) => {
//                             if let Ok(ticker_data) =
//                                 serde_json::from_str::<BinanceBookTicker>(&text)
//                             {
//                                 shared_state.lock().await.binance_tob = Some(ticker_data);
//                             }
//                         }
//                         Err(e) => {
//                             error!("[Binance Ticker] WebSocket error: {}", e);
//                             break;
//                         }
//                         _ => {}
//                     }
//                 }
//             }
//             Err(e) => {
//                 error!("[Binance Ticker] Connection error: {}. Reconnecting...", e);
//                 shared_state.lock().await.binance_tob = None;
//             }
//         }
//         tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
//     }
// }

// async fn polymarket_listener(shared_state: SharedState) {
//     info!("--> Polymarket listener started (using placeholder data).");
//     loop {
//         let mut book = OrderBook::default();
//         book.bids.insert("0.5100".to_string(), "1000".to_string());
//         book.asks.insert("0.5150".to_string(), "1000".to_string());
//         shared_state.lock().await.polymarket_up_book = Some(book);
//         tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
//     }
// }

// // --- ONE-TIME IV INITIALIZATION ---
// async fn initialize_smoothed_iv(
//     shared_state: SharedState,
//     market_config: MarketConfig,
//     variance_profiles: Arc<VarianceProfile>,
// ) -> Result<VecDeque<f64>> {
//     info!("--- [IV Initializer] Calculating initial ATM IV to fill smoothing window...");
//     loop {
//         if shared_state.lock().await.is_kline_window_ready {
//             break;
//         }
//         info!("--- [IV Initializer] Waiting for historical kline data to be ready...");
//         tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
//     }

//     let mut initial_atm_iv: Option<f64> = None;
//     while initial_atm_iv.is_none() {
//         let result: Result<f64> = async {
//             let local_closes_copy = shared_state.lock().await.kline_closes.clone();
//             if local_closes_copy.len() < KLINE_WINDOW_SIZE {
//                 return Err(anyhow!("Not enough klines yet"));
//             }

//             let t_now = Utc::now();
//             let t_expiry = get_expiry_datetime(&market_config.end_time_et)?;
//             if t_now >= t_expiry {
//                 return Ok(0.0);
//             }

//             let t_start_rv = t_now - Duration::minutes(RV_WINDOW_MINUTES);
//             let var_realized = calculate_realized_variance(&local_closes_copy)
//                 .ok_or_else(|| anyhow!("Failed to calculate realized variance"))?;

//             let var_expected_rv_window =
//                 calculate_expected_variance_in_window(t_start_rv, t_now, &variance_profiles);
//             let activity_multiplier = if var_expected_rv_window > dec!(0) {
//                 var_realized / var_expected_rv_window
//             } else {
//                 dec!(1.0)
//             };
//             let var_expected_future =
//                 calculate_expected_variance_in_window(t_now, t_expiry, &variance_profiles);
//             let var_projected_future = var_expected_future * activity_multiplier;

//             let seconds_to_expiry = (t_expiry - t_now).num_seconds();
//             if seconds_to_expiry > 0 {
//                 let annualization_factor = SECONDS_IN_YEAR / Decimal::from(seconds_to_expiry);
//                 let annualized_variance = var_projected_future * annualization_factor;
//                 if annualized_variance > dec!(0) {
//                     return Ok(annualized_variance.sqrt().unwrap().to_f64().unwrap_or(0.0));
//                 }
//             }
//             Ok(0.0)
//         }
//         .await;

//         match result {
//             Ok(iv) => initial_atm_iv = Some(iv),
//             Err(e) => {
//                 warn!("Retrying initial IV calculation: {}", e);
//                 tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
//             }
//         }
//     }

//     let final_iv = initial_atm_iv.unwrap();
//     let mut window = VecDeque::with_capacity(MARKET_IV_WINDOW_SIZE);
//     for _ in 0..MARKET_IV_WINDOW_SIZE {
//         window.push_back(final_iv);
//     }
//     info!(
//         "✅ [IV Initializer] Smoothed IV window filled with initial ATM IV: {:.2}%",
//         final_iv * 100.0
//     );
//     Ok(window)
// }

// // --- PRICING & SPREAD LOGIC HELPERS ---
// fn predict_vwap_difference(log_imbalance: f64) -> Decimal {
//     Decimal::from_f64(A * (B * log_imbalance).tanh()).unwrap_or_default()
// }

// fn _calculate_vwap_for_volume(
//     levels: &[(Decimal, Decimal)],
//     target_volume: Decimal,
// ) -> Option<Decimal> {
//     let mut cumulative_value = dec!(0);
//     let mut cumulative_volume = dec!(0);
//     for (price, quantity) in levels {
//         let volume_to_take = (target_volume - cumulative_volume).min(*quantity);
//         cumulative_value += *price * volume_to_take;
//         cumulative_volume += volume_to_take;
//         if cumulative_volume >= target_volume {
//             break;
//         }
//     }
//     if cumulative_volume > dec!(0) {
//         Some(cumulative_value / cumulative_volume)
//     } else {
//         None
//     }
// }

// fn calculate_polymarket_book_vwap(book: &OrderBook, levels: usize) -> Option<f64> {
//     let mut bids: Vec<(Decimal, Decimal)> = book
//         .bids
//         .iter()
//         .filter_map(|(p, q)| Some((p.parse().ok()?, q.parse().ok()?)))
//         .collect();
//     let mut asks: Vec<(Decimal, Decimal)> = book
//         .asks
//         .iter()
//         .filter_map(|(p, q)| Some((p.parse().ok()?, q.parse().ok()?)))
//         .collect();
//     bids.sort_by(|a, b| b.0.cmp(&a.0));
//     asks.sort_by(|a, b| a.0.cmp(&b.0));

//     let top_bids: Vec<_> = bids.into_iter().take(levels).collect();
//     let top_asks: Vec<_> = asks.into_iter().take(levels).collect();
//     if top_bids.is_empty() || top_asks.is_empty() {
//         return None;
//     }

//     let bid_total_volume: Decimal = top_bids.iter().map(|(_, q)| *q).sum();
//     let ask_total_volume: Decimal = top_asks.iter().map(|(_, q)| *q).sum();

//     if bid_total_volume.is_zero() || ask_total_volume.is_zero() {
//         return None;
//     }
//     let matching_volume = bid_total_volume.min(ask_total_volume);

//     let (bid_vwap, ask_vwap) = (
//         _calculate_vwap_for_volume(&top_bids, matching_volume),
//         _calculate_vwap_for_volume(&top_asks, matching_volume),
//     );
//     if let (Some(b), Some(a)) = (bid_vwap, ask_vwap) {
//         return ((b + a) / dec!(2)).to_f64();
//     }
//     None
// }

// // --- MAIN ANALYSIS LOOP ---
// async fn analysis_loop(
//     shared_state: SharedState,
//     market_config: MarketConfig,
//     mut market_iv_window: VecDeque<f64>,
// ) {
//     info!("\n--- Smoothed IV Pricer with IV Clipping is LIVE ---");
//     let mut live_market_iv = 0.0;

//     loop {
//         tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
//         let result: Result<()> = async {
//             let (book, binance_tob) = {
//                 let data = shared_state.lock().await;
//                 (data.polymarket_up_book.clone(), data.binance_tob.clone())
//             };

//             let (book, binance_tob) = match (book, binance_tob) {
//                 (Some(b), Some(tob)) => (b, tob),
//                 _ => {
//                     shared_state.lock().await.system_status =
//                         "Waiting for data feeds (Binance/Polymarket)...".to_string();
//                     return Ok(());
//                 }
//             };

//             let midpoint = (binance_tob.bid_price.parse::<Decimal>().unwrap_or_default()
//                 + binance_tob.ask_price.parse::<Decimal>().unwrap_or_default())
//                 / dec!(2);
//             let log_imbalance = (binance_tob.bid_qty.parse::<f64>().unwrap_or(1.0)
//                 / binance_tob.ask_qty.parse::<f64>().unwrap_or(1.0))
//             .ln();
//             let binance_fair_value = midpoint + predict_vwap_difference(log_imbalance);

//             let expiry_utc = get_expiry_datetime(&market_config.end_time_et)?;
//             let t_expiry =
//                 (expiry_utc - Utc::now()).num_seconds() as f64 / SECONDS_IN_YEAR.to_f64().unwrap();
//             if t_expiry <= 0.0 {
//                 shared_state.lock().await.system_status = "Market has expired.".to_string();
//                 return Ok(());
//             }

//             let (s, k, t) = (
//                 binance_fair_value.to_f64().unwrap(),
//                 market_config.strike_price,
//                 t_expiry,
//             );
//             let market_prob = calculate_polymarket_book_vwap(&book, POLYMARKET_VWAP_LEVELS)
//                 .ok_or(anyhow!("Could not calculate PM VWAP"))?;
//             let in_toxic_region =
//                 (TOXIC_REGION_LOWER_BOUND..TOXIC_REGION_UPPER_BOUND).contains(&market_prob);

//             // --- MARKET IV SMOOTHING & CLIPPING LOGIC ---
//             let current_smoothed_iv =
//                 market_iv_window.iter().sum::<f64>() / market_iv_window.len() as f64;

//             if !in_toxic_region {
//                 if let Some(live_market_iv_val) = calculate_market_implied_iv(s, k, t, market_prob)
//                 {
//                     live_market_iv = live_market_iv_val;

//                     // Clipping logic
//                     let iv_upper_bound = current_smoothed_iv * (1.0 + IV_CLIP_PERCENTAGE);
//                     let iv_lower_bound = current_smoothed_iv * (1.0 - IV_CLIP_PERCENTAGE);
//                     let clipped_iv = live_market_iv_val.clamp(iv_lower_bound, iv_upper_bound);

//                     if market_iv_window.len() >= MARKET_IV_WINDOW_SIZE {
//                         market_iv_window.pop_front();
//                     }
//                     market_iv_window.push_back(clipped_iv);
//                 }
//             }

//             let smoothed_market_iv =
//                 market_iv_window.iter().sum::<f64>() / market_iv_window.len() as f64;
//             let effective_iv = smoothed_market_iv;

//             // --- STATUS UPDATE ---
//             {
//                 let mut data = shared_state.lock().await;
//                 if in_toxic_region {
//                     data.system_status = "TOXIC REGION: Mkt IV Frozen.".to_string();
//                 } else {
//                     data.system_status = "✅ Active".to_string();
//                 }
//             }
//             let status = shared_state.lock().await.system_status.clone();

//             // --- QUOTING LOGIC & DISPLAY ---
//             print!("\x1B[2J\x1B[1;1H"); // Clear screen
//             println!(
//                 "Market: {} | Strike: ${:.2} | Status: {}",
//                 market_config.name, market_config.strike_price, status
//             );
//             println!("{:-<85}", "");
//             println!(
//                 "{:<30} | {:>15} | {:>15} | {:>15}",
//                 "Pricing Model", "Bid Price", "Ask Price", "Spread (cents)"
//             );
//             println!("{:-<85}", "");

//             let (model_bid_price, model_ask_price, model_total_spread) = if effective_iv > 0.0 {
//                 let spot_for_gamma = if in_toxic_region {
//                     let target_prob = if s > k {
//                         TOXIC_REGION_UPPER_BOUND
//                     } else {
//                         TOXIC_REGION_LOWER_BOUND
//                     };
//                     let normal_dist = Normal::new(0.0, 1.0).unwrap();
//                     let d2_target = normal_dist.inverse_cdf(target_prob);
//                     k * (d2_target * effective_iv * t.sqrt() + 0.5 * effective_iv.powi(2) * t).exp()
//                 } else {
//                     s
//                 };

//                 let gamma_std = if t > 1e-9 {
//                     let d1_g = ((spot_for_gamma / k).ln() + (0.5 * effective_iv.powi(2)) * t)
//                         / (effective_iv * t.sqrt());
//                     (GAMMA_PUNISHER
//                         * Decimal::from_f64(Normal::new(0.0, 1.0).unwrap().pdf(d1_g)).unwrap())
//                         / Decimal::from_f64(t).unwrap().sqrt().unwrap()
//                 } else {
//                     dec!(0)
//                 };

//                 let total_stdevs = IV_SPREAD_STD_DEVS + gamma_std;
//                 let (sqrt_t, sig_dec) = (
//                     Decimal::from_f64(t).unwrap().sqrt().unwrap(),
//                     Decimal::from_f64(effective_iv).unwrap(),
//                 );
//                 let mult_total = (total_stdevs * sig_dec * sqrt_t).to_f64().unwrap().exp();
//                 let (lb, ub) = (s / mult_total, s * mult_total);

//                 let (raw_bid, raw_ask) = (
//                     get_itm_prob(lb, k, t, effective_iv * 100.0),
//                     get_itm_prob(ub, k, t, effective_iv * 100.0),
//                 );
//                 let mid = Decimal::from_f64((raw_bid + raw_ask) / 2.0).unwrap();
//                 let tail_mult = dec!(1.0) + QUARTIC_TAIL_PUNISHER * (mid - dec!(0.5)).powi(4);
//                 let punished_spread = Decimal::from_f64(raw_ask - raw_bid).unwrap() * tail_mult;

//                 let bid_p = (mid - punished_spread / dec!(2))
//                     .max(dec!(0))
//                     .to_f64()
//                     .unwrap();
//                 let ask_p = (mid + punished_spread / dec!(2))
//                     .min(dec!(1))
//                     .to_f64()
//                     .unwrap();
//                 (bid_p, ask_p, (ask_p - bid_p) * 100.0)
//             } else {
//                 (0.0, 0.0, 0.0)
//             };

//             println!(
//                 "{:<30} | {:>15.4} | {:>15.4} | {:>15.2}",
//                 "Smoothed IV Model (Punished)",
//                 model_bid_price,
//                 model_ask_price,
//                 model_total_spread
//             );
//             println!(
//                 "{:<30} | {:>15.4} | {:>15.4} | {:>15.2}",
//                 "Polymarket Best Bid/Ask",
//                 book.highest_bid().unwrap_or(0.0),
//                 book.lowest_ask().unwrap_or(1.0),
//                 (book.lowest_ask().unwrap_or(1.0) - book.highest_bid().unwrap_or(0.0)) * 100.0
//             );
//             println!("{:-<85}", "");

//             println!("Inputs & Intermediate Values:");
//             println!("  - Binance Fair Value      : ${:.2}", binance_fair_value);
//             println!(
//                 "  - PM Implied IV (Smooth)  : {:.2}% (Live Raw: {:.2}%) {}",
//                 smoothed_market_iv * 100.0,
//                 live_market_iv * 100.0,
//                 if in_toxic_region { "(FROZEN)" } else { "" }
//             );
//             println!("  - Effective IV for Quoting: {:.2}%", effective_iv * 100.0);
//             println!("{:-<85}", "");
//             let model_fair_prob = if effective_iv > 0.0 {
//                 get_itm_prob(s, k, t, effective_iv * 100.0)
//             } else {
//                 0.5
//             };
//             println!(
//                 "  - Model Fair Prob: {:.4} -> PM VWAP Prob: {:.4}",
//                 model_fair_prob, market_prob
//             );

//             Ok(())
//         }
//         .await;
//         if let Err(e) = result {
//             error!("An error occurred in the main analysis loop: {}", e);
//         }
//     }
// }

// // --- MAIN ENTRY POINT ---
// pub async fn pricing_main() {
//     env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

//     let market_config = MarketConfig {
//         name: "BTC > $70,000".to_string(),
//         strike_price: 70000.0,
//         end_time_et: "2025-10-31T16:00:00".to_string(),
//     };

//     let variance_profiles =
//         Arc::new(load_variance_profile(PROFILE_FILENAME).expect("Failed to load variance profile"));

//     let shared_state = Arc::new(Mutex::new(SharedData {
//         kline_closes: VecDeque::with_capacity(KLINE_WINDOW_SIZE),
//         ..Default::default()
//     }));

//     if !bootstrap_historical_klines(shared_state.clone())
//         .await
//         .unwrap_or(false)
//     {
//         error!("Could not start analyzer due to RV bootstrap failure.");
//         return;
//     }

//     let market_iv_window = match initialize_smoothed_iv(
//         shared_state.clone(),
//         market_config.clone(),
//         variance_profiles.clone(),
//     )
//     .await
//     {
//         Ok(window) => window,
//         Err(e) => {
//             error!("Failed to initialize smoothed IV window: {}", e);
//             return;
//         }
//     };

//     info!("Spawning all background tasks...");
//     let handles = vec![
//         tokio::spawn(binance_spot_listener(shared_state.clone())),
//         tokio::spawn(binance_1s_kline_listener(shared_state.clone())),
//         tokio::spawn(polymarket_listener(shared_state.clone())),
//         tokio::spawn(analysis_loop(
//             shared_state.clone(),
//             market_config.clone(),
//             market_iv_window,
//         )),
//     ];

//     futures_util::future::join_all(handles).await;
// }




// src/strategies/pricing.rs

// --- IMPORTS ---
// Standard library imports
use std::collections::{HashMap, VecDeque};
use std::str;
use std::sync::{Arc, RwLock};

// Crate-level imports from the project structure
use crate::exchange_listeners::poly_models::{
    AggOrderbook, Listener, PolymarketMessageWrapperOld, PriceChange,
};
use crate::exchange_listeners::{
    event_processor::{SocketEvent, SocketEventSender},
    orderbooks::poly_orderbook::OrderBook as CoreOrderBook,
    AppState, PolyMarketState,
};
use crate::strategies::{Strategy, StrategyContext};
use crate::exchange_listeners::poly_models::TickSizeChangePayload;


// External crate imports
use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, NaiveDateTime, TimeZone, Timelike, Utc};
use chrono_tz::US::Eastern;
use futures_util::{stream::StreamExt, SinkExt};
use log::{error, info, warn};
use roots::{find_root_brent, SimpleConvergency};
use rust_decimal::prelude::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use serde_json::Value;
use statrs::distribution::{Continuous, ContinuousCDF, Normal};
use tap::pipe::Pipe;
use tokio::{
    net::TcpStream,
    sync::mpsc::{self},
    time::{self, Duration},
};
use tokio_rustls::{
    rustls::{ClientConfig, OwnedTrustAnchor, RootCertStore, ServerName},
    TlsConnector,
};
use tokio_tungstenite::{
    client_async,
    tungstenite::{client::IntoClientRequest, protocol::Message},
    WebSocketStream,
};

// --- GENERAL CONFIGURATION (from Python) ---
const A: f64 = 9.0;
const B: f64 = 0.35;
const POLYMARKET_VWAP_LEVELS: usize = 5;
const IV_SPREAD_STD_DEVS: Decimal = dec!(0.025);
const GAMMA_PUNISHER: Decimal = dec!(0.00060);
const QUARTIC_TAIL_PUNISHER: Decimal = dec!(50);
const TOXIC_REGION_LOWER_BOUND: f64 = 0.44;
const TOXIC_REGION_UPPER_BOUND: f64 = 0.56;
const IV_CLIP_PERCENTAGE: f64 = 0.10; // Clip raw IV if it deviates by more than 25% from smoothed IV

// --- REALIZED VOL PROJECTOR CONFIG (FOR INITIALIZATION ONLY) ---
const RV_WINDOW_MINUTES: i64 = 15;
const KLINE_WINDOW_SIZE: usize = (RV_WINDOW_MINUTES * 60 + 1) as usize;
const BINANCE_SPOT_KLINES_URL: &str = "https://api.binance.com/api/v3/klines";
const BINANCE_SPOT_WEBSOCKET_1S_KLINE_URL: &str =
    "wss://stream.binance.com:9443/ws/btcusdt@kline_1s";
const BINANCE_SPOT_WEBSOCKET_TICKER_URL: &str =
    "wss://stream.binance.com:9443/ws/btcusdt@bookTicker";
const PROFILE_FILENAME: &str = "daily_half_hourly_variance_profiles_1m.json";

// --- IV SMOOTHING CONFIG ---
const MARKET_IV_WINDOW_SIZE: usize = 50;
const SECONDS_IN_YEAR: Decimal = dec!(31_557_600); // 365.25 * 24 * 3600

// --- PROJECT STRUCTURES (LOCAL TO PRICING LOGIC) ---
// Note: The local MarketConfig is now removed as we use DiscoveredMarketConfig directly.

/// A simplified OrderBook struct for use within the pricing model's VWAP calculations.
#[derive(Debug, Default, Clone)]
pub struct PricingOrderBook {
    pub bids: HashMap<String, String>, // Price -> Quantity
    pub asks: HashMap<String, String>, // Price -> Quantity
}

impl PricingOrderBook {
    fn highest_bid(&self) -> Option<f64> {
        self.bids
            .keys()
            .filter_map(|p| p.parse::<f64>().ok())
            .fold(f64::NEG_INFINITY, f64::max)
            .pipe(|v| if v.is_infinite() { None } else { Some(v) })
    }
    fn lowest_ask(&self) -> Option<f64> {
        self.asks
            .keys()
            .filter_map(|p| p.parse::<f64>().ok())
            .fold(f64::INFINITY, f64::min)
            .pipe(|v| if v.is_infinite() { None } else { Some(v) })
    }
}

// --- SHARED STATE & DATA STRUCTURES ---
#[derive(Debug, Clone, Deserialize)]
struct BinanceKlineStreamData {
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "x")]
    is_final: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct BinanceWebsocketKline {
    #[serde(rename = "k")]
    kline: BinanceKlineStreamData,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceBookTicker {
    #[serde(rename = "u")]
    pub update_id: u64,
    #[serde(rename = "s")]
    pub symbol: String,
    #[serde(rename = "b")]
    pub bid_price: String,
    #[serde(rename = "B")]
    pub bid_qty: String,
    #[serde(rename = "a")]
    pub ask_price: String,
    #[serde(rename = "A")]
    pub ask_qty: String,
}

#[derive(Debug, Default)]
struct SharedData {
    binance_tob: Option<BinanceBookTicker>,
    kline_closes: VecDeque<Decimal>,
    is_kline_window_ready: bool,
    system_status: String,
}

type SharedState = Arc<tokio::sync::Mutex<SharedData>>;

// --- VARIANCE PROFILE & RV PROJECTOR HELPERS ---
type VarianceProfile = HashMap<String, HashMap<String, Decimal>>;

// In src/strategies/pricing.rs

fn load_variance_profile(filename: &str) -> Result<VarianceProfile> {
    let data = std::fs::read_to_string(filename)?;
    let json: Value = serde_json::from_str(&data)?;
    let profiles_json = json["average_profiles"]
        .as_object()
        .ok_or_else(|| anyhow!("'average_profiles' key not found in profile json"))?;

    let mut profiles: VarianceProfile = HashMap::new();
    for (day, slots_json) in profiles_json {
        let mut slots: HashMap<String, Decimal> = HashMap::new();
        if let Some(slot_map) = slots_json.as_object() {
            for (slot, val_json) in slot_map {
                // --- FIX IS HERE ---
                // The JSON value is a Number, not a String.
                // We convert the Value to a string representation first, then parse.
                let val_as_string = val_json.to_string();
                if let Ok(dec_val) = Decimal::from_str(&val_as_string) {
                    slots.insert(slot.clone(), dec_val);
                }
                // --- END FIX ---
            }
        }
        profiles.insert(day.clone(), slots);
    }
    info!("✅ [Data] Loaded variance profiles from '{}'", filename);
    Ok(profiles)
}

fn calculate_expected_variance_in_window(
    start_dt: DateTime<Utc>,
    end_dt: DateTime<Utc>,
    weekly_profiles: &VarianceProfile,
) -> Decimal {
    if start_dt >= end_dt {
        return dec!(0.0);
    }
    let mut total_variance = dec!(0.0);
    let mut current_time = start_dt;
    let day_names = [
        "Monday",
        "Tuesday",
        "Wednesday",
        "Thursday",
        "Friday",
        "Saturday",
        "Sunday",
    ];

    while current_time < end_dt {
        let day_of_week = day_names[current_time.weekday().num_days_from_monday() as usize];
        let slot_key = format!(
            "{:.1}",
            current_time.hour() as f64
                + if current_time.minute() >= 30 {
                    0.5
                } else {
                    0.0
                }
        );

        // --- FIX IS HERE ---
        // This logic now exactly mirrors the Python version's robust `get` with a default value.
        // It correctly handles cases where a day or a specific time slot might be missing from the profile.
        let slot_variance = weekly_profiles
            .get(day_of_week) // Get Option<&HashMap> for the day
            .and_then(|day_profile| day_profile.get(&slot_key)) // Chain to get Option<&&Decimal> for the slot
            .copied() // Dereference to get Option<Decimal>
            .unwrap_or(dec!(0)); // Default to Decimal(0) if day or slot key is missing
        // --- END FIX ---

        let slot_start_time = Utc
            .with_ymd_and_hms(
                current_time.year(),
                current_time.month(),
                current_time.day(),
                current_time.hour(),
                if current_time.minute() >= 30 { 30 } else { 0 },
                0,
            )
            .unwrap();
        let slot_end_time = slot_start_time + chrono::Duration::minutes(30);
        
        let overlap_start = start_dt.max(slot_start_time);
        let overlap_end = end_dt.min(slot_end_time);
        let overlap_seconds = (overlap_end - overlap_start).num_seconds();

        if overlap_seconds > 0 {
            let overlap_fraction = Decimal::from(overlap_seconds) / dec!(1800.0);
            total_variance += slot_variance * overlap_fraction;
        }

        current_time = slot_end_time;
    }
    total_variance
}

fn calculate_realized_variance(closes_deque: &VecDeque<Decimal>) -> Option<Decimal> {
    if closes_deque.len() < 2 {
        return None;
    }
    let closes: Vec<f64> = closes_deque.iter().filter_map(|d| d.to_f64()).collect();
    if closes.len() != closes_deque.len() {
        return None;
    }
    
    // --- FIX IS HERE ---
    // Make the calculation robust against floating point errors (NaN/Infinity)
    let log_returns: Vec<f64> = closes.windows(2)
        .map(|w| (w[1] / w[0]).ln())
        .collect();

    // Check for non-finite numbers which can poison the calculation
    if log_returns.iter().any(|&r| !r.is_finite()) {
        warn!("Non-finite log return detected during realized variance calculation.");
        return None;
    }
    
    let unannualized_variance: f64 = log_returns.iter().map(|&r| r.powi(2)).sum();

    if !unannualized_variance.is_finite() {
        warn!("Non-finite unannualized variance detected.");
        return None;
    }
    
    Decimal::from_f64(unannualized_variance)
    // --- END FIX ---
}

// --- IMPLIED VOLATILITY HELPER ---
fn calculate_market_implied_iv(
    spot_price: f64,
    strike_price: f64,
    t_expiry: f64,
    market_prob: f64,
) -> Option<f64> {
    if !(0.001..0.999).contains(&market_prob) || t_expiry <= 0.0 {
        return None;
    }
    let f = |s: f64| get_itm_prob(spot_price, strike_price, t_expiry, s * 100.0) - market_prob;
    let mut solver = SimpleConvergency { eps: 1e-6, max_iter: 100 };
    find_root_brent(1e-4, 10.0, &f, &mut solver).ok()
}

fn get_itm_prob(s: f64, k: f64, t: f64, sigma_percent: f64) -> f64 {
    let sigma = sigma_percent / 100.0;
    if t <= 0.0 || sigma <= 0.0 {
        return if s > k { 1.0 } else { 0.0 };
    }
    let d2 = ((s / k).ln() - (0.5 * sigma.powi(2)) * t) / (sigma * t.sqrt());
    Normal::new(0.0, 1.0).unwrap().cdf(d2)
}

fn get_expiry_datetime(end_time_et: &str) -> Result<DateTime<Utc>> {
    let naive_dt = NaiveDateTime::parse_from_str(end_time_et, "%Y-%m-%dT%H:%M:%S")?;
    chrono_tz::US::Eastern
        .from_local_datetime(&naive_dt)
        .single()
        .ok_or_else(|| anyhow!("Failed to convert local datetime to unique timezone datetime"))?
        .with_timezone(&Utc)
        .pipe(Ok)
}


// --- BOOTSTRAPPING & ASYNC DATA LISTENERS ---
async fn bootstrap_historical_klines(shared_state: SharedState) -> Result<bool> {
    info!(
        "--- [Bootstrap] Fetching initial {}-minute kline data for RV...",
        RV_WINDOW_MINUTES
    );
    let start_time_ms =
        (Utc::now() - chrono::Duration::minutes(RV_WINDOW_MINUTES + 2)).timestamp_millis();
    let client = reqwest::Client::new();
    let mut all_klines: Vec<Vec<Value>> = vec![];
    let mut last_ts = start_time_ms;

    for _ in 0..(((RV_WINDOW_MINUTES + 2) * 60) / 1000 + 1) {
        let params = [
            ("symbol", "BTCUSDT"),
            ("interval", "1s"),
            ("startTime", &last_ts.to_string()),
            ("limit", "1000"),
        ];
        let klines_batch: Vec<Vec<Value>> = client.get(BINANCE_SPOT_KLINES_URL).query(&params).send().await?.json().await?;
        if klines_batch.is_empty() { break; }
        last_ts = klines_batch.last().unwrap()[0].as_i64().unwrap() + 1;
        all_klines.extend(klines_batch);
    }
    
    let unique_klines: HashMap<i64, &Vec<Value>> = all_klines.iter().map(|k| (k[0].as_i64().unwrap(), k)).collect();
    let mut sorted_klines: Vec<&Vec<Value>> = unique_klines.values().copied().collect();
    sorted_klines.sort_by_key(|k| k[0].as_i64().unwrap());

    if sorted_klines.len() < KLINE_WINDOW_SIZE {
        error!("❌ FATAL: Bootstrap failed. Needed {} klines, got {}.", KLINE_WINDOW_SIZE, sorted_klines.len());
        return Ok(false);
    }

    let recent_klines = sorted_klines.iter().rev().take(KLINE_WINDOW_SIZE).rev();
    let mut data = shared_state.lock().await;
    for kline in recent_klines {
        if let Some(close_str) = kline[4].as_str() {
            if let Ok(close_val) = Decimal::from_str(close_str) {
                data.kline_closes.push_back(close_val);
            }
        }
    }
    info!("✅ [Bootstrap] Successfully filled RV data window with {} prices.", data.kline_closes.len());
    data.is_kline_window_ready = true;
    Ok(true)
}

async fn binance_1s_kline_listener(shared_state: SharedState) {
    info!("--- [WebSocket] Connecting to Binance 1s kline stream for RV...");
    loop {
        match tokio_tungstenite::connect_async(BINANCE_SPOT_WEBSOCKET_1S_KLINE_URL).await {
            Ok((ws_stream, _)) => {
                info!("✅ [Data] RV WebSocket connected. Listening for live 1s klines.");
                let mut ws_stream = ws_stream;
                while let Some(msg) = ws_stream.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        if let Ok(kline_data) = serde_json::from_str::<BinanceWebsocketKline>(&text) {
                            if kline_data.kline.is_final {
                                if let Ok(close_val) = Decimal::from_str(&kline_data.kline.close) {
                                    let mut data = shared_state.lock().await;
                                    data.kline_closes.push_back(close_val);
                                    // --- FIX IS HERE ---
                                    // This is more robust than a single `if`, ensuring the deque
                                    // never exceeds its intended maximum size.
                                    while data.kline_closes.len() > KLINE_WINDOW_SIZE {
                                        data.kline_closes.pop_front();
                                    }
                                    // --- END FIX ---
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => error!("[RV WebSocket] Connection error: {}. Reconnecting in 5s...", e),
        }
        time::sleep(Duration::from_secs(5)).await;
    }
}

async fn binance_spot_listener(shared_state: SharedState) {
    loop {
        match tokio_tungstenite::connect_async(BINANCE_SPOT_WEBSOCKET_TICKER_URL).await {
            Ok((ws_stream, _)) => {
                info!("✅ [Data] Connected to Binance Spot Ticker WebSocket (BTC/USDT).");
                let mut ws_stream = ws_stream;
                while let Some(msg) = ws_stream.next().await {
                    if let Ok(Message::Text(text)) = msg {
                        if let Ok(ticker_data) = serde_json::from_str::<BinanceBookTicker>(&text) {
                            shared_state.lock().await.binance_tob = Some(ticker_data);
                        }
                    }
                }
            }
            Err(e) => {
                error!("[Binance Ticker] Connection error: {}. Reconnecting...", e);
                shared_state.lock().await.binance_tob = None;
            }
        }
        time::sleep(Duration::from_secs(5)).await;
    }
}

// --- ONE-TIME IV INITIALIZATION ---
async fn initialize_smoothed_iv(
    shared_state: SharedState,
    market_config: DiscoveredMarketConfig,
    variance_profiles: Arc<VarianceProfile>,
) -> Result<VecDeque<f64>> {
    info!("--- [IV Initializer] Calculating initial ATM IV to fill smoothing window...");
    while !shared_state.lock().await.is_kline_window_ready {
        info!("--- [IV Initializer] Waiting for historical kline data to be ready...");
        time::sleep(Duration::from_secs(2)).await;
    }

    let mut initial_atm_iv = None;
    while initial_atm_iv.is_none() {
        let result: Result<f64> = async {
            let local_closes_copy = shared_state.lock().await.kline_closes.clone();
            if local_closes_copy.len() < KLINE_WINDOW_SIZE {
                return Err(anyhow!("Not enough klines yet"));
            }

            let t_now = Utc::now();
            let t_expiry = get_expiry_datetime(&market_config.end_time_et)?;
            if t_now >= t_expiry { return Ok(0.0); }

            let t_start_rv = t_now - chrono::Duration::minutes(RV_WINDOW_MINUTES);
            
            // --- START: DIAGNOSTIC LOGGING ---
            let var_realized = calculate_realized_variance(&local_closes_copy)
                .ok_or_else(|| anyhow!("Failed to calculate realized variance"))?;
            info!("[IV DEBUG] var_realized: {}", var_realized);

            let var_expected_rv_window = calculate_expected_variance_in_window(t_start_rv, t_now, &variance_profiles);
            info!("[IV DEBUG] var_expected_rv_window (for past {} min): {}", RV_WINDOW_MINUTES, var_expected_rv_window);

            let activity_multiplier = if var_expected_rv_window > dec!(0) { 
                var_realized / var_expected_rv_window 
            } else { 
                dec!(1.0) 
            };
            info!("[IV DEBUG] activity_multiplier: {}", activity_multiplier);

            let var_expected_future = calculate_expected_variance_in_window(t_now, t_expiry, &variance_profiles);
            info!("[IV DEBUG] var_expected_future (until expiry): {}", var_expected_future);

            let var_projected_future = var_expected_future * activity_multiplier;
            info!("[IV DEBUG] var_projected_future: {}", var_projected_future);

            let seconds_to_expiry = (t_expiry - t_now).num_seconds();
            if seconds_to_expiry <= 0 { return Ok(0.0); }
            info!("[IV DEBUG] seconds_to_expiry: {}", seconds_to_expiry);

            let annualization_factor = SECONDS_IN_YEAR / Decimal::from(seconds_to_expiry);
            info!("[IV DEBUG] annualization_factor: {}", annualization_factor);

            let annualized_variance = var_projected_future * annualization_factor;
            info!("[IV DEBUG] annualized_variance: {}", annualized_variance);

            let final_iv = if annualized_variance > dec!(0) {
                annualized_variance.sqrt().and_then(|v| v.to_f64()).unwrap_or(0.0)
            } else {
                0.0
            };
            info!("[IV DEBUG] final_iv (pre-return): {}", final_iv);
            // --- END: DIAGNOSTIC LOGGING ---

            Ok(final_iv)
        }.await;

        match result {
            Ok(iv) => initial_atm_iv = Some(iv),
            Err(e) => {
                warn!("Retrying initial IV calculation. Error: {}", e);
                time::sleep(Duration::from_secs(2)).await;
            }
        }
    }

    let final_iv = initial_atm_iv.unwrap();
    let mut window = VecDeque::with_capacity(MARKET_IV_WINDOW_SIZE);
    for _ in 0..MARKET_IV_WINDOW_SIZE {
        window.push_back(final_iv);
    }
    
    info!("✅ [IV Initializer] Smoothed IV window filled with initial ATM IV: {:.2}%", final_iv * 100.0);
    Ok(window)
}

// --- PRICING & SPREAD LOGIC HELPERS ---
fn predict_vwap_difference(log_imbalance: f64) -> Decimal {
    Decimal::from_f64(A * f64::tanh(B * log_imbalance)).unwrap_or_default()
}

fn _calculate_vwap_for_volume(
    levels: &[(Decimal, Decimal)],
    target_volume: Decimal,
) -> Option<Decimal> {
    let mut cumulative_value = dec!(0);
    let mut cumulative_volume = dec!(0);
    for (price, quantity) in levels {
        let volume_to_take = (target_volume - cumulative_volume).min(*quantity);
        cumulative_value += *price * volume_to_take;
        cumulative_volume += volume_to_take;
        if cumulative_volume >= target_volume { break; }
    }
    if cumulative_volume > dec!(0) { Some(cumulative_value / cumulative_volume) } else { None }
}

fn calculate_polymarket_book_vwap(book: &PricingOrderBook, levels: usize) -> Option<f64> {
    let mut bids: Vec<(Decimal, Decimal)> = book.bids.iter().filter_map(|(p, q)| Some((p.parse().ok()?, q.parse().ok()?))).collect();
    let mut asks: Vec<(Decimal, Decimal)> = book.asks.iter().filter_map(|(p, q)| Some((p.parse().ok()?, q.parse().ok()?))).collect();
    bids.sort_by(|a, b| b.0.cmp(&a.0));
    asks.sort_by(|a, b| a.0.cmp(&b.0));

    let top_bids: Vec<_> = bids.into_iter().take(levels).collect();
    let top_asks: Vec<_> = asks.into_iter().take(levels).collect();
    if top_bids.is_empty() || top_asks.is_empty() { return None; }

    let bid_total_volume: Decimal = top_bids.iter().map(|(_, q)| *q).sum();
    let ask_total_volume: Decimal = top_asks.iter().map(|(_, q)| *q).sum();
    if bid_total_volume.is_zero() || ask_total_volume.is_zero() { return None; }
    let matching_volume = bid_total_volume.min(ask_total_volume);

    let (bid_vwap, ask_vwap) = (
        _calculate_vwap_for_volume(&top_bids, matching_volume),
        _calculate_vwap_for_volume(&top_asks, matching_volume),
    );
    if let (Some(b), Some(a)) = (bid_vwap, ask_vwap) {
        return ((b + a) / dec!(2)).to_f64();
    }
    None
}

// --- MAIN ANALYSIS LOOP ---
async fn analysis_loop(
    shared_state: SharedState,
    poly_state: Arc<PolyMarketState>,
    market_config: DiscoveredMarketConfig,
    mut market_iv_window: VecDeque<f64>,
) {
    info!("\n--- Smoothed IV Pricer with IV Clipping is LIVE ---");
    let mut live_market_iv = 0.0;
    let up_token_id = market_config.yes_token_id.clone();

    loop {
        time::sleep(Duration::from_millis(100)).await;
        let result: Result<()> = async {
            let (book, binance_tob) = {
                let binance_tob = shared_state.lock().await.binance_tob.clone();
                let book = if let Some(orderbook_entry) = poly_state.orderbooks.get(&up_token_id) {
                    if let Ok(orderbook_lock) = orderbook_entry.read() {
                        let mut pricing_book = PricingOrderBook::default();
                        for bid in orderbook_lock.get_bid_map().iter() {
                            pricing_book.bids.insert((f64::from(*bid.key()) / 1000.0).to_string(), (f64::from(*bid.value()) / 1000.0).to_string());
                        }
                        for ask in orderbook_lock.get_ask_map().iter() {
                            pricing_book.asks.insert((f64::from(*ask.key()) / 1000.0).to_string(), (f64::from(*ask.value()) / 1000.0).to_string());
                        }
                        Some(pricing_book)
                    } else { None }
                } else { None };
                (book, binance_tob)
            };

            let (book, binance_tob) = match (book, binance_tob) {
                (Some(b), Some(tob)) => (b, tob),
                _ => {
                    shared_state.lock().await.system_status = "Waiting for data feeds (Binance/Polymarket)...".to_string();
                    return Ok(());
                }
            };

            let midpoint = (binance_tob.bid_price.parse::<Decimal>()? + binance_tob.ask_price.parse::<Decimal>()?) / dec!(2);
            let log_imbalance = (binance_tob.bid_qty.parse::<f64>()? / binance_tob.ask_qty.parse::<f64>()?).ln();
            let binance_fair_value = midpoint + predict_vwap_difference(log_imbalance);
            
            let expiry_utc = get_expiry_datetime(&market_config.end_time_et)?;
            let t_expiry = (expiry_utc - Utc::now()).num_seconds() as f64 / SECONDS_IN_YEAR.to_f64().unwrap();
            if t_expiry <= 0.0 {
                shared_state.lock().await.system_status = "Market has expired.".to_string();
                return Ok(());
            }

            let (s, k, t) = (binance_fair_value.to_f64().unwrap(), market_config.strike_price, t_expiry);
            let market_prob = calculate_polymarket_book_vwap(&book, POLYMARKET_VWAP_LEVELS).ok_or_else(|| anyhow!("Could not calculate PM VWAP"))?;
            let in_toxic_region = (TOXIC_REGION_LOWER_BOUND..TOXIC_REGION_UPPER_BOUND).contains(&market_prob);

            let current_smoothed_iv = market_iv_window.iter().sum::<f64>() / market_iv_window.len() as f64;

            if !in_toxic_region {
                if let Some(live_market_iv_val) = calculate_market_implied_iv(s, k, t, market_prob) {
                    live_market_iv = live_market_iv_val;
                    let iv_upper_bound = current_smoothed_iv * (1.0 + IV_CLIP_PERCENTAGE);
                    let iv_lower_bound = current_smoothed_iv * (1.0 - IV_CLIP_PERCENTAGE);
                    let clipped_iv = live_market_iv_val.clamp(iv_lower_bound, iv_upper_bound);
                    market_iv_window.pop_front();
                    market_iv_window.push_back(clipped_iv);
                }
            }

            let smoothed_market_iv = market_iv_window.iter().sum::<f64>() / market_iv_window.len() as f64;
            let effective_iv = smoothed_market_iv;

            // --- STATUS UPDATE AND DISPLAY ---
            let status = if in_toxic_region { "TOXIC REGION: Mkt IV Frozen.".to_string() } else { "✅ Active".to_string() };
            shared_state.lock().await.system_status = status.clone();
            
            print!("\x1B[2J\x1B[1;1H"); // Clear screen
            println!("Market: {} | Strike: ${:.2} | Status: {}", market_config.name, market_config.strike_price, status);
            println!("{:-<85}", "");
            println!("{:<30} | {:>15} | {:>15} | {:>15}", "Pricing Model", "Bid Price", "Ask Price", "Spread (cents)");
            println!("{:-<85}", "");

            let (model_bid_price, model_ask_price, model_total_spread) = if effective_iv > 0.0 {
                let spot_for_gamma = if in_toxic_region {
                    let target_prob = if s > k { TOXIC_REGION_UPPER_BOUND } else { TOXIC_REGION_LOWER_BOUND };
                    let normal_dist = Normal::new(0.0, 1.0).unwrap();
                    let d2_target = normal_dist.inverse_cdf(target_prob);
                    k * (d2_target * effective_iv * t.sqrt() + 0.5 * effective_iv.powi(2) * t).exp()
                } else { s };

                let gamma_std = if t > 1e-9 {
                    let d1_g = ((spot_for_gamma / k).ln() + (0.5 * effective_iv.powi(2)) * t) / (effective_iv * t.sqrt());
                    (GAMMA_PUNISHER * Decimal::from_f64(Normal::new(0.0, 1.0).unwrap().pdf(d1_g)).unwrap()) / Decimal::from_f64(t).unwrap().sqrt().unwrap()
                } else { dec!(0) };

                let total_stdevs = IV_SPREAD_STD_DEVS + gamma_std;
                let (sqrt_t, sig_dec) = (Decimal::from_f64(t).unwrap().sqrt().unwrap(), Decimal::from_f64(effective_iv).unwrap());
                let mult_total = (total_stdevs * sig_dec * sqrt_t).to_f64().unwrap().exp();
                let (lb, ub) = (s / mult_total, s * mult_total);

                let (raw_bid, raw_ask) = (get_itm_prob(lb, k, t, effective_iv * 100.0), get_itm_prob(ub, k, t, effective_iv * 100.0));
                let mid = Decimal::from_f64((raw_bid + raw_ask) / 2.0).unwrap();
                let tail_mult = dec!(1.0) + QUARTIC_TAIL_PUNISHER * (mid - dec!(0.5)).powi(4);
                let punished_spread = Decimal::from_f64(raw_ask - raw_bid).unwrap() * tail_mult;
                
                let bid_p = (mid - punished_spread / dec!(2)).max(dec!(0)).to_f64().unwrap();
                let ask_p = (mid + punished_spread / dec!(2)).min(dec!(1)).to_f64().unwrap();
                (bid_p, ask_p, (ask_p - bid_p) * 100.0)
            } else { (0.0, 0.0, 0.0) };
            
            println!("{:<30} | {:>15.4} | {:>15.4} | {:>15.2}", "Smoothed IV Model (Punished)", model_bid_price, model_ask_price, model_total_spread);
            println!("{:<30} | {:>15.4} | {:>15.4} | {:>15.2}", "Polymarket Best Bid/Ask", book.highest_bid().unwrap_or(0.0), book.lowest_ask().unwrap_or(1.0), (book.lowest_ask().unwrap_or(1.0) - book.highest_bid().unwrap_or(0.0)) * 100.0);
            println!("{:-<85}", "");

            println!("Inputs & Intermediate Values:");
            println!("  - Binance Fair Value      : ${:.2}", binance_fair_value);
            println!("  - PM Implied IV (Smooth)  : {:.2}% (Live Raw: {:.2}%) {}", smoothed_market_iv * 100.0, live_market_iv * 100.0, if in_toxic_region { "(FROZEN)" } else { "" });
            println!("  - Effective IV for Quoting: {:.2}%", effective_iv * 100.0);
            println!("{:-<85}", "");
            let model_fair_prob = if effective_iv > 0.0 { get_itm_prob(s, k, t, effective_iv * 100.0) } else { 0.5 };
            println!("  - Model Fair Prob: {:.4} -> PM VWAP Prob: {:.4}", model_fair_prob, market_prob);

            Ok(())
        }.await;

        if let Err(e) = result {
            error!("An error occurred in the main analysis loop: {}", e);
        }
    }
}


// --- MAIN ENTRY POINT ---
pub async fn pricing_main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let market_config = autodiscover_market_config("bitcoin", "btc").await.expect("Market discovery failed").expect("Failed to find active market");

    let app_state = Arc::new(AppState::default());
    let polymarket_state = Arc::new(PolyMarketState::default());
    let strategies: Vec<Arc<dyn Strategy>> = vec![Arc::new(UpdateOrderbookStrategy::new())];
    let event_sender = spawn_event_processor(app_state.clone(), polymarket_state.clone(), strategies);
    info!("--- Event Processor thread has been started ---");

    let variance_profiles = Arc::new(load_variance_profile(PROFILE_FILENAME).expect("Failed to load variance profile"));
    let shared_state = Arc::new(tokio::sync::Mutex::new(SharedData { kline_closes: VecDeque::with_capacity(KLINE_WINDOW_SIZE), ..Default::default() }));

    if !bootstrap_historical_klines(shared_state.clone()).await.unwrap_or(false) {
        error!("Could not start analyzer due to RV bootstrap failure.");
        return;
    }

    let market_iv_window = initialize_smoothed_iv(shared_state.clone(), market_config.clone(), variance_profiles.clone()).await.expect("Failed to initialize smoothed IV window");
    
    info!("Spawning all background tasks...");
    
    let yes_token_static: &'static str = Box::leak(market_config.yes_token_id.clone().into_boxed_str());
    let market_asset_ids = vec![yes_token_static];
    let market_event_sender = event_sender.clone();
    tokio::spawn(async move {
        polymarket_market_listener_legacy(&market_asset_ids, market_event_sender).await;
    });
    info!("--- Polymarket Exchange Listener has been started ---");

    let handles = vec![
        tokio::spawn(binance_spot_listener(shared_state.clone())),
        tokio::spawn(binance_1s_kline_listener(shared_state.clone())),
        tokio::spawn(analysis_loop(shared_state.clone(), polymarket_state.clone(), market_config.clone(), market_iv_window)),
    ];
    futures_util::future::join_all(handles).await;
}


// ==========================================================================================
// --- START: LOGIC COPIED FROM OTHER FILES TO MAKE THIS SELF-CONTAINED ---
// ==========================================================================================

// --- FROM: autodiscover_markets.rs ---
#[derive(Debug, Clone)]
pub struct DiscoveredMarketConfig {
    pub name: String,
    pub yes_token_id: String,
    pub no_token_id: String,
    pub end_time_et: String,
    pub binance_symbol: String,
    pub strike_price: f64,
}

pub async fn autodiscover_market_config(auto_market: &str, crypto: &str) -> Result<Option<DiscoveredMarketConfig>> {
    info!("--- Autodiscovering current crypto market ---");
    let client = reqwest::Client::new();
    let now_et = Utc::now().with_timezone(&Eastern);
    let start_hour_et = now_et.with_minute(0).and_then(|dt| dt.with_second(0)).and_then(|dt| dt.with_nanosecond(0)).ok_or_else(|| anyhow!("failed to normalize time"))?;
    let end_hour_et = start_hour_et + chrono::Duration::hours(1);
    let hour_str = start_hour_et.format("%I%p").to_string().trim_start_matches('0').to_lowercase();
    let day_str = start_hour_et.format("%e").to_string().trim().to_string();
    let market_slug = format!("{}-up-or-down-{}-{}-{}-et", auto_market, start_hour_et.format("%B").to_string().to_lowercase(), day_str, hour_str);
    info!("--> Target market slug: {}", market_slug);

    let mut offset = 0usize;
    let mut target_event: Option<Value> = None;
    loop {
        let url = format!("https://gamma-api.polymarket.com/events?limit=100&active=true&closed=false&offset={}", offset);
        let events: Value = client.get(&url).send().await?.json().await?;
        let events_arr = events.as_array().ok_or_else(|| anyhow!("events not an array"))?;
        if events_arr.is_empty() { break; }
        if let Some(event) = events_arr.iter().find(|e| e.get("slug").and_then(Value::as_str).map_or(false, |s| s.eq_ignore_ascii_case(&market_slug))) {
            target_event = Some(event.clone());
            break;
        }
        offset += events_arr.len();
    }

    let target_event = target_event.ok_or_else(|| anyhow!("Market slug '{}' not found", market_slug))?;
    let markets = target_event.get("markets").and_then(Value::as_array).ok_or_else(|| anyhow!("event has no markets"))?;
    let (mut yes_token_id, mut no_token_id) = (None, None);

    for market in markets {
        if let (Some(Value::String(s_tokens)), Some(Value::String(s_outcomes))) = (market.get("clobTokenIds"), market.get("outcomes")) {
            let token_ids: Vec<String> = serde_json::from_str(s_tokens)?;
            let outcomes: Vec<String> = serde_json::from_str(s_outcomes)?;
            for (token_id, outcome) in token_ids.into_iter().zip(outcomes.into_iter()) {
                if outcome.eq_ignore_ascii_case("Up") { yes_token_id = Some(token_id); }
                else if outcome.eq_ignore_ascii_case("Down") { no_token_id = Some(token_id); }
            }
        }
        if yes_token_id.is_some() && no_token_id.is_some() { break; }
    }

    let yes_token_id = yes_token_id.ok_or_else(|| anyhow!("Could not find 'Up' token"))?;
    info!("--> Found YES Token ID: {}", yes_token_id);
    let no_token_id = no_token_id.ok_or_else(|| anyhow!("Could not find 'Down' token"))?;

    let start_timestamp_ms = start_hour_et.timestamp_millis();
    let binance_symbol = format!("{}USDT", crypto.to_uppercase());
    let binance_url = format!("https://api.binance.com/api/v3/klines?symbol={}&interval=1h&startTime={}&limit=1", binance_symbol, start_timestamp_ms);
    let kline_data: Value = client.get(&binance_url).send().await?.json().await?;
    let strike_price = kline_data.as_array().and_then(|a| a.first()).and_then(|e| e.get(1)).and_then(Value::as_str).and_then(|p| p.parse().ok()).ok_or_else(|| anyhow!("could not parse strike"))?;
    info!("--> Found Strike Price: {}", strike_price);

    let config = DiscoveredMarketConfig {
        name: market_slug,
        yes_token_id,
        no_token_id,
        end_time_et: end_hour_et.format("%Y-%m-%dT%H:%M:%S").to_string(),
        binance_symbol,
        strike_price,
    };
    Ok(Some(config))
}

// --- FROM: poly_listeners.rs ---
async fn connect_with_tls12(url: &str) -> Result<WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>> {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| OwnedTrustAnchor::from_subject_spki_name_constraints(ta.subject, ta.spki, ta.name_constraints)));
    let config = ClientConfig::builder().with_safe_defaults().with_root_certificates(root_cert_store).with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let request = url.into_client_request()?;
    let host = request.uri().host().ok_or_else(|| anyhow!("URL has no host"))?;
    let port = request.uri().port_u16().unwrap_or(443);
    let server_name = ServerName::try_from(host)?;
    let tcp_stream = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let tls_stream = connector.connect(server_name, tcp_stream).await?;
    let (ws_stream, _) = client_async(request, tls_stream).await?;
    Ok(ws_stream)
}

async fn polymarket_websocket_handler_with_message(listener: Listener, url: &str, subscription_message: String, event_tx: SocketEventSender) {
    loop {
        match connect_with_tls12(url).await {
            Ok(ws_stream) => {
                info!("[{}] TLS 1.2 connection established.", listener);
                let (mut write, mut read) = ws_stream.split();
                if let Err(e) = write.send(Message::Text(subscription_message.clone())).await {
                    error!("[{}] Failed to subscribe: {}. Retrying...", listener, e);
                    time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                info!("[{}] Subscription message sent.", listener);
                while let Some(msg_result) = read.next().await {
                    match msg_result {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = event_tx.send(SocketEvent::Market { listener, payload: text.into_bytes() }) {
                                error!("[{}] Failed to forward market event: {}", listener, e);
                            }
                        }
                        Ok(Message::Close(_)) => { warn!("[{}] Connection closed by server.", listener); break; },
                        Err(e) => { error!("[{}] WebSocket stream error: {}.", listener, e); break; },
                        _ => {}
                    }
                }
            }
            Err(e) => error!("[{}] Connection failed: {}", listener, e),
        }
        warn!("[{}] Listener DOWN. Reconnecting in 5s...", listener);
        time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn polymarket_market_listener_legacy(asset_ids: &[&'static str], event_tx: SocketEventSender) {
    let subscription_msg = serde_json::json!({ "assets_ids": asset_ids, "type": "market" }).to_string();
    polymarket_websocket_handler_with_message(
        Listener::PolyMarketLegacy,
        "wss://ws-subscriptions-clob.polymarket.com/ws/market",
        subscription_msg,
        event_tx,
    ).await;
}

// --- FROM: event_processor.rs and strategies/poly_state_updates/update_orderbooks.rs ---
pub struct UpdateOrderbookStrategy;
impl UpdateOrderbookStrategy { pub fn new() -> Self { Self } }
impl Strategy for UpdateOrderbookStrategy {
    fn name(&self) -> &'static str { "UpdateOrderbooks" }

    fn poly_handle_market_agg_orderbook(&self, ctx: &StrategyContext, _listener: Listener, snapshot: &AggOrderbook) {
        let orderbook = CoreOrderBook::new(snapshot);
        ctx.poly_state.orderbooks.insert(snapshot.asset_id.clone(), Arc::new(RwLock::new(orderbook)));
    }

    fn poly_handle_market_price_change(&self, ctx: &StrategyContext, _listener: Listener, payload: &PriceChange) {
        if let Some(orderbook_entry) = ctx.poly_state.orderbooks.get(&payload.asset_id) {
            if let Ok(book) = orderbook_entry.write() {
                let now_epoch = Utc::now().timestamp().to_string();
                book.apply_price_change(payload, &now_epoch);
            }
        }
    }
}

struct EventProcessor {
    poly_state: Arc<PolyMarketState>,
    strategies: Vec<Arc<dyn Strategy>>,
}

impl EventProcessor {
    fn handle_event(&self, event: SocketEvent) {
        if let SocketEvent::Market { listener, mut payload } = event {
            self.handle_market_event(listener, &mut payload);
        }
    }
    
    fn strategy_context(&self) -> StrategyContext {
        StrategyContext::new(Arc::new(AppState::default()), Arc::clone(&self.poly_state))
    }
    
    fn handle_market_event(&self, listener: Listener, payload: &mut [u8]) {
        if payload.is_empty() { return; }
        
        let dispatch = |wrapper: PolymarketMessageWrapperOld| {
            let ctx = self.strategy_context();
            match wrapper.event_type.as_str() {
                "book" => {
                    let snapshot = AggOrderbook {
                        asset_id: wrapper.asset_id.unwrap_or_default(),
                        bids: wrapper.bids,
                        asks: wrapper.asks,
                        timestamp: wrapper.timestamp.unwrap_or_default(),
                        hash: wrapper.hash.unwrap_or_default(),
                    };
                    if !snapshot.asset_id.is_empty() {
                        for strategy in &self.strategies { strategy.poly_handle_market_agg_orderbook(&ctx, listener, &snapshot); }
                    }
                }
                "price_change" => {
                    for change in &wrapper.price_changes {
                        let pc = PriceChange { asset_id: change.asset_id.clone(), price: change.price.clone(), size: change.size.clone(), side: change.side.clone() };
                        for strategy in &self.strategies { strategy.poly_handle_market_price_change(&ctx, listener, &pc); }
                    }
                }
                _ => {}
            }
        };

        if let Ok(wrappers) = simd_json::from_slice::<Vec<PolymarketMessageWrapperOld>>(payload) {
            for w in wrappers { dispatch(w); }
        } else if let Ok(w) = simd_json::from_slice::<PolymarketMessageWrapperOld>(payload) {
            dispatch(w);
        } else {
            error!("[{}] Failed to parse legacy market message. Raw: {}", listener, String::from_utf8_lossy(payload));
        }
    }
}

pub fn spawn_event_processor(app_state: Arc<AppState>, poly_state: Arc<PolyMarketState>, strategies: Vec<Arc<dyn Strategy>>) -> SocketEventSender {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let processor = EventProcessor { poly_state, strategies };
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            processor.handle_event(event);
        }
    });
    tx
}