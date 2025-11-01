use async_trait::async_trait;
use log::info;

use crate::{
    exchange_listeners::{
        crypto_models::CryptoPriceUpdate,
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth},
        poly_models::{LegacyPriceChange, Listener, PriceChange},
        Crypto, Exchange, Instrument,
    },
    strategies::{Strategy, StrategyContext},
};

pub struct OrderLoggingStrategy;

impl OrderLoggingStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Strategy for OrderLoggingStrategy {
    fn name(&self) -> &'static str {
        "OrderLogger"
    }

    async fn poly_handle_user_order(
        &self,
        _ctx: &StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::OrderPayload,
    ) {
        // info!("Just received a message from {}", _exchange);
        for asset_entry in _ctx.poly_state.open_orders.iter() {
            let asset_id = asset_entry.key();
            let asset_orders = asset_entry.value();
            
            // Iterate over bids
            for bid_entry in asset_orders.bids.iter() {
                let (price, size) = *bid_entry.key();
                if let Ok(order) = bid_entry.value().lock() {
                    info!(
                        "[OrderLogger] Asset: {}, Side: Buy, Price: {}, Size: {}, Size Filled: {}, State: {:?}, ID: {:?}",
                        asset_id,
                        price,
                        size,
                        order.size_filled(),
                        order.state(),
                        order.id(),
                    );
                }
            }
            
            // Iterate over asks
            for ask_entry in asset_orders.asks.iter() {
                let (price, size) = *ask_entry.key();
                if let Ok(order) = ask_entry.value().lock() {
                    info!(
                        "[OrderLogger] Asset: {}, Side: Sell, Price: {}, Size: {}, Size Filled: {}, State: {:?}, ID: {:?}",
                        asset_id,
                        price,
                        size,
                        order.size_filled(),
                        order.state(),
                        order.id(),
                    );
                }
            }
        }
    }
}
