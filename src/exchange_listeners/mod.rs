use std::sync::Arc;
use tokio::task::JoinHandle;

// Keep existing mods
pub mod crypto_listeners;
pub mod crypto_models;

// Add new mods for Polymarket
pub mod autodiscover_markets;
pub mod event_processor;
pub mod orderbooks;
pub mod poly_client;
pub mod poly_listeners;
pub mod poly_models;
pub mod states;

pub use crypto_models::{Crypto, Exchange, Instrument};
// Use the new Polymarket state
pub use states::{AppState, PolyMarketState};

pub fn spawn_exchange_price_listeners(
    event_tx: Arc<event_processor::CountingSender>,
) -> Vec<JoinHandle<()>> {
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();

    // tasks.push(tokio::spawn(
    //     crypto_exchange_listeners::binance_usdc_usdt_listener(Arc::clone(&rates), event_tx.clone()),
    // ));
    // tasks.push(tokio::spawn(listeners::coinbase_usdt_usd_listener(
    //     Arc::clone(&rates),
    //     event_tx.clone(),
    // )));

    for &crypto in &[Crypto::BTC] {
        //, Crypto::ETH, Crypto::XRP, Crypto::SOL
        tasks.push(tokio::spawn(crypto_listeners::binance_listener(
            crypto,
            false,
            event_tx.clone(),
        )));

        // --- Bybit Spot & Perp ---
        // tasks.push(tokio::spawn(crypto_listeners::bybit_listener(
        //     crypto,
        //     false, // is_perp = false
        //     event_tx.clone(),
        // )));
        tasks.push(tokio::spawn(crypto_listeners::bybit_listener(
            crypto,
            true, // is_perp = true
            event_tx.clone(),
        )));

        // --- OKX Spot & Perp ---
        // tasks.push(tokio::spawn(crypto_listeners::okx_listener(
        //     crypto,
        //     false, // is_perp = false
        //     event_tx.clone(),
        // )));
        tasks.push(tokio::spawn(crypto_listeners::okx_listener(
            crypto,
            true, // is_perp = true
            event_tx.clone(),
        )));

        // --- Deribit Perp (L2 Book) ---
        tasks.push(tokio::spawn(crypto_listeners::deribit_listener(
            crypto,
            true, // is_perp = true
            event_tx.clone(),
        )));

        // Kraken Perp
        // tasks.push(tokio::spawn(crypto_listeners::kraken_listener(
        //     crypto,
        //     true, // is_perp = true
        //     event_tx.clone(),
        // )));
    }

    // tasks.push(tokio::spawn(listeners::coinbase_legacy_listener(
    //     crypto,
    //     Arc::clone(&rates),
    //     event_tx.clone(),
    // )));
    // tasks.push(tokio::spawn(listeners::coinbase_advanced_listener(
    //     crypto,
    //     Arc::clone(&rates),
    //     event_tx.clone(),
    // )));
    // tasks.push(tokio::spawn(listeners::bitstamp_listener(
    //     crypto,
    //     Arc::clone(&rates),
    //     event_tx.clone(),
    // )));

    // tasks.push(tokio::spawn(listeners::deribit_listener(
    //     crypto,
    //     true,
    //     Arc::clone(&rates),
    //     event_tx.clone(),
    // )));

    // tasks.push(tokio::spawn(listeners::bitmex_listener(
    //     crypto,
    //     Arc::clone(&rates),
    //     event_tx.clone(),
    // )));

    //     tasks.push(tokio::spawn(crypto_listeners::bybit_listener(
    //         crypto,
    //         false,
    //         event_tx.clone(),
    //     )));

    //     tasks.push(tokio::spawn(crypto_listeners::okx_listener(
    //         crypto,
    //         false,
    //         event_tx.clone(),
    //     )));
    // }

    tasks
}

// #[derive(Debug, Clone)]
// pub struct ListenerConfig {
//     pub cryptos: BTreeSet<Crypto>,
//     pub exchanges: BTreeMap<Exchange, (bool, bool)>,
//     pub binance_usdc_usdt: bool,
//     pub coinbase_usdt_usd: bool,
//     pub polymarket: bool,
// }

// impl ListenerConfig {
//     pub fn all() -> Self {
//         let mut cryptos = BTreeSet::new();
//         cryptos.insert(Crypto::BTC);
//         cryptos.insert(Crypto::ETH);
//         cryptos.insert(Crypto::XRP);
//         cryptos.insert(Crypto::SOL);

//         let mut exchanges = BTreeMap::new();
//         exchanges.insert(Exchange::Binance, (true, true));
//         exchanges.insert(Exchange::CoinbaseLegacy, (true, false));
//         exchanges.insert(Exchange::CoinbaseAdvanced, (true, false));
//         exchanges.insert(Exchange::Bitstamp, (true, false));
//         exchanges.insert(Exchange::Deribit, (true, true));
//         exchanges.insert(Exchange::Bitmex, (false, true));
//         exchanges.insert(Exchange::Bybit, (true, true));
//         exchanges.insert(Exchange::Okx, (true, true));

//         Self {
//             cryptos,
//             exchanges,
//             binance_usdc_usdt: true,
//             coinbase_usdt_usd: true,
//             polymarket: true,
//         }
//     }
// }

// /// Gathers and runs all enabled listener tasks.
// // MODIFIED: Function signature now accepts PolyMarketState.
// pub async fn run_all_listeners(
//     config: ListenerConfig,
//     state: Arc<AppState>,
//     polymarket_state: Arc<PolyMarketState>,
// ) {
//     let mut tasks: Vec<JoinHandle<()>> = Vec::new();
//     info!("--- Initializing all exchange listeners ---");

//     // --- Start Polymarket Listeners ---
//     if config.polymarket {
//         // State is now passed in, not created here.
//         tasks.push(tokio::spawn(poly_listeners_new_endpoint::polymarket_market_listener(
//             Arc::clone(&polymarket_state),
//         )));

//         tasks.push(tokio::spawn(poly_listeners_new_endpoint::polymarket_user_listener(
//             Arc::clone(&polymarket_state),
//         )));
//     }

//     // --- Start Existing Crypto Exchange Listeners ---
//     let rates = Arc::new(Rates::default());

//     if config.binance_usdc_usdt {
//         tasks.push(tokio::spawn(listeners::binance_usdc_usdt_listener(Arc::clone(&rates))));
//     }

//     if config.coinbase_usdt_usd {
//         tasks.push(tokio::spawn(listeners::coinbase_usdt_usd_listener(Arc::clone(&rates))));
//     }

//     for &crypto in &config.cryptos {
//         let prices = match crypto {
//             Crypto::BTC => Arc::clone(&state.btc_prices),
//             Crypto::ETH => Arc::clone(&state.eth_prices),
//             Crypto::XRP => Arc::clone(&state.xrp_prices),
//             Crypto::SOL => Arc::clone(&state.sol_prices),
//         };

//         for (&exchange, &(spot_enabled, perp_enabled)) in &config.exchanges {
//             if spot_enabled {
//                 let task = get_listener_task(exchange, crypto, Instrument::Spot, Arc::clone(&prices), Arc::clone(&rates));
//                 tasks.push(task);
//             }
//             if perp_enabled {
//                 let task = get_listener_task(exchange, crypto, Instrument::Perpetual, Arc::clone(&prices), Arc::clone(&rates));
//                 tasks.push(task);
//             }
//         }
//     }

//     if tasks.is_empty() {
//         warn!("No listener tasks to run. Exiting listener manager.");
//         return;
//     }

//     info!("{} listener tasks spawned. Awaiting completion...", tasks.len());
//     futures_util::future::join_all(tasks).await;
// }

// fn get_listener_task(
//     exchange: Exchange,
//     crypto: Crypto,
//     instrument: Instrument,
//     prices: Arc<DashMap<(Exchange, Instrument), f64>>,
//     rates: Arc<Rates>,
// ) -> JoinHandle<()> {
//     let is_perp = instrument == Instrument::Perpetual;
//     match exchange {
//         Exchange::Binance => tokio::spawn(listeners::binance_listener(prices, rates, crypto, is_perp)),
//         Exchange::CoinbaseLegacy => tokio::spawn(listeners::coinbase_legacy_listener(prices, rates, crypto)),
//         Exchange::CoinbaseAdvanced => tokio::spawn(listeners::coinbase_advanced_listener(prices, rates, crypto)),
//         Exchange::Bitstamp => tokio::spawn(listeners::bitstamp_listener(prices, rates, crypto)),
//         Exchange::Deribit => tokio::spawn(listeners::deribit_listener(prices, rates, crypto, is_perp)),
//         Exchange::Bitmex => tokio::spawn(listeners::bitmex_listener(prices, rates, crypto)),
//         Exchange::Bybit => tokio::spawn(listeners::bybit_listener(prices, rates, crypto, is_perp)),
//         Exchange::Okx => tokio::spawn(listeners::okx_listener(prices, rates, crypto, is_perp)),
//     }
// }
