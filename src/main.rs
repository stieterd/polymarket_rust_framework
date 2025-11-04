//main.rs
pub mod clob_client;
pub mod config;
pub mod credentials;
pub mod marketmaking;
pub mod poly_orderbooks;
pub mod popular_tokens;
pub mod strategies;

use itertools::Itertools;
use log::info;
use strategies::{Strategy, UpdateOrderStrategy, UpdateOrderbookStrategy, UpdatePositionStrategy};

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
    marketmaking::poly_market_struct::events_json_to_events_with_market_map,
    strategies::{
        app_state_updates::update_crypto_orderbooks::UpdateCryptoOrderbookStrategy,
        custom::{koen::koen_strategy::KoenStrategy, peter::peter_strategy::PeterStrategy},
        logging::{
            bbo_logging::BBOLoggingStrategy, crypto_logging::CryptoLoggingStrategy,
            main_logging::MainLoggingStrategy, order_logging::OrderLoggingStrategy,
            position_logging::PositionLoggingStrategy,
        },
    },
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
    info!("Fetching neg risk markets");
    let events = fetch_neg_risk_markets().await.unwrap();
    let (events, market_map) = events_json_to_events_with_market_map(events);

    let app_state = Arc::new(AppState::default()); // Financial instruments
    let market_map = Arc::new(market_map); // put into Arc for sharing
    let market_asset_ids: Vec<String> = market_map.keys().cloned().collect();
    let market_asset_ids = Arc::new(market_asset_ids);
    let polymarket_state = Arc::new(PolyMarketState {
        markets: Arc::clone(&market_map),
        ..Default::default()
    }); // Orderbooks
    info!("Starting strategies");
    let strategies: Vec<Arc<dyn Strategy>> = vec![
        Arc::new(UpdateOrderbookStrategy::new()),
        Arc::new(UpdateOrderStrategy::new()),
        Arc::new(UpdatePositionStrategy::new()),
        Arc::new(PeterStrategy::new()),
        // Arc::new(OrderLoggingStrategy::new()),
        Arc::new(PositionLoggingStrategy::new()),

        // Arc::new(UpdateCryptoOrderbookStrategy::new()),
        // Arc::new(BBOLoggingStrategy::new()),
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

    let market_counting_sender = counting_sender.clone();
    const MARKET_LISTENER_BATCH_SIZE: usize = 500;
    let total_asset_ids = market_asset_ids.len();

    if total_asset_ids == 0 {
        tokio::spawn(async move {
            let asset_refs: Vec<&str> = Vec::new();
            exchange_listeners::poly_listeners::polymarket_market_listener_legacy(
                &asset_refs,
                market_counting_sender,
            )
            .await;
        });
    } else {
        for batch_start in (0..total_asset_ids).step_by(MARKET_LISTENER_BATCH_SIZE) {
            let batch_end = (batch_start + MARKET_LISTENER_BATCH_SIZE).min(total_asset_ids);
            let market_asset_ids = Arc::clone(&market_asset_ids);
            let market_counting_sender = counting_sender.clone();

            tokio::spawn(async move {
                let asset_refs: Vec<&str> = market_asset_ids[batch_start..batch_end]
                    .iter()
                    .map(String::as_str)
                    .collect();
                exchange_listeners::poly_listeners::polymarket_market_listener_legacy(
                    &asset_refs,
                    market_counting_sender,
                )
                .await;
            });
        }
    }

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
            // println!("Pending events: {} - {}", counting_sender.pending(), now);
        }
    }
}
