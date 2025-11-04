use std::{collections::HashMap, sync::Arc};
use ethers::abi::Hash;
use log::error;

use crate::{exchange_listeners::{orderbooks::poly_orderbook::OrderBook, poly_client::PolyClient, poly_models::{AssetSide, OrderSide}}, marketmaking::poly_market_struct::Market, strategies::{Strategy, StrategyContext}};

pub struct StrategyOrderBook;
pub struct StrategyOpenOrder;
pub struct StrategyAsset;
pub struct StrategyClient;
pub struct StrategyPosition;

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

    pub fn is_yes_market(ctx: &StrategyContext, asset_id: &str) -> bool {
        ctx.poly_state
            .markets
            .get(asset_id)
            .and_then(|m| m.is_yes_market.clone())
            .unwrap_or(false)
    }

    pub fn get_market(ctx: &StrategyContext, asset_id: &str) -> Arc<Market> {
        ctx.poly_state
            .markets
            .get(asset_id).unwrap().clone()
    }

    pub fn get_other_side(_ctx: &StrategyContext, asset_id: &str, assets_in_market: &Vec<String>) -> String {
        assets_in_market
            .iter()
            .find(|&id| id != asset_id)
            .cloned()
            .unwrap_or_default()
    }

    pub fn get_yes_and_no(ctx: &StrategyContext, asset_id: &str) -> Vec<String> {
        ctx.poly_state
            .markets
            .get(asset_id)
            .and_then(|m| m.clobTokenIds.as_ref())
            .and_then(|clob_token_ids| serde_json::from_str::<Vec<String>>(clob_token_ids).ok())
            .unwrap_or_default()
    }

    // EXPENSIVE METHOD, TRY TO USE IT AS FEW AS POSSIBLE
    pub fn get_negrisk_assets(
        ctx: &StrategyContext,
        asset_id: &str,
    ) -> HashMap<AssetSide, Vec<String>> {
        let mut result = HashMap::new();
        result.insert(AssetSide::YES, Vec::new());
        result.insert(AssetSide::NO, Vec::new());

        let market = match ctx.poly_state.markets.get(asset_id) {
            Some(m) => m,
            None => return result,
        };
        let neg_risk_market_id = match market.negRiskMarketID.as_ref() {
            Some(id) => id,
            None => return result,
        };

        for (other_asset_id, other_market) in ctx.poly_state.markets.iter() {
            if other_market
                .negRiskMarketID
                .as_ref()
                .map(|id| id == neg_risk_market_id)
                .unwrap_or(false)
            {
                match other_market.is_yes_market {
                    Some(true) => {
                        if let Some(list) = result.get_mut(&AssetSide::YES) {
                            list.push(other_asset_id.clone());
                        }
                    }
                    Some(false) => {
                        if let Some(list) = result.get_mut(&AssetSide::NO) {
                            list.push(other_asset_id.clone());
                        }
                    }
                    None => {}
                }
            }
        }

        result
    }
}

impl StrategyPosition {
    pub fn asset_position(ctx: &StrategyContext, asset_id: &str) -> u32 {
        ctx.poly_state
            .positions
            .get(asset_id)
            .and_then(|position_lock| position_lock.read().ok().map(|position| position.size))
            .unwrap_or(0)
    }

    pub fn assets_position_map(ctx: &StrategyContext, assets: &[String]) -> HashMap<String, u32> {
        assets
            .iter()
            .map(|asset_id| {
                let size = Self::asset_position(ctx, asset_id);
                (asset_id.clone(), size)
            })
            .collect()
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
