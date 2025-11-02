use crate::credentials;
use crate::exchange_listeners::crypto_models::{
    get_crypto_orderbook_map, Crypto, CryptoPriceUpdate, Exchange, Instrument, RateKind,
};
use crate::exchange_listeners::orderbooks::poly_orderbook::OrderBook;
use crate::exchange_listeners::orderbooks::{CryptoOrderbook, OrderbookDepth, OrderbookLevel};
use crate::exchange_listeners::poly_models::{
    AggOrderbook, Listener, OrderPayload, PolymarketMessageWrapper, PolymarketMessageWrapperOld,
    Position, PriceChange, PriceChangePayload, TickSizeChangePayload, TradePayload,
};

use crate::exchange_listeners::states::{AppState, PolyMarketState};
use crate::strategies::{Strategy, StrategyContext};
use std::sync::Arc;
use dashmap::mapref::entry::Entry;
use log::{debug, error, info, warn};
use simd_json::{
    prelude::{ValueAsScalar, ValueObjectAccess},
    value::owned::Value as OwnedValue,
};
use simd_json::{to_borrowed_value, BorrowedValue};
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc::{self, UnboundedSender};

pub type SocketEventSender = UnboundedSender<SocketEvent>;

#[derive(Debug, Clone)]
pub enum SocketEvent {
    Market {
        listener: Listener,
        payload: Vec<u8>,
    },
    User {
        listener: Listener,
        payload: Vec<u8>,
    },
    Rate {
        source: &'static str,
        kind: RateKind,
        value: f64,
    },
    ClearRate {
        kind: RateKind,
    },
    Price {
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        depth: OrderbookDepth,
        price_update: CryptoPriceUpdate,
    },
    L2Snapshot {
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        bids: Vec<OrderbookLevel>,
        asks: Vec<OrderbookLevel>,
    },
    L2Update {
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        bids: Vec<OrderbookLevel>,
        asks: Vec<OrderbookLevel>,
    },
    ClearPrice {
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
    },
}

#[derive(Clone)]
pub struct CountingSender {
    event_tx: SocketEventSender,
    pending: Arc<AtomicUsize>,
}

impl CountingSender {
    pub fn send(&self, event: SocketEvent) -> Result<(), mpsc::error::SendError<SocketEvent>> {
        self.pending.fetch_add(1, Ordering::SeqCst);
        match self.event_tx.send(event) {
            Ok(()) => Ok(()),
            Err(err) => {
                self.pending.fetch_sub(1, Ordering::SeqCst);
                Err(err)
            }
        }
    }

    pub fn pending(&self) -> usize {
        self.pending.load(Ordering::SeqCst)
    }
}

pub fn spawn_event_processor(
    app_state: Arc<AppState>,
    poly_state: Arc<PolyMarketState>,
    strategies: Vec<Arc<dyn Strategy>>,
) -> Arc<CountingSender> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let pending = Arc::new(AtomicUsize::new(0));

    let processor = EventProcessor {
        poly_state,
        app_state,
        strategies,
    };
    let pending_clone = Arc::clone(&pending);

    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            processor.handle_event(event);
            pending_clone.fetch_sub(1, Ordering::SeqCst);
        }
    });

    Arc::new(CountingSender {
        event_tx: tx,
        pending,
    })
}

struct EventProcessor {
    poly_state: Arc<PolyMarketState>,
    app_state: Arc<AppState>,
    strategies: Vec<Arc<dyn Strategy>>,
}

impl EventProcessor {
    fn handle_event(&self, event: SocketEvent) {
        match event {
            SocketEvent::Market { listener, payload } => {
                self.handle_market_event(listener, payload)
            }
            SocketEvent::User { listener, payload } => self.handle_user_event(listener, payload),
            SocketEvent::Rate {
                source,
                kind,
                value,
            } => {}
            SocketEvent::ClearRate { kind } => {}
            SocketEvent::Price {
                exchange,
                instrument,
                crypto,
                depth,
                price_update,
            } => self.handle_price_update(exchange, instrument, crypto, depth, &price_update),
            SocketEvent::L2Snapshot {
                exchange,
                instrument,
                crypto,
                bids,
                asks,
            } => self.handle_l2_snapshot(exchange, instrument, crypto, &bids, &asks),
            SocketEvent::L2Update {
                exchange,
                instrument,
                crypto,
                bids,
                asks,
            } => self.handle_l2_update(exchange, instrument, crypto, &bids, &asks),
            SocketEvent::ClearPrice {
                exchange,
                instrument,
                crypto,
            } => self.handle_price_clear(exchange, instrument, crypto),
        }
    }

    fn strategy_context(&self) -> Arc<StrategyContext> {
        Arc::new(StrategyContext::new(
            Arc::clone(&self.app_state),
            Arc::clone(&self.poly_state),
        ))
    }

    fn handle_price_update(
        &self,
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        depth: OrderbookDepth,
        price_update: &CryptoPriceUpdate,
    ) {
        let ctx = self.strategy_context();

        for strategy in &self.strategies {
            strategy.crypto_handle_price_update(
                Arc::clone(&ctx),
                exchange,
                instrument,
                crypto,
                depth,
                price_update,
            );
        }
    }

    fn handle_l2_snapshot(
        &self,
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        bids: &[OrderbookLevel],
        asks: &[OrderbookLevel],
    ) {
        let ctx = self.strategy_context();
        for strategy in &self.strategies {
            strategy.crypto_handle_l2_snapshot(
                Arc::clone(&ctx),
                exchange,
                instrument,
                crypto,
                bids,
                asks,
            );
        }
    }

    fn handle_l2_update(
        &self,
        exchange: Exchange,
        instrument: Instrument,
        crypto: Crypto,
        bids: &[OrderbookLevel],
        asks: &[OrderbookLevel],
    ) {
        let ctx = self.strategy_context();
        for strategy in &self.strategies {
            strategy.crypto_handle_l2_update(
                Arc::clone(&ctx),
                exchange,
                instrument,
                crypto,
                bids,
                asks,
            );
        }
    }

    fn handle_price_clear(&self, exchange: Exchange, instrument: Instrument, crypto: Crypto) {
        let map = get_crypto_orderbook_map(Arc::clone(&self.app_state), crypto);
        let mut depths = Vec::new();
        if map
            .remove(&(exchange, instrument, OrderbookDepth::L1))
            .is_some()
        {
            depths.push(OrderbookDepth::L1);
        }
        if map
            .remove(&(exchange, instrument, OrderbookDepth::L2))
            .is_some()
        {
            depths.push(OrderbookDepth::L2);
        }

        if !depths.is_empty() {
            let ctx = self.strategy_context();
            for depth in depths {
                for strategy in &self.strategies {
                    strategy.crypto_handle_price_clear(
                        Arc::clone(&ctx),
                        exchange,
                        instrument,
                        crypto,
                        depth,
                    );
                }
            }
        }
    }

    fn handle_market_event(&self, listener: Listener, mut payload: Vec<u8>) {
        if payload.is_empty() {
            return;
        }
        if listener.is_legacy() {
            if let Ok(s) = str::from_utf8(&payload) {
                let t = s.trim();
                if t.eq_ignore_ascii_case("PONG") {
                    let ctx = self.strategy_context();
                    for strategy in &self.strategies {
                        strategy.poly_handle_market_pong(Arc::clone(&ctx), listener);
                    }
                    return;
                }
            }

            let first = payload.iter().copied().find(|b| !b.is_ascii_whitespace());

            match first {
                Some(b'[') => {
                    // Batch
                    match simd_json::from_slice::<Vec<PolymarketMessageWrapperOld>>(
                        payload.as_mut_slice(),
                    ) {
                        Ok(wrappers) => {
                            for w in wrappers {
                                self.dispatch_market_message_legacy(listener, w);
                            }
                        }
                        Err(e) => error!(
                            "[{}] Failed to parse legacy market message batch: {}. Raw: {}",
                            listener,
                            e,
                            String::from_utf8_lossy(&payload)
                        ),
                    }
                }
                _ => {
                    // Single
                    match simd_json::from_slice::<PolymarketMessageWrapperOld>(
                        payload.as_mut_slice(),
                    ) {
                        Ok(w) => self.dispatch_market_message_legacy(listener, w),
                        Err(e) => error!(
                            "[{}] Failed to parse legacy market message wrapper: {}. Raw: {}",
                            listener,
                            e,
                            String::from_utf8_lossy(&payload)
                        ),
                    }
                }
            }
            return;
        }

        match simd_json::from_slice::<PolymarketMessageWrapper>(payload.as_mut_slice()) {
            Ok(wrapper) => {
                self.dispatch_market_message(listener, wrapper);
            }
            Err(e) => error!(
                "[{}] Failed to parse market message wrapper: {}. Raw: {}",
                listener,
                e,
                String::from_utf8_lossy(&payload)
            ),
        }
    }

    fn handle_user_event(&self, listener: Listener, mut payload: Vec<u8>) {
        if let Ok(s) = str::from_utf8(&payload) {
            let t = s.trim();
            if t.eq_ignore_ascii_case("PONG") {
                let ctx = self.strategy_context();
                for strategy in &self.strategies {
                    strategy.poly_handle_user_pong(Arc::clone(&ctx), listener);
                }
                return;
            }
        }

        match simd_json::from_slice::<OwnedValue>(&mut payload) {
            Ok(v) => match v {
                OwnedValue::Array(events) => {
                    for event in events.into_iter() {
                        self.dispatch_user_event(listener, event);
                    }
                }
                event => {
                    self.dispatch_user_event(listener, event);
                }
            },
            Err(e) => error!(
                "[{}] Error parsing user message: {}. Raw: {}",
                listener.as_str(),
                e,
                String::from_utf8_lossy(&payload)
            ),
        }
    }

    fn dispatch_market_message(&self, listener: Listener, wrapper: PolymarketMessageWrapper) {
        match wrapper.msg_type.as_str() {
            "agg_orderbook" => self.handle_agg_orderbook(listener, wrapper.payload),
            "price_change" => self.handle_price_change(listener, wrapper.payload),
            "tick_size_change" => self.handle_tick_size_change(listener, wrapper.payload),
            "pong" => {
                let ctx = self.strategy_context();
                for strategy in &self.strategies {
                    strategy.poly_handle_market_pong(Arc::clone(&ctx), listener);
                }
            }
            unknown_type => warn!(
                "[{}] Unhandled market message type '{}': {:?}",
                listener.as_str(),
                unknown_type,
                wrapper.payload
            ),
        }
    }

    fn dispatch_market_message_legacy(
        &self,
        listener: Listener,
        wrapper: PolymarketMessageWrapperOld,
    ) {
        match wrapper.event_type.as_str() {
            "price_change" => self.handle_price_change_legacy(listener, wrapper),
            "book" => self.handle_book_legacy(listener, wrapper),
            "tick_size_change" => self.handle_tick_size_change_legacy(listener, wrapper),
            "last_trade_price" => {}
            other => warn!(
                "[{}] Unhandled legacy market message type '{}': {:?}",
                listener.as_str(),
                other,
                wrapper
            ),
        }
    }

    fn dispatch_user_event(&self, listener: Listener, event: OwnedValue) {
        let event_type = event
            .get("event_type")
            .and_then(|v| v.as_str())
            .or_else(|| event.get("type").and_then(|v| v.as_str()))
            .map(|s| s.to_ascii_lowercase());
        match event_type.as_deref() {
            Some("trade") => {
                self.handle_trade(listener, event);
            }
            Some("order") => {
                self.handle_order(listener, event);
            }
            Some(other) => debug!("[{}] Unhandled user event type: {}", listener, other),
            None => debug!("[{}] User event missing type field", listener),
        }
    }

    // fn ensure_poly_orderbook(&self, snapshot: &AggOrderbook) {
    //     match self.poly_state.orderbooks.entry(snapshot.asset_id.clone()) {
    //         Entry::Occupied(_) => {}
    //         Entry::Vacant(entry) => {
    //             let orderbook = OrderBook::new(snapshot);
    //             entry.insert(Arc::new(RwLock::new(orderbook)));
    //         }
    //     }
    // }

    fn handle_agg_orderbook(&self, listener: Listener, payload: OwnedValue) {
        let ctx = self.strategy_context();
        if let Ok(snapshots) =
            simd_json::serde::from_owned_value::<Vec<AggOrderbook>>(payload.clone())
        {
            for snapshot in snapshots {
                // self.ensure_poly_orderbook(&snapshot);

                for strategy in &self.strategies {
                    strategy.poly_handle_market_agg_orderbook(
                        Arc::clone(&ctx),
                        listener,
                        &snapshot,
                    );
                }
            }
        } else if let Ok(snapshot) =
            simd_json::serde::from_owned_value::<AggOrderbook>(payload.clone())
        {
            // self.ensure_poly_orderbook(&snapshot);

            for strategy in &self.strategies {
                strategy.poly_handle_market_agg_orderbook(Arc::clone(&ctx), listener, &snapshot);
            }
        } else {
            warn!(
                "[PolyMarket] Failed to parse agg_orderbook payload. Raw: {}",
                payload
            );
        }
    }

    fn handle_price_change(&self, listener: Listener, payload: OwnedValue) {
        if let Ok(payload_data) =
            simd_json::serde::from_owned_value::<PriceChangePayload>(payload.clone())
        {
            self.process_price_change_payload(listener, payload_data);
        } else {
            warn!(
                "[PolyMarket] Failed to parse price_change payload. Raw: {}",
                payload
            );
        }
    }

    fn handle_price_change_legacy(&self, listener: Listener, wrapper: PolymarketMessageWrapperOld) {
        let PolymarketMessageWrapperOld {
            timestamp,
            price_changes,
            market,
            ..
        } = wrapper;

        // let payload_data = PriceChangePayload {
        //     pc: price_changes
        //         .into_iter()
        //         .map(|legacy_change| PriceChange {
        //             asset_id: legacy_change.asset_id,
        //             price: legacy_change.price,
        //             size: legacy_change.size,
        //             side: legacy_change.side,

        //         })
        //         .collect(),
        //     timestamp: timestamp.unwrap_or_default(),
        // };

        // if payload_data.pc.is_empty() {
        //     return;
        // }

        for change in &price_changes {
            let ctx = self.strategy_context();

            let pc = PriceChange {
                asset_id: change.asset_id.clone(),
                price: change.price.clone(),
                size: change.size.clone(),
                side: change.side.clone(),
            };
            for strategy in &self.strategies {
                strategy.poly_handle_market_price_change(Arc::clone(&ctx), listener, &pc);
            }
        }
    }

    fn process_price_change_payload(&self, listener: Listener, payload_data: PriceChangePayload) {
        let ctx = self.strategy_context();
        for change in &payload_data.pc {
            for strategy in &self.strategies {
                strategy.poly_handle_market_price_change(
                    Arc::clone(&ctx),
                    listener,
                    change,
                );
            }
        }
    }

    fn handle_book_legacy(&self, listener: Listener, wrapper: PolymarketMessageWrapperOld) {
        let PolymarketMessageWrapperOld {
            timestamp,
            market,
            asset_id,
            hash,
            bids,
            asks,
            ..
        } = wrapper;

        if bids.is_empty() && asks.is_empty() {
            return;
        }

        let asset_id = asset_id.unwrap();

        let snapshot = AggOrderbook {
            asset_id: asset_id.clone(),
            bids,
            asks,
            timestamp: timestamp.unwrap_or_default(),
            hash: hash.or_else(|| market).unwrap_or_default(),
        };

        let ctx = self.strategy_context();
        for strategy in &self.strategies {
            strategy.poly_handle_market_agg_orderbook(Arc::clone(&ctx), listener, &snapshot);
        }
    }

    fn handle_tick_size_change(&self, listener: Listener, payload: OwnedValue) {
        if let Ok(payload_data) =
            simd_json::serde::from_owned_value::<TickSizeChangePayload>(payload.clone())
        {
            if let Some(orderbook_entry) = self.poly_state.orderbooks.get(&payload_data.asset_id) {
                if let Ok(mut book) = orderbook_entry.write() {
                    book.set_tick_size(payload_data.new_tick_size.clone());
                }
            }

            let ctx = self.strategy_context();
            for strategy in &self.strategies {
                strategy.poly_handle_market_tick_size_change(
                    Arc::clone(&ctx),
                    listener,
                    &payload_data,
                );
            }
        } else {
            warn!(
                "[PolyMarket] Failed to parse tick_size_change payload. Raw: {}",
                payload
            );
        }
    }

    fn handle_tick_size_change_legacy(
        &self,
        listener: Listener,
        payload: PolymarketMessageWrapperOld,
    ) {
        let ticksize_pl = TickSizeChangePayload {
            asset_id: payload.asset_id.unwrap(),
            new_tick_size: payload.new_tick_size.unwrap(),
        };

        let ctx = self.strategy_context();
        for strategy in &self.strategies {
            strategy.poly_handle_market_tick_size_change(
                Arc::clone(&ctx),
                listener,
                &ticksize_pl,
            );
        }
    }

    fn handle_trade(&self, listener: Listener, payload: OwnedValue) {
        if let Ok(trade) = simd_json::serde::from_owned_value::<TradePayload>(payload.clone()) {
            let ctx = self.strategy_context();
            for strategy in &self.strategies {
                strategy.poly_handle_user_trade(Arc::clone(&ctx), listener, &trade);
            }
        } else {
            warn!(
                "[{}] Failed to parse trade payload. Raw: {}",
                listener.as_str(),
                payload
            );
        }
    }

    fn handle_order(&self, listener: Listener, payload: OwnedValue) {
        if let Ok(order) = simd_json::serde::from_owned_value::<OrderPayload>(payload.clone()) {
            let ctx = self.strategy_context();
            for strategy in &self.strategies {
                strategy.poly_handle_user_order(Arc::clone(&ctx), listener, &order);
            }
        } else {
            warn!("[PolyUser] Failed to parse order payload. Raw: {}", payload);
        }
    }

    // fn handle_rate_update(&self, _source: &'static str, kind: RateKind, value: f64) {
    //     match kind {
    //         RateKind::UsdcUsdtBinance => self.rates.set_usdc_usdt_binance(value),
    //         RateKind::UsdUsdtCoinbase => self.rates.set_usd_usdt_coinbase(value),
    //     }

    //     let ctx = self.strategy_context();
    //     for strategy in &self.strategies {
    //         strategy.conversion_handle_rate_update(&ctx, kind, value);
    //     }
    // }

    // fn handle_rate_clear(&self, kind: RateKind) {
    //     match kind {
    //         RateKind::UsdcUsdtBinance => self.rates.set_usdc_usdt_binance(0.0),
    //         RateKind::UsdUsdtCoinbase => self.rates.set_usd_usdt_coinbase(0.0),
    //     }

    //     let ctx = self.strategy_context();
    //     for strategy in &self.strategies {
    //         strategy.conversion_handle_rate_clear(&ctx, kind);
    //     }
    // }
}
