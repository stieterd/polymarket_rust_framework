use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OrderbookDepth {
    L1,
    L2,
}

#[derive(Debug, Clone, Copy)]
pub struct OrderbookLevel {
    pub price: f64,
    pub size: f64,
}

impl OrderbookLevel {
    pub fn new(price: f64, size: f64) -> Self {
        Self { price, size }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VwapState {
    pub dominant_side_is_bid: bool,
    pub search_volume: f64, // The target volume (size of the dominant order)
    pub total_value: f64,   // Sum of (price * size) for the search side
    pub total_volume: f64,  // Sum of size for the search side
    pub cutoff_price_key: u64, // The price key of the last level included
    pub cutoff_volume_used: f64, // The amount of volume used from the cutoff level
}

#[derive(Debug, Clone)]
pub struct CryptoOrderbook {
    pub depth: OrderbookDepth,
    pub bids: BTreeMap<u64, f64>,
    pub asks: BTreeMap<u64, f64>,
    pub vwap_cache: Option<VwapState>,
}

impl Default for CryptoOrderbook {
    fn default() -> Self {
        Self::new(OrderbookDepth::L1)
    }
}

impl CryptoOrderbook {
    pub fn new(depth: OrderbookDepth) -> Self {
        Self {
            depth,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            vwap_cache: None,
        }
    }

    pub fn invalidate_vwap_cache(&mut self) {
        self.vwap_cache = None;
    }

    pub fn set_depth(&mut self, depth: OrderbookDepth) {
        self.depth = depth;
    }

    pub fn get_midpoint(&self) -> f64 {
        (self.best_bid().unwrap().price + self.best_ask().unwrap().price) / 2.0
    }

    pub fn clear(&mut self) {
        self.bids.clear();
        self.asks.clear();
    }

    pub fn update_l1(
        &mut self,
        best_bid: Option<OrderbookLevel>,

        best_ask: Option<OrderbookLevel>,
    ) {
        self.depth = OrderbookDepth::L1;
        self.clear();

        if let Some(level) = best_bid {
            self.bids.insert(Self::price_key(level.price), level.size);
        }
        if let Some(level) = best_ask {
            self.asks.insert(Self::price_key(level.price), level.size);
        }
    }

    pub fn apply_l2_snapshot(&mut self, is_bid: bool, levels: &[OrderbookLevel]) {
        self.depth = OrderbookDepth::L2;
        self.invalidate_vwap_cache();
        let side = self.side_mut(is_bid);
        side.clear();
        for level in levels {
            if level.size > 0.0 {
                side.insert(Self::price_key(level.price), level.size);
            }
        }
    }

    pub fn apply_l2_updates(&mut self, is_bid: bool, levels: &[OrderbookLevel]) {
        self.depth = OrderbookDepth::L2;
        let side = self.side_mut(is_bid);
        for level in levels {
            let key = Self::price_key(level.price);
            if level.size <= 0.0 {
                side.remove(&key);
            } else {
                side.insert(key, level.size);
            }
        }
    }

    pub fn best_bid(&self) -> Option<OrderbookLevel> {
        self.bids
            .iter()
            .next_back()
            .map(|(price, size)| OrderbookLevel::new(Self::price_from_key(*price), *size))
    }

    pub fn best_ask(&self) -> Option<OrderbookLevel> {
        self.asks
            .iter()
            .next()
            .map(|(price, size)| OrderbookLevel::new(Self::price_from_key(*price), *size))
    }

    pub fn depth(&self) -> OrderbookDepth {
        self.depth
    }

    fn side_mut(&mut self, is_bid: bool) -> &mut BTreeMap<u64, f64> {
        if is_bid {
            &mut self.bids
        } else {
            &mut self.asks
        }
    }

    pub fn price_key(price: f64) -> u64 {
        (price * 1_000_000.0).round() as u64
    }

    pub fn price_from_key(key: u64) -> f64 {
        key as f64 / 1_000_000.0
    }
}
