use std::sync::Arc;
use log::error;

use crate::{exchange_listeners::{orderbooks::poly_orderbook::OrderBook, poly_client::PolyClient, poly_models::OrderSide}, strategies::StrategyContext};

pub struct StrategyOrderBook;
pub struct StrategyOpenOrder;
pub struct StrategyAsset;
pub struct StrategyClient;


pub fn parse_millis(numeric: &str) -> Result<u32, String> {
    numeric
        .parse::<f64>()
        .map(|numeric_f| (numeric_f * 1000.0).round() as u32)
        .map_err(|err| format!("Failed to parse '{}' as f64: {}", numeric, err))
}


impl StrategyOrderBook{

    pub fn price_matches_top_of_book(orderbook: &OrderBook, price: u32) -> bool {
        let bid_matches = orderbook
            .best_bid()
            .map(|(bid_price, _)| bid_price == price)
            .unwrap_or(false);
        let ask_matches = orderbook
            .best_ask()
            .map(|(ask_price, _)| ask_price == price)
            .unwrap_or(false);

        bid_matches || ask_matches
    }
}

impl StrategyOpenOrder{
    pub fn order_exists(ctx: &StrategyContext, asset_id: &str, side: OrderSide, price: u32, size: u32) -> bool {
        let order_exists = ctx
                .poly_state
                .open_orders
                .get(asset_id)
                .map(|orders| {
                    orders.order_exists(
                        side,
                        price,
                        size,
                    )
                })
                .unwrap_or(false);
        
        order_exists
    }

    pub fn collect_orders_asset(
        ctx: &StrategyContext,
        asset_id: &str,
    ) -> Vec<(OrderSide, u32, u32)> {
        ctx.poly_state
            .open_orders
            .get(asset_id)
            .map(|orders| {
                orders
                    .bids
                    .iter()
                    .map(|entry| (OrderSide::Buy, entry.key().0, entry.key().1))
                    .chain(
                        orders
                            .asks
                            .iter()
                            .map(|entry| (OrderSide::Sell, entry.key().0, entry.key().1)),
                    )
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl StrategyAsset{
    pub fn is_negrisk(ctx: &StrategyContext, asset_id: &str) -> bool {
        ctx.poly_state
            .markets
            .get(asset_id)
            .and_then(|m| m.negRisk.clone())
            .unwrap_or(false)
    }
}

impl StrategyClient {
    
    pub fn cancel_orders(
        ctx: Arc<StrategyContext>,
        asset_id: &str,
        orders_to_cancel: Vec<(OrderSide, u32, u32)>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        
        if orders_to_cancel.is_empty() {
            return Ok(());
        }

        for (side, price, size) in orders_to_cancel {
            PolyClient::cancel_limit_order(
                Arc::clone(&ctx.poly_state),
                asset_id,
                side,
                price,
                size,
            )?;
        }

        Ok(())
    }

    pub fn place_limit_order(
        ctx: Arc<StrategyContext>,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
        tick_size: &str,
        neg_risk: bool,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        PolyClient::place_limit_order(
            Arc::clone(&ctx.poly_state),
            asset_id,
            side,
            price,
            size,
            tick_size,
            neg_risk,
        )
    }
}
