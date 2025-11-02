use log::error;
use serde_json::{json, Map, Value};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;

use crate::exchange_listeners::crypto_models::{get_crypto_prices_map, CryptoPriceUpdate};
use crate::exchange_listeners::orderbooks::{OrderbookDepth, OrderbookLevel};
use crate::exchange_listeners::poly_models::{AggOrderbook, Listener, PriceChange};
use crate::exchange_listeners::{Crypto, Exchange, Instrument};
use crate::strategies::StrategyContext;
use crate::Strategy;

#[derive(Default)]
pub struct CryptoLoggingStrategy;

impl Strategy for CryptoLoggingStrategy {
    fn name(&self) -> &'static str {
        "CryptoLogging"
    }

    fn crypto_handle_price_update(
        &self,
        ctx: Arc<StrategyContext>,
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        depth: OrderbookDepth,
        _price_update: &CryptoPriceUpdate,
    ) {
        if depth == OrderbookDepth::L1 {
            // --- THE DEFINITIVE FIX: Read, Release, then Format & Print ---
            // 1. Get the data and release the lock immediately.
            let price_to_log = {
                let prices_map = get_crypto_prices_map(ctx.app_state.clone(), crypto);
                // The `get` returns a temporary Ref (a lock guard). We copy the value
                // out and then the Ref is immediately dropped at the semicolon.
                prices_map
                    .get(&(exchange, instrument, depth))
                    .map(|p| *p.value())
            }; // The lock is guaranteed to be gone here.

            // 2. Now, safely use the copied data.
            if let Some(crypto_price) = price_to_log {
                let log_message = format!(
                    "L1 Price | {:<8} | {:<4} | {:<9} | Midpoint: {:>10.2}",
                    exchange.as_str(),
                    crypto.to_string(),
                    format!("{:?}", instrument),
                    crypto_price.midpoint
                );
                // println!("{}", log_message);
            }
        }
    }

    // ADDED: This function will now log the initial state received from Kraken.
    fn crypto_handle_l2_snapshot(
        &self,
        ctx: Arc<StrategyContext>,
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        _bids: &[OrderbookLevel],
        _asks: &[OrderbookLevel],
    ) {
        // This logic is identical to crypto_handle_l2_update, ensuring snapshots are logged.
        if exchange == Exchange::Kraken {
            let price_to_log = {
                let prices_map = get_crypto_prices_map(ctx.app_state.clone(), crypto);
                let key = (exchange, instrument, OrderbookDepth::L2);
                prices_map.get(&key).map(|p| *p.value())
            };

            if let Some(crypto_price) = price_to_log {
                let log_message = format!(
                    "L2 Snapshot | {:<8} | {:<4} | {:<9} | Midpoint: {:>10.2}", // Changed "Price" to "Snapshot" for clarity
                    exchange.as_str(),
                    crypto.to_string(),
                    format!("{:?}", instrument),
                    crypto_price.midpoint
                );
                // println!("{}", log_message);
            }
        }
    }

    fn crypto_handle_l2_update(
        &self,
        ctx: Arc<StrategyContext>,
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        _bids: &[OrderbookLevel],
        _asks: &[OrderbookLevel],
    ) {
        let price_to_log = {
            let prices_map = get_crypto_prices_map(ctx.app_state.clone(), crypto);
            let key = (exchange, instrument, OrderbookDepth::L2);
            prices_map.get(&key).map(|p| *p.value())
        };

        if let Some(crypto_price) = price_to_log {
            if exchange == Exchange::Kraken {
                let log_message = format!(
                    "L2 Update | {:<8} | {:<4} | {:<9} | Midpoint: {:>10.2}", // Changed "Price" to "Update" for clarity
                    exchange.as_str(),
                    crypto.to_string(),
                    format!("{:?}", instrument),
                    crypto_price.midpoint
                );
                // println!("{}", log_message);
            } else if exchange == Exchange::Deribit {
                let log_message = format!(
                    "L2 Update | {:<8} | {:<4} | {:<9} | Midpoint: {:>10.2} | Custom VWAP: {:>10.2}",
                    exchange.as_str(),
                    crypto.to_string(),
                    format!("{:?}", instrument),
                    crypto_price.midpoint,
                    crypto_price.price
                );
                // println!("{}", log_message);
            }
        }
    }
}
