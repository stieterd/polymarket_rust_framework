use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;
use log::{error, info, warn};
use serde_json::Value;

use crate::{
    clob_client::clob_types::OrderArgs,
    exchange_listeners::{
        poly_models::{AssetOrders, OpenOrder, OrderSide, OrderState},
        states::PolyMarketState,
    },
    marketmaking::marketmakingclient::CLIENT,
};

#[derive(Debug, Default)]
pub struct PolyClient;

impl PolyClient {
    /// Places a limit order, sends it to the exchange, and records it in `poly_state.open_orders`.
    pub fn place_limit_order(
        poly_state: Arc<PolyMarketState>,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
        tick_size: &str,
        neg_risk: bool,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        if let Ok(mut rate_limit) = poly_state.rate_limit.write() {
            if rate_limit.should_wait() {
                return Err("Rate limit has been hit".into());
            }
            rate_limit.update_timestamp();
        }
        let client = Arc::clone(&CLIENT);
        let price_dec = price as f64 / 1000.0;
        let size_dec = size as f64 / 1000.0;
        // log::info!(
        //     "[PolyClient] preparing order asset={} side={:?} price_int={} size_int={} price_dec={} size_dec={} tick_size={} neg_risk={}",
        //     asset_id,
        //     side,
        //     price,
        //     size,
        //     price_dec,
        //     size_dec,
        //     tick_size,
        //     neg_risk
        // );

        let local_order =
            Self::record_order(poly_state.as_ref(), asset_id, side, price, size, 0, None)
                .ok_or_else(|| "order already exists".to_string())?;

        let order_args = OrderArgs::new(
            asset_id,
            price_dec,
            size_dec,
            side.as_str(),
            None,
            None,
            None,
            None,
        );

        let signed_order = client.create_order(&order_args, tick_size, neg_risk);
        if signed_order.order.maker_amount.is_zero() || signed_order.order.taker_amount.is_zero() {
            log::error!(
                "[PolyClient] Computed zero maker/taker amount for asset={} side={:?} price_dec={} size_dec={} tick_size={}",
                asset_id,
                side,
                price_dec,
                size_dec,
                tick_size
            );
            Self::remove_order_entry(poly_state.as_ref(), asset_id, side, price, size);
            return Err("Computed zero maker/taker amount when building order".into());
        }
        let client_clone = Arc::clone(&client);
        let poly_state_clone = Arc::clone(&poly_state);
        let asset_id_owned = asset_id.to_string();
        let local_order_clone = Arc::clone(&local_order);
        tokio::spawn(async move {
            match client_clone.post_order(&signed_order, "GTC").await {
                Ok(posted_order) => {
                    let order_id = match posted_order
                        .get("orderID")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            posted_order
                                .get("orderId")
                                .and_then(Value::as_str)
                                .map(str::to_owned)
                        }) {
                        Some(id) => id,
                        None => {
                            Self::remove_order_entry(
                                poly_state_clone.as_ref(),
                                &asset_id_owned,
                                side,
                                price,
                                size,
                            );
                            error!(
                                "orderID missing from response when placing {}",
                                asset_id_owned
                            );
                            return;
                        }
                    };

                    if let Ok(mut order) = local_order_clone.lock() {
                        order.set_id(Some(order_id));
                    } else {
                        warn!(
                        "Placed order for {} {:?} at {} / {}, but mutex was poisoned when recording id",
                        asset_id_owned, side, price, size
                    );
                    }

                    drop(posted_order);
                }
                Err(e) => {
                    error!(
                        "[PolyClient] Failed to place order for {} {:?} at {}x{}: {}",
                        asset_id_owned, side, price, size, e
                    );
                    Self::remove_order_entry(
                        poly_state_clone.as_ref(),
                        &asset_id_owned,
                        side,
                        price,
                        size,
                    );
                }
            }
        });
        Ok(())
    }

    pub fn cancel_limit_order(
        poly_state: Arc<PolyMarketState>,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let client = Arc::clone(&CLIENT);
        let order_key = (price, size);

        let order_arc = {
            let asset_orders = poly_state
                .open_orders
                .get(asset_id)
                .ok_or_else(|| "asset has no open orders".to_string())?;

            let book = match side {
                OrderSide::Buy => Arc::clone(&asset_orders.bids),
                OrderSide::Sell => Arc::clone(&asset_orders.asks),
            };

            let entry = book
                .get(&order_key)
                .ok_or_else(|| "order not found".to_string())?;
            Arc::clone(entry.value())
        };

        let order_id = {
            let mut order = order_arc
                .lock()
                .map_err(|_| "order mutex poisoned".to_string())?;
            let id = order
                .id()
                .cloned()
                .ok_or_else(|| "order id not set".to_string())?;
            order.set_state(OrderState::ToBeCanceled);
            id
        };

        let poly_state_clone = Arc::clone(&poly_state);
        let asset_id_owned = asset_id.to_string();
        let client_clone = Arc::clone(&client);
        let order_arc_clone = Arc::clone(&order_arc);
        tokio::spawn(async move {
            let id_ref = order_id.as_str();
            match client_clone.cancel_orders(&[id_ref]).await {
                Ok(resp) => {
                    let canceled_ids = resp
                        .get("canceled")
                        .and_then(Value::as_array)
                        .map(|arr| arr.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                        .unwrap_or_default();

                    if canceled_ids.iter().any(|&id| id == id_ref) {
                        Self::remove_order_entry(
                            poly_state_clone.as_ref(),
                            &asset_id_owned,
                            side,
                            price,
                            size,
                        );
                    } else {
                        if let Ok(mut order) = order_arc_clone.lock() {
                            order.set_state(OrderState::Live);
                        }
                        error!("Order {} not canceled for asset {}", id_ref, asset_id_owned);
                    }
                }
                Err(e) => {
                    error!(
                        "Failed to cancel order {} for asset {}: {}",
                        id_ref, asset_id_owned, e
                    );
                    if let Ok(mut order) = order_arc_clone.lock() {
                        order.set_state(OrderState::Live);
                    }
                }
            }
        });

        Ok(())
    }

    fn record_order(
        poly_state: &PolyMarketState,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
        size_filled: u32,
        order_id: Option<String>,
    ) -> Option<Arc<Mutex<OpenOrder>>> {
        let order_key = (price, size);
        let order = Arc::new(Mutex::new(OpenOrder::new(
            asset_id.to_string(),
            price,
            size,
            size_filled,
            order_id,
        )));

        if let Some(asset_orders) = poly_state.open_orders.get_mut(asset_id) {
            let book = match side {
                OrderSide::Buy => Arc::clone(&asset_orders.bids),
                OrderSide::Sell => Arc::clone(&asset_orders.asks),
            };
            drop(asset_orders);

            // Only insert if there is not already an order at that price/size
            if book.contains_key(&order_key) {
                if let Some(existing) = book.get(&order_key) {
                    if let Ok(existing_order) = existing.lock() {
                        warn!(
                            "Order {:?} for {} at price {} size {} already exists; not replacing",
                            side,
                            existing_order.asset(),
                            existing_order.price(),
                            existing_order.size()
                        );
                    } else {
                        warn!(
                            "Order {:?} for {} at price {} size {} already exists, but it was poisoned; not replacing",
                            side,
                            asset_id,
                            price,
                            size
                        );
                    }
                } else {
                    warn!(
                        "Order {:?} for {} at price {} size {} already exists, but could not acquire lock; not replacing",
                        side,
                        asset_id,
                        price,
                        size
                    );
                }

                return None;
            } else {
                book.insert(order_key, Arc::clone(&order));
            }
        } else {
            let bids = DashMap::new();
            let asks = DashMap::new();

            match side {
                OrderSide::Buy => {
                    bids.insert(order_key, Arc::clone(&order));
                }
                OrderSide::Sell => {
                    asks.insert(order_key, Arc::clone(&order));
                }
            }

            poly_state
                .open_orders
                .insert(asset_id.to_string(), AssetOrders::new(bids, asks));
        }

        Some(order)
    }

    fn remove_order_entry(
        poly_state: &PolyMarketState,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
    ) -> Option<Arc<Mutex<OpenOrder>>> {
        let order_key = (price, size);

        if let Some(asset_orders) = poly_state.open_orders.get_mut(asset_id) {
            let book = match side {
                OrderSide::Buy => Arc::clone(&asset_orders.bids),
                OrderSide::Sell => Arc::clone(&asset_orders.asks),
            };
            drop(asset_orders);

            book.remove(&order_key).map(|(_, arc)| arc)
        } else {
            None
        }
    }

}
