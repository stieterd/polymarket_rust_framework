use std::sync::Arc;

use crate::{
    credentials,
    exchange_listeners::poly_models::{
        Listener, OrderEventType, OrderSide, TradeRole, TradeStatus,
    },
    strategies::Strategy,
};
use log::{info, warn};

pub struct UpdateOrderStrategy;

impl UpdateOrderStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl Strategy for UpdateOrderStrategy {
    fn name(&self) -> &'static str {
        "UpdateOrders"
    }

    fn poly_handle_user_order(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::OrderPayload,
    ) {
        let crate::exchange_listeners::poly_models::OrderPayload {
            id,
            asset_id,
            order_event_type,
            price,
            side,
            original_size,
            associate_trades,
            event_type,
            market,
            order_owner,
            outcome,
            owner,
            size_matched,
            timestamp,
            status,
        } = &_payload;

        let price_u32 = match price.parse::<f64>() {
            Ok(val) => (val * 1000.0).round() as u32,
            Err(e) => {
                warn!(
                    "[{}] Failed to parse price '{}' for asset {}: {}",
                    self.name(),
                    price,
                    asset_id,
                    e
                );
                return;
            }
        };
        let size_u32 = match original_size.parse::<f64>() {
            Ok(val) => (val * 1000.0).round() as u32,
            Err(e) => {
                warn!(
                    "[{}] Failed to parse size '{}' for asset {}: {}",
                    self.name(),
                    original_size,
                    asset_id,
                    e
                );
                return;
            }
        };

        let size_matched_u32 = match size_matched.parse::<f64>() {
            Ok(val) => (val * 1000.0).round() as u32,
            Err(e) => {
                warn!(
                    "[{}] Failed to parse size_matched '{}' for asset {}: {}",
                    self.name(),
                    size_matched,
                    asset_id,
                    e
                );
                return;
            }
        };

        match OrderEventType::from_str(order_event_type.as_str()) {
            Some(OrderEventType::PLACEMENT) => {
                // Handle new order placement
                // Place order in bids/asks as appropriate, etc.
                // For now, we just log placement

                // info!(
                //     "[{}] Order placement event for id={} asset={} price={} size={}",
                //     self.name(),
                //     id,
                //     asset_id,
                //     price,
                //     original_size
                // );

                let order_arc_opt = _ctx
                    .poly_state
                    .open_orders
                    .get_mut(asset_id)
                    .and_then(|asset_orders| match OrderSide::from_str(side.as_str()) {
                        Some(OrderSide::Buy) => Some(Arc::clone(&asset_orders.bids)),
                        Some(OrderSide::Sell) => Some(Arc::clone(&asset_orders.asks)),
                        None => {
                            warn!(
                                "[{}] Unknown side '{}' for asset {}",
                                self.name(),
                                side,
                                asset_id
                            );
                            None
                        }
                    })
                    .and_then(|orders_map| {
                        orders_map
                            .get(&(price_u32, size_u32))
                            .map(|entry| Arc::clone(entry.value()))
                    });

                if let Some(order_arc) = order_arc_opt {
                    match order_arc.lock() {
                        Ok(mut order) => {
                            order.set_id(Some(id.clone()));
                        }
                        Err(poisoned) => {
                            warn!(
                                "[{}] Mutex poisoned when updating order id for asset {} price={} size={}",
                                self.name(),
                                asset_id,
                                price,
                                original_size
                            );
                            let mut order = poisoned.into_inner();
                            order.set_id(Some(id.clone()));
                            order.set_size_filled(size_matched_u32);
                        }
                    }
                }
            }

            Some(OrderEventType::UPDATE) => {
                // Handle order update
                // info!(
                //     "[{}] Order update event for id={} asset={} price={} size={}",
                //     self.name(),
                //     id,
                //     asset_id,
                //     price,
                //     original_size
                // );

                if status.eq_ignore_ascii_case("LIVE") {
                    let order_arc_opt = _ctx
                        .poly_state
                        .open_orders
                        .get_mut(asset_id)
                        .and_then(|asset_orders| match OrderSide::from_str(side.as_str()) {
                            Some(OrderSide::Buy) => Some(Arc::clone(&asset_orders.bids)),
                            Some(OrderSide::Sell) => Some(Arc::clone(&asset_orders.asks)),
                            None => {
                                warn!(
                                    "[{}] Unknown side '{}' for asset {}",
                                    self.name(),
                                    side,
                                    asset_id
                                );
                                None
                            }
                        })
                        .and_then(|orders_map| {
                            orders_map
                                .get(&(price_u32, size_u32))
                                .map(|entry| Arc::clone(entry.value()))
                        });

                    if let Some(order_arc) = order_arc_opt {
                        match order_arc.lock() {
                            Ok(mut order) => {
                                order.set_size_filled(size_matched_u32);
                            }
                            Err(poisoned) => {
                                warn!(
                                    "[{}] Mutex poisoned when updating order size for asset {} price={} size={}",
                                    self.name(),
                                    asset_id,
                                    price,
                                    original_size
                                );
                                let mut order = poisoned.into_inner();
                                order.set_size_filled(size_matched_u32);
                            }
                        }
                    }
                } else if status.eq_ignore_ascii_case("MATCHED") {
                    if let Some(asset_orders) = _ctx.poly_state.open_orders.get_mut(asset_id) {
                        match OrderSide::from_str(side.as_str()) {
                            Some(OrderSide::Buy) => {
                                if asset_orders.bids.remove(&(price_u32, size_u32)).is_none() {
                                    warn!(
                                        "[{}] No open bid to cancel for asset={} price={} size={}",
                                        self.name(),
                                        asset_id,
                                        price,
                                        original_size
                                    );
                                }
                            }
                            Some(OrderSide::Sell) => {
                                if asset_orders.asks.remove(&(price_u32, size_u32)).is_none() {
                                    warn!(
                                        "[{}] No open ask to cancel for asset={} price={} size={}",
                                        self.name(),
                                        asset_id,
                                        price,
                                        original_size
                                    );
                                }
                            }
                            None => {
                                warn!(
                                    "[{}] Unknown side '{}' for asset {}",
                                    self.name(),
                                    side,
                                    asset_id
                                );
                            }
                        }
                    }
                }
            }
            Some(OrderEventType::CANCELLATION) => {
                // Handle order cancellation
                // info!(
                //     "[{}] Order cancellation event for id={} asset={} price={} size={}",
                //     self.name(),
                //     id,
                //     asset_id,
                //     price,
                //     original_size
                // );

                if let Some(asset_orders) = _ctx.poly_state.open_orders.get_mut(asset_id) {
                    match OrderSide::from_str(side.as_str()) {
                        Some(OrderSide::Buy) => {
                            if asset_orders.bids.remove(&(price_u32, size_u32)).is_none() {
                                warn!(
                                    "[{}] No open bid to cancel for asset={} price={} size={}",
                                    self.name(),
                                    asset_id,
                                    price,
                                    original_size
                                );
                            }
                        }
                        Some(OrderSide::Sell) => {
                            if asset_orders.asks.remove(&(price_u32, size_u32)).is_none() {
                                warn!(
                                    "[{}] No open ask to cancel for asset={} price={} size={}",
                                    self.name(),
                                    asset_id,
                                    price,
                                    original_size
                                );
                            }
                        }
                        None => {
                            warn!(
                                "[{}] Unknown side '{}' for asset {}",
                                self.name(),
                                side,
                                asset_id
                            );
                        }
                    }
                }
            }
            None => {
                warn!(
                    "[{}] Unknown order_event_type '{}' for order id={} asset={}",
                    self.name(),
                    order_event_type,
                    id,
                    asset_id
                );
            }
        }
    }

    fn poly_handle_user_trade(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TradePayload,
    ) {
        if _payload.status != TradeStatus::Matched {
            return;
        }

        match _payload.trade_role {
            TradeRole::Taker => {
                let asset_id = _payload.asset_id.clone();
                let size_str = _payload.size.clone();
                let price_str = _payload.price.clone();

                let price_u32 = match price_str.parse::<f64>() {
                    Ok(val) => (val * 1000.0).round() as u32,
                    Err(e) => {
                        warn!(
                            "[{}] Failed to parse size '{}' for asset {}: {}",
                            self.name(),
                            price_str,
                            asset_id,
                            e
                        );
                        return;
                    }
                };

                let size_u32 = match size_str.parse::<f64>() {
                    Ok(val) => (val * 1000.0).round() as u32,
                    Err(e) => {
                        warn!(
                            "[{}] Failed to parse size '{}' for asset {}: {}",
                            self.name(),
                            size_str,
                            asset_id,
                            e
                        );
                        return;
                    }
                };

                if let Some(asset_orders) = _ctx.poly_state.open_orders.get_mut(&asset_id) {
                    match OrderSide::from_str(_payload.side.as_str()) {
                        Some(OrderSide::Buy) => {
                            if asset_orders.bids.remove(&(price_u32, size_u32)).is_none() {
                                warn!(
                                    "[{}] No open bid to fill (TAKER) for asset={} price={} size={}",
                                    self.name(),
                                    asset_id,
                                    price_str,
                                    size_str
                                );
                            }
                        }
                        Some(OrderSide::Sell) => {
                            if asset_orders.asks.remove(&(price_u32, size_u32)).is_none() {
                                warn!(
                                    "[{}] No open ask to fill (TAKER) for asset={} price={} size={}",
                                    self.name(),
                                    asset_id,
                                    price_str,
                                    size_str
                                );
                            }
                        }
                        None => {
                            warn!(
                                "[{}] Unknown side '{}' for matched trade asset={}",
                                self.name(),
                                _payload.side,
                                asset_id
                            );
                        }
                    };
                }
                // info!(
                //     "[{}] Matched TAKER trade id={} asset={} price={} size={}",
                //     self.name(),
                //     _payload.id,
                //     _payload.asset_id,
                //     _payload.price,
                //     _payload.size
                // );
            }
            TradeRole::Maker => {
                // info!(
                //     "[{}] Matched MAKER trade id={} asset={} price={} size={}",
                //     self.name(),
                //     _payload.id,
                //     _payload.asset_id,
                //     _payload.price,
                //     _payload.size
                // );
            }
            TradeRole::Unknown => {
                warn!(
                    "[{}] Matched trade with unknown trader_side for id={} asset={}",
                    self.name(),
                    _payload.id,
                    _payload.asset_id
                );
            }
        }
    }
}
