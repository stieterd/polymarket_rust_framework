use log::{error, info};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    exchange_listeners::{
        crypto_models::{
            get_crypto_orderbook_map, get_crypto_prices_map, CryptoPrice, CryptoPriceUpdate,
        },
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth, OrderbookLevel},
        poly_models::{LegacyPriceChange, Listener, OrderSide, PriceChange},
    },
    strategies::{
        custom::tob::models::{OrderBookContext, MAX_VOLUME, TARGET_ORDER_SIZE},
        strategy_utils::{
            parse_millis, StrategyAsset, StrategyClient, StrategyOpenOrder, StrategyOrderBook,
            StrategyPosition,
        },
        Strategy, StrategyContext,
    },
};

pub struct NegRiskNoMakerStrategy;

impl Strategy for NegRiskNoMakerStrategy {
    fn name(&self) -> &'static str {
        "NegRiskNoMakerStrategy"
    }
}
