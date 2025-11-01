use crate::exchange_listeners::poly_models::{AggOrderbook, OrderSide, PriceChange};
use dashmap::DashMap;
use log::warn;
use std::cmp::{min, Reverse};
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// --- Constants for feasible price logic, as seen in the old implementation ---
const IGNORING_VOLUME: u32 = 5_000; // Corresponds to 5 shares
const SEARCH_DEPTH: u32 = 100; // Corresponds to 10 cents

// --- Heap Conversion and Best Price Logic (Ported from old implementation) ---

fn convert_to_bid_heap(bids: &DashMap<u32, u32>) -> BinaryHeap<(u32, u32)> {
    bids.iter()
        .map(|entry| (*entry.key(), *entry.value()))
        .collect()
}

fn convert_to_ask_heap(asks: &DashMap<u32, u32>) -> BinaryHeap<Reverse<(u32, u32)>> {
    asks.iter()
        .map(|entry| Reverse((*entry.key(), *entry.value())))
        .collect()
}

#[derive(Debug)]
pub struct OrderBook {
    pub asset_id: String,
    timestamp: String,
    // make these private to protect invariants
    asks: DashMap<u32, u32>,
    bids: DashMap<u32, u32>,
    tick_size: String,

    // heaps are derived state
    pub bid_heap: Mutex<BinaryHeap<(u32, u32)>>,
    pub ask_heap: Mutex<BinaryHeap<Reverse<(u32, u32)>>>,
    bids_dirty: AtomicBool,
    asks_dirty: AtomicBool,
}

impl OrderBook {
    pub fn new(snapshot: &AggOrderbook) -> Self {
        let bids = DashMap::new();
        for entry in &snapshot.bids {
            if let (Ok(price), Ok(size)) = (entry.price.parse::<f64>(), entry.size.parse::<f64>()) {
                if size > 0.0 {
                    bids.insert(
                        (price * 1000.0).round() as u32,
                        (size * 1000.0).round() as u32,
                    );
                }
            }
        }

        let asks = DashMap::new();
        for entry in &snapshot.asks {
            if let (Ok(price), Ok(size)) = (entry.price.parse::<f64>(), entry.size.parse::<f64>()) {
                if size > 0.0 {
                    asks.insert(
                        (price * 1000.0).round() as u32,
                        (size * 1000.0).round() as u32,
                    );
                }
            }
        }

        Self {
            asset_id: snapshot.asset_id.clone(),
            timestamp: snapshot.timestamp.clone(),
            tick_size: "0.01".to_string(),
            bid_heap: Mutex::new(convert_to_bid_heap(&bids)),
            ask_heap: Mutex::new(convert_to_ask_heap(&asks)),
            bids_dirty: AtomicBool::new(false),
            asks_dirty: AtomicBool::new(false),
            bids,
            asks,
        }
    }

    #[inline]
    fn rebuild_bid_heap_if_dirty(&self) {
        if self.bids_dirty.swap(false, Ordering::AcqRel) {
            let new_heap = convert_to_bid_heap(&self.bids);
            *self.bid_heap.lock().unwrap() = new_heap;
        }
    }

    #[inline]
    fn rebuild_ask_heap_if_dirty(&self) {
        if self.asks_dirty.swap(false, Ordering::AcqRel) {
            let new_heap = convert_to_ask_heap(&self.asks);
            *self.ask_heap.lock().unwrap() = new_heap;
        }
    }

    pub fn get_bid_map(&self) -> &DashMap<u32, u32> {
        &self.bids
    }

    pub fn get_ask_map(&self) -> &DashMap<u32, u32> {
        &self.asks
    }

    /// Applies updates from a `price_change` message.
    /// NOTE: still `&self` thanks to DashMap + Mutex on heaps.
    pub fn apply_price_change(&self, change: &PriceChange, _timestamp: &str) {
        // Parse inputs and scale to milli-units
        if let (Ok(price_f), Ok(size_f)) = (change.price.parse::<f64>(), change.size.parse::<f64>())
        {
            let price = (price_f * 1000.0).round() as u32;
            let size = (size_f * 1000.0).round() as u32;

            // Determine side to update
            let is_bid = change.side.eq_ignore_ascii_case(OrderSide::Buy.as_str());
            let side = if is_bid {
                &self.bids
            } else if change.side.eq_ignore_ascii_case(OrderSide::Sell.as_str()) {
                &self.asks
            } else {
                warn!(
                    "[PolyOrderbook] Unknown side '{}' in price_change: p={}, s={}",
                    change.side, change.price, change.size
                );
                return;
            };

            // 1) Apply the upsert/delete to the authoritative map for this side
            if size == 0 {
                side.remove(&price);
            } else {
                side.insert(price, size);
            }

            // Mark only the updated side as dirty (its heap needs to reflect this change)
            if is_bid {
                self.bids_dirty.store(true, Ordering::Release);
            } else {
                self.asks_dirty.store(true, Ordering::Release);
            }

            // 2) Cross the book if the incoming price crosses the opposite top-of-book.
            if is_bid {
                // Mutate the ASK heap + map:
                // Lock heap once; rebuild once if dirty; then pop while ask_price <= bid_price.
                let mut ask_heap = self.ask_heap.lock().unwrap();
                if self.asks_dirty.swap(false, Ordering::AcqRel) {
                    *ask_heap = convert_to_ask_heap(&self.asks);
                }
                loop {
                    match ask_heap.peek().copied() {
                        Some(std::cmp::Reverse((ask_p, _ask_sz))) if ask_p <= price => {
                            // Pop from heap to keep it in sync …
                            let std::cmp::Reverse((popped_p, _)) = ask_heap.pop().unwrap();
                            // … and remove the corresponding price level from the map.
                            self.asks.remove(&popped_p);
                            // No dirty flag flip needed; heap already reflects the pop.
                        }
                        _ => break, // empty or no longer crossing
                    }
                }
            } else {
                // Mutate the BID heap + map:
                let mut bid_heap = self.bid_heap.lock().unwrap();
                if self.bids_dirty.swap(false, Ordering::AcqRel) {
                    *bid_heap = convert_to_bid_heap(&self.bids);
                }
                loop {
                    match bid_heap.peek().copied() {
                        Some((bid_p, _bid_sz)) if bid_p >= price => {
                            let (popped_p, _) = bid_heap.pop().unwrap();
                            self.bids.remove(&popped_p);
                        }
                        _ => break,
                    }
                }
            }
        } else {
            warn!(
                "[PolyOrderbook] Failed to parse price/size in price_change: p={}, s={}",
                change.price, change.size
            );
        }
    }

    pub fn set_tick_size(&mut self, new_tick_size: String) {
        self.tick_size = new_tick_size;
    }

    pub fn get_tick_size(&self) -> &str {
        &self.tick_size
    }

    pub fn best_bid(&self) -> Option<(u32, u32)> {
        self.rebuild_bid_heap_if_dirty();
        self.bid_heap.lock().unwrap().peek().copied()
    }

    pub fn best_ask(&self) -> Option<(u32, u32)> {
        self.rebuild_ask_heap_if_dirty();
        self.ask_heap.lock().unwrap().peek().map(|r| r.0)
    }

    pub fn get_midpoint(&self) -> u32 {
        let best_bid = self.best_bid();
        let best_ask = self.best_ask();
        if let (Some((bid_price, _)), Some((ask_price, _))) = (best_bid, best_ask) {
            (bid_price + ask_price) / 2
        } else {
            0
        }
    }

    pub fn get_spread(&self) -> u32 {
        let best_bid = self.best_bid();
        let best_ask = self.best_ask();
        if let (Some((bid_price, _)), Some((ask_price, _))) = (best_bid, best_ask) {
            ask_price - bid_price
        } else {
            0
        }
    }

    pub fn best_feasible_bid(&self) -> Option<(u32, u32)> {
        self.rebuild_bid_heap_if_dirty();
        // work on a clone so we don't mutate the stored heap
        let mut heap = self.bid_heap.lock().unwrap().clone();
        let mut cumulative_size = 0;
        let start_bid = heap.peek()?.0;
        let price_boundary = start_bid.saturating_sub(SEARCH_DEPTH);

        while let Some((price, size)) = heap.pop() {
            if price < price_boundary {
                return None;
            }
            cumulative_size += size;
            if cumulative_size >= IGNORING_VOLUME {
                return Some((price, size));
            }
        }
        None
    }

    pub fn best_feasible_ask(&self) -> Option<(u32, u32)> {
        self.rebuild_ask_heap_if_dirty();
        let mut heap = self.ask_heap.lock().unwrap().clone();
        let mut cumulative_size = 0;
        let start_ask = heap.peek()?.0 .0;
        let price_boundary = min(start_ask.saturating_add(SEARCH_DEPTH), 999);

        while let Some(Reverse((price, size))) = heap.pop() {
            if price > price_boundary {
                return None;
            }
            cumulative_size += size;
            if cumulative_size >= IGNORING_VOLUME {
                return Some((price, size));
            }
        }
        None
    }

    // (Optional) helper if you want to mutate directly:
    pub fn upsert_bid(&self, price: u32, size: u32) {
        if size == 0 {
            self.bids.remove(&price);
        } else {
            self.bids.insert(price, size);
        }
        self.bids_dirty.store(true, Ordering::Release);
    }
    pub fn upsert_ask(&self, price: u32, size: u32) {
        if size == 0 {
            self.asks.remove(&price);
        } else {
            self.asks.insert(price, size);
        }
        self.asks_dirty.store(true, Ordering::Release);
    }
}
