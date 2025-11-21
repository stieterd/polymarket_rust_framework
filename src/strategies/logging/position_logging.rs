use log::{info, warn};
use std::{collections::HashSet, sync::Arc};

use crate::{
    credentials::ADDRESS_STR,
    exchange_listeners::{
        crypto_models::CryptoPriceUpdate,
        orderbooks::{poly_orderbook::OrderBook, CryptoOrderbook, OrderbookDepth},
        poly_models::{LegacyPriceChange, Listener, PriceChange, TradeRole, TradeStatus},
        Crypto, Exchange, Instrument,
    },
    strategies::{Strategy, StrategyContext},
};

pub struct PositionLoggingStrategy;

impl PositionLoggingStrategy {
    pub fn new() -> Self {
        Self
    }

    fn log_asset_position(&self, ctx: &StrategyContext, asset_id: &str) {
        let Some(position_entry) = ctx.poly_state.positions.get(asset_id) else {
            info!(
                "[{}] Asset {} new position: 0.000",
                self.name(),
                asset_id
            );
            return;
        };

        let position_arc = Arc::clone(position_entry.value());
        drop(position_entry);

        match position_arc.read() {
            Ok(position) => {
                let display_size = position.size as f64 / 1000.0;
                info!(
                    "[{}] Asset {} new position: {:.3}",
                    self.name(),
                    asset_id,
                    display_size
                );
            }
            Err(_) => warn!(
                "[{}] Failed to read position lock for asset {}",
                self.name(),
                asset_id
            ),
        };
    }
}

impl Strategy for PositionLoggingStrategy {
    fn name(&self) -> &'static str {
        "PositionLogger"
    }

    fn poly_handle_user_trade(
        &self,
        ctx: Arc<StrategyContext>,
        _listener: Listener,
        _payload: &crate::exchange_listeners::poly_models::TradePayload,
    ) {
        if _payload.status != TradeStatus::Matched {
            return;
        }
        
        match _payload.trade_role {
            TradeRole::Taker => {
                self.log_asset_position(ctx.as_ref(), &_payload.asset_id);
            }
            TradeRole::Maker => {
                let mut affected_assets: HashSet<String> = HashSet::new();
                for maker_order in &_payload.maker_orders {
                    if maker_order
                        .maker_address
                        .eq_ignore_ascii_case(ADDRESS_STR)
                    {
                        affected_assets.insert(maker_order.asset_id.clone());
                    }
                }

                for asset_id in affected_assets {
                    self.log_asset_position(ctx.as_ref(), &asset_id);
                }
            }
            TradeRole::Unknown => {}
        }
    }
}
