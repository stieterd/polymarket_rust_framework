//main.rs
pub mod clob_client;
pub mod credentials;
pub mod popular_tokens;
pub mod marketmaking;
pub mod poly_orderbooks;
pub mod strategies;
use itertools::Itertools;
use strategies::{
    Strategy, UpdateOrderStrategy, UpdateOrderbookStrategy,
    UpdatePositionStrategy,
};

use clob_client::{
    client::ClobClient,
    prebuilt_order::{build_prebuilt_order, PrebuiltOrder},
};

use ethers::abi::Hash;

use std::{
    alloc::System,
    cmp::{min, Ordering},
    collections::{HashMap, HashSet, VecDeque},
    ops::Deref,
    process,
    sync::{atomic, Arc},
    thread,
    time::{Instant, SystemTime},
};

use tokio::{
    sync::{Mutex, RwLock},
    time::{sleep, Duration},
};

use clob_client::constants::{FRAC_CENTS, FULL_CENTS};

use dashmap::DashMap;
use marketmaking::{
    maker_taker_config::REFRESH_TIME,
    marketmakingclient::CLIENT,
    poly_get_markets::fetch_neg_risk_markets,
    poly_market_struct::{
        build_asset_id_to_event_map, build_asset_id_to_market_map, Event, EventJson, Market,
    },
    utils::MarketMakingCalculated,
};

mod exchange_listeners;
use exchange_listeners::{event_processor, AppState, PolyMarketState};
use tokio::runtime;

use crate::{
    exchange_listeners::autodiscover_markets::autodiscover_market_config,
    strategies::{app_state_updates::update_crypto_orderbooks::UpdateCryptoOrderbookStrategy, logging::{bbo_logging::BBOLoggingStrategy, crypto_logging::CryptoLoggingStrategy, main_logging::MainLoggingStrategy}},
};

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(4)
        .max_blocking_threads(8)
        .enable_all()
        .build()
        .expect("Failed to build custom Tokio runtime");

    //runtime.block_on(debug_main());
    // runtime.block_on(strategies::pricing::pricing_main());
    runtime.block_on(debug_main());
}

async fn debug_main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let app_state = Arc::new(AppState::default()); // Financial instruments
    let polymarket_state = Arc::new(PolyMarketState::default()); // Orderbooks
    let strategies: Vec<Arc<dyn Strategy>> = vec![
        Arc::new(UpdateOrderbookStrategy::new()),
        Arc::new(UpdateOrderStrategy::new()),
        Arc::new(UpdatePositionStrategy::new()),
        Arc::new(UpdateCryptoOrderbookStrategy::new()),
        Arc::new(BBOLoggingStrategy::new()),
        // Arc::new(MainLoggingStrategy::new()),
        // Arc::new(CryptoLoggingStrategy),

    ];
    let counting_sender = event_processor::spawn_event_processor(
        Arc::clone(&app_state),
        Arc::clone(&polymarket_state),
        strategies,
    );

    let market_config = autodiscover_market_config("bitcoin", "btc")
        .await
        .unwrap()
        .unwrap();

    log::info!("--- Exchange Listener Thread has been started ---");

    let yes_token_static: &'static str =
        Box::leak(market_config.yes_token_id.clone().into_boxed_str());
    let market_asset_ids = vec![yes_token_static];
    let market_asset_ids = popular_tokens::ASSET_TOKENS;
    let market_counting_sender = counting_sender.clone();

    let market_asset_ids = market_asset_ids.to_vec(); // Ensure market_asset_ids is moved into the async block

    tokio::spawn(async move {
        exchange_listeners::poly_listeners::polymarket_market_listener_legacy(
            &market_asset_ids,
            market_counting_sender,
        )
        .await;
    });

    let _exchange_listener_handles =
        exchange_listeners::spawn_exchange_price_listeners(counting_sender.clone());

    let user_counting_sender = counting_sender.clone();
    tokio::spawn(async move {
        exchange_listeners::poly_listeners::polymarket_user_listener_legacy(user_counting_sender)
            .await;
    });

    loop {
        tokio::time::sleep(Duration::from_millis(1)).await;
        let pending = counting_sender.pending();
        if pending > 0 {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            println!("Pending events: {} - {}", counting_sender.pending(), now);

        }
    }
}
