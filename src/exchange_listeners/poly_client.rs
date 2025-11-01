use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use dashmap::DashMap;
use log::warn;
use serde_json::Value;

use crate::{
    clob_client::clob_types::OrderArgs,
    exchange_listeners::{
        poly_models::{AssetOrders, OpenOrder, OrderSide},
        states::PolyMarketState,
    },
    marketmaking::marketmakingclient::CLIENT,
};

#[derive(Debug, Default)]
pub struct PolyClient;

impl PolyClient {
    /// Places a limit order, sends it to the exchange, and records it in `poly_state.open_orders`.
    pub async fn place_limit_order(
        poly_state: &PolyMarketState,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
        tick_size: &str,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let client = Arc::clone(&CLIENT);
        let price_dec = price as f64 / 1000.0;
        let size_dec = size as f64 / 1000.0;

        let local_order = Self::record_order(poly_state, asset_id, side, price, size, 0, None)
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

        let signed_order = client.create_order(&order_args, tick_size, true);
        match client.post_order(&signed_order, "GTC").await {
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
                        Self::remove_order_entry(poly_state, asset_id, side, price, size);
                        return Err("orderID missing from response".into());
                    }
                };

                if let Ok(mut order) = local_order.lock() {
                    order.set_id(Some(order_id));
                } else {
                    warn!(
                        "Placed order for {} {:?} at {} / {}, but mutex was poisoned when recording id",
                        asset_id, side, price, size
                    );
                }

                Ok(posted_order)
            }
            Err(e) => {
                Self::remove_order_entry(poly_state, asset_id, side, price, size);
                Err(e)
            }
        }
    }

    pub async fn cancel_limit_order(
        poly_state: &PolyMarketState,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let client = Arc::clone(&CLIENT);
        let order_key = (price, size);

        let (order_arc, book) = {
            let asset_orders = poly_state
                .open_orders
                .get(asset_id)
                .ok_or_else(|| "asset has no open orders".to_string())?;

            let book = match side {
                OrderSide::Buy => Arc::clone(&asset_orders.bids),
                OrderSide::Sell => Arc::clone(&asset_orders.asks),
            };

            let order_arc = {
                let entry = book
                    .get(&order_key)
                    .ok_or_else(|| "order not found".to_string())?;
                Arc::clone(entry.value())
            };

            (order_arc, book)
        };

        let order_id = {
            let order = order_arc
                .lock()
                .map_err(|_| "order mutex poisoned".to_string())?;
            order
                .id()
                .cloned()
                .ok_or_else(|| "order id not set".to_string())?
        };

        let removed_order = book.remove(&order_key).map(|(_, arc)| arc);

        let id_ref = order_id.as_str();
        match client.cancel_orders(&[id_ref]).await {
            Ok(resp) => {
                let canceled_ids = resp
                    .get("canceled")
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                    .unwrap_or_default();

                if canceled_ids.iter().any(|&id| id == id_ref) {
                    Ok(resp)
                } else {
                    if let Some(arc) = removed_order {
                        Self::restore_order_entry(poly_state, asset_id, side, price, size, arc);
                    }
                    Err("order not canceled".into())
                }
            }
            Err(e) => {
                if let Some(arc) = removed_order {
                    Self::restore_order_entry(poly_state, asset_id, side, price, size, arc);
                }
                Err(e)
            }
        }
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

    fn restore_order_entry(
        poly_state: &PolyMarketState,
        asset_id: &str,
        side: OrderSide,
        price: u32,
        size: u32,
        order: Arc<Mutex<OpenOrder>>,
    ) {
        let order_key = (price, size);

        if let Some(asset_orders) = poly_state.open_orders.get_mut(asset_id) {
            let book = match side {
                OrderSide::Buy => Arc::clone(&asset_orders.bids),
                OrderSide::Sell => Arc::clone(&asset_orders.asks),
            };
            drop(asset_orders);

            book.insert(order_key, order);
        } else {
            let bids = DashMap::new();
            let asks = DashMap::new();

            match side {
                OrderSide::Buy => {
                    bids.insert(order_key, order);
                }
                OrderSide::Sell => {
                    asks.insert(order_key, order);
                }
            }

            poly_state
                .open_orders
                .insert(asset_id.to_string(), AssetOrders::new(bids, asks));
        }
    }
}
