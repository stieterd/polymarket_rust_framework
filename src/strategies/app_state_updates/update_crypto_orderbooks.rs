use crate::{
    exchange_listeners::{
        crypto_models::{
            get_crypto_orderbook_map, get_crypto_prices_map, CryptoPrice, CryptoPriceUpdate,
        },
        orderbooks::{
            crypto_orderbook::{CryptoOrderbook, VwapState},
            OrderbookDepth, OrderbookLevel,
        },
        Exchange,
    },
    strategies::Strategy,
};
use async_trait::async_trait;

pub struct UpdateCryptoOrderbookStrategy;

impl UpdateCryptoOrderbookStrategy {
    pub fn new() -> Self {
        Self
    }

    fn calculate_full_vwap_and_state<'a>(book: &'a CryptoOrderbook) -> Option<(f64, VwapState)> {
        let best_bid = book.best_bid()?;
        let best_ask = book.best_ask()?;

        let (dominant_price, search_volume, dominant_side_is_bid, search_side_iter): (
            f64,
            f64,
            bool,
            Box<dyn Iterator<Item = (&'a u64, &'a f64)>>,
        ) = if best_bid.size > best_ask.size {
            (
                best_bid.price,
                best_bid.size,
                true,
                Box::new(book.asks.iter()),
            )
        } else {
            (
                best_ask.price,
                best_ask.size,
                false,
                Box::new(book.bids.iter().rev()),
            )
        };

        if search_volume <= 0.0 {
            return None;
        }

        let mut total_value = 0.0;
        let mut total_volume = 0.0;
        let mut cutoff_price_key = 0;
        let mut cutoff_volume_used = 0.0;

        for (price_key, size) in search_side_iter {
            if total_volume >= search_volume {
                break;
            }
            let price = CryptoOrderbook::price_from_key(*price_key);
            let volume_needed = search_volume - total_volume;
            let volume_to_use = size.min(volume_needed);

            total_value += price * volume_to_use;
            total_volume += volume_to_use;

            cutoff_price_key = *price_key;
            cutoff_volume_used = volume_to_use;
        }

        if total_volume > 0.0 && (total_volume / search_volume) > 0.999 {
            let vwap_of_search_side = total_value / total_volume;
            let final_price = (dominant_price + vwap_of_search_side) / 2.0;

            let state = VwapState {
                dominant_side_is_bid,
                search_volume,
                total_value,
                total_volume,
                cutoff_price_key,
                cutoff_volume_used,
            };

            Some((final_price, state))
        } else {
            None
        }
    }
}

#[async_trait]
impl Strategy for UpdateCryptoOrderbookStrategy {
    fn name(&self) -> &'static str {
        "UpdateCryptoOrderbooks"
    }

    async fn crypto_handle_price_update(
        &self,
        _ctx: &crate::strategies::StrategyContext,
        _exchange: crate::exchange_listeners::Exchange,
        _instrument: crate::exchange_listeners::Instrument,
        _crypto: crate::exchange_listeners::Crypto,
        _depth: OrderbookDepth,
        _price_update: &CryptoPriceUpdate,
    ) {
        let orderbook_map = get_crypto_orderbook_map(_ctx.app_state.clone(), _crypto);
        let prices_map = get_crypto_prices_map(_ctx.app_state.clone(), _crypto);
        let orderbook_key = (_exchange, _instrument, _depth);

        if let OrderbookDepth::L1 = _depth {
            let bid_level = OrderbookLevel {
                price: _price_update.best_bid_price,
                size: _price_update.best_bid_vol,
            };
            let ask_level = OrderbookLevel {
                price: _price_update.best_ask_price,
                size: _price_update.best_ask_vol,
            };

            let mut orderbook = orderbook_map
                .entry(orderbook_key)
                .or_insert_with(|| CryptoOrderbook::new(_depth));

            orderbook.update_l1(Some(bid_level), Some(ask_level));

            let price_key = (_exchange, _instrument, _depth);
            let mut price = prices_map.entry(price_key).or_insert_with(CryptoPrice::new);
            price.midpoint = orderbook.get_midpoint();
            price.price = price.midpoint;
        }
    }

    async fn crypto_handle_l2_snapshot(
        &self,
        ctx: &crate::strategies::StrategyContext,
        exchange: crate::exchange_listeners::Exchange,
        instrument: crate::exchange_listeners::Instrument,
        crypto: crate::exchange_listeners::Crypto,
        bids: &[OrderbookLevel],
        asks: &[OrderbookLevel],
    ) {
        let orderbook_map = get_crypto_orderbook_map(ctx.app_state.clone(), crypto);
        let orderbook_key = (exchange, instrument, OrderbookDepth::L2);

        let price_data: Option<(f64, f64)> = {
            let mut book = orderbook_map
                .entry(orderbook_key)
                .or_insert_with(|| CryptoOrderbook::new(OrderbookDepth::L2));

            book.apply_l2_snapshot(true, bids);
            book.apply_l2_snapshot(false, asks);

            match exchange {
                Exchange::Deribit => {
                    if let Some((vwap, state)) = Self::calculate_full_vwap_and_state(&book) {
                        book.vwap_cache = Some(state);
                        Some((book.get_midpoint(), vwap))
                    } else {
                        book.invalidate_vwap_cache();
                        let midpoint = book.get_midpoint();
                        Some((midpoint, midpoint))
                    }
                }
                Exchange::Kraken => {
                    let midpoint = book.get_midpoint();
                    Some((midpoint, midpoint))
                }
                _ => None,
            }
        };

        if let Some((midpoint, final_price)) = price_data {
            let prices_map = get_crypto_prices_map(ctx.app_state.clone(), crypto);
            let price_key = (exchange, instrument, OrderbookDepth::L2);
            let mut price = prices_map.entry(price_key).or_insert_with(CryptoPrice::new);

            price.midpoint = midpoint;
            price.price = final_price;
        }
    }

    async fn crypto_handle_l2_update(
        &self,
        ctx: &crate::strategies::StrategyContext,
        exchange: crate::exchange_listeners::Exchange,
        instrument: crate::exchange_listeners::Instrument,
        crypto: crate::exchange_listeners::Crypto,
        bids: &[OrderbookLevel],
        asks: &[OrderbookLevel],
    ) {
        let orderbook_map = get_crypto_orderbook_map(ctx.app_state.clone(), crypto);
        let orderbook_key = (exchange, instrument, OrderbookDepth::L2);

        let mut book = match orderbook_map.get_mut(&orderbook_key) {
            Some(book) => book,
            None => return, // Book doesn't exist yet, so exit the function.
        };

        let price_data: Option<(f64, f64)> = match exchange {
            Exchange::Deribit => {
                let old_best_bid = book.best_bid();
                let old_best_ask = book.best_ask();
                let old_cache = book.vwap_cache;

                book.apply_l2_updates(true, bids);
                book.apply_l2_updates(false, asks);

                let new_best_bid = book.best_bid();
                let new_best_ask = book.best_ask();

                // Check for a valid book state before proceeding.
                if new_best_bid.is_none() || new_best_ask.is_none() {
                    book.invalidate_vwap_cache();
                    None // FIX 2: This now correctly evaluates to None for the price_data variable.
                } else {
                    let midpoint = book.get_midpoint();

                    let requires_full_recalc = old_cache.is_none()
                        || old_best_bid.is_none()
                        || old_best_ask.is_none()
                        || (old_best_bid.unwrap().size > old_best_ask.unwrap().size)
                            != (new_best_bid.unwrap().size > new_best_ask.unwrap().size)
                        || (old_cache.unwrap().dominant_side_is_bid
                            && old_best_bid.unwrap().price != new_best_bid.unwrap().price)
                        || (!old_cache.unwrap().dominant_side_is_bid
                            && old_best_ask.unwrap().price != new_best_ask.unwrap().price);

                    if requires_full_recalc {
                        if let Some((vwap, state)) = Self::calculate_full_vwap_and_state(&book) {
                            book.vwap_cache = Some(state);
                            Some((midpoint, vwap))
                        } else {
                            book.invalidate_vwap_cache();
                            Some((midpoint, midpoint))
                        }
                    } else {
                        // Fallback to full calculation for simplicity.
                        if let Some((vwap, state)) = Self::calculate_full_vwap_and_state(&book) {
                            book.vwap_cache = Some(state);
                            Some((midpoint, vwap))
                        } else {
                            book.invalidate_vwap_cache();
                            Some((midpoint, midpoint))
                        }
                    }
                }
            }
            Exchange::Kraken => {
                book.apply_l2_updates(true, bids);
                book.apply_l2_updates(false, asks);
                let midpoint = book.get_midpoint();
                Some((midpoint, midpoint))
            }
            _ => None,
        };

        if let Some((midpoint, final_price)) = price_data {
            let prices_map = get_crypto_prices_map(ctx.app_state.clone(), crypto);
            let price_key = (exchange, instrument, OrderbookDepth::L2);
            let mut price = prices_map.entry(price_key).or_insert_with(CryptoPrice::new);

            price.midpoint = midpoint;
            price.price = final_price;
        }
    }
}
