use crate::exchange_listeners::crypto_models::*;
use crate::exchange_listeners::event_processor::{CountingSender, SocketEvent};
use crate::exchange_listeners::orderbooks::{OrderbookDepth, OrderbookLevel};
use anyhow::Result;
use bstr::BString;
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use ordered_float::OrderedFloat;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// A generic WebSocket handler for simple subscribe-and-listen connections.
async fn websocket_handler<F>(
    name: &'static str,
    url: &'static str,
    subscribe_msg: Option<String>,
    exchange: Exchange,
    instrument: Instrument,
    crypto: Crypto,
    event_tx: Arc<CountingSender>,
    mut on_message: F,
) where
    F: FnMut(&[u8]) -> Result<Option<CryptoPriceUpdate>>,
{
    let clear_event = SocketEvent::ClearPrice {
        exchange,
        instrument,
        crypto,
    };

    loop {
        if let Ok((ws_stream, _)) = connect_async(url).await {
            let (mut write, mut read) = ws_stream.split();

            if let Some(msg) = &subscribe_msg {
                if let Err(e) = write.send(Message::Text(msg.clone())).await {
                    error!(
                        "[{}] Failed to send subscribe message: {}. Retrying...",
                        name, e
                    );
                    let _ = event_tx.send(clear_event.clone());
                    time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
            }

            let mut ping_interval = time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    Some(msg_result) = read.next() => {
                        let msg = match msg_result {
                            Ok(m) => m,
                            Err(e) => {
                                error!("[{}] WebSocket stream error: {}. Reconnecting...", name, e);
                                break;
                            }
                        };

                        match msg {
                            Message::Text(text) => {
                                match on_message(text.as_bytes()) {
                                    Ok(Some(price_update)) => {
                                        if event_tx
                                            .send(SocketEvent::Price {
                                                exchange,
                                                instrument,
                                                crypto,
                                                depth: OrderbookDepth::L1,
                                                price_update,
                                            })
                                            .is_err()
                                        {
                                            error!("[{}] Failed to forward price event. Stopping listener loop.", name);
                                            return;
                                        }
                                    }
                                    Ok(None) => { /* Message did not produce a price update */ }
                                    Err(_) => { /* Harmless processing error, e.g., on confirmation msg */ }
                                }
                            }
                            Message::Ping(payload) => {
                                if write.send(Message::Pong(payload)).await.is_err() {
                                    break;
                                }
                            }
                            Message::Close(_) => {
                                warn!("[{}] Connection closed by server.", name);
                                break;
                            }
                            _ => {}
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(e) = write.send(Message::Ping(vec![])).await {
                            error!("[{}] Failed to send proactive ping: {}", name, e);
                            break;
                        }
                    }
                }
            }
        } else {
            error!("[{}] Connection failed", name);
        }

        let _ = event_tx.send(clear_event.clone());
        warn!(
            "[{}] Listener DOWN. Clearing price and reconnecting in 5s...",
            name
        );
        time::sleep(Duration::from_secs(5)).await;
    }
}

// --- Specific Listener Implementations ---
// Price listeners use the generic handler where possible.
pub async fn binance_listener(crypto: Crypto, is_perp: bool, event_tx: Arc<CountingSender>) {
    let (name_prefix, base_url, instrument) = if is_perp {
        (
            "Perp",
            "wss://fstream.binance.com/ws/",
            Instrument::Perpetual,
        )
    } else {
        (
            "Spot",
            "wss://stream.binance.com:9443/ws/",
            Instrument::Spot,
        )
    };
    let symbol = format!("{}usdt", crypto.to_string().to_lowercase());
    let url = format!("{}{}@bookTicker", base_url, symbol);
    let name = format!("Binance_{}_{}", crypto, name_prefix);

    websocket_handler(
        Box::leak(name.into_boxed_str()),
        Box::leak(url.into_boxed_str()),
        None,
        Exchange::Binance,
        instrument,
        crypto,
        event_tx,
        move |bytes| {
            let mut vec = bytes.to_vec();
            let ticker: BinanceBookTicker = simd_json::from_slice(&mut vec)?;
            let best_bid_price = ticker.best_bid.parse::<f64>().ok();
            let best_ask_price = ticker.best_ask.parse::<f64>().ok();
            let best_bid_vol = ticker.best_bid_qty.parse::<f64>().ok();
            let best_ask_vol = ticker.best_ask_qty.parse::<f64>().ok();
            if let (
                Some(best_bid_price),
                Some(best_ask_price),
                Some(best_bid_vol),
                Some(best_ask_vol),
            ) = (best_bid_price, best_ask_price, best_bid_vol, best_ask_vol)
            {
                Ok(Some(CryptoPriceUpdate {
                    symbol: Some(ticker.symbol),
                    best_bid_price,
                    best_bid_vol,
                    best_ask_price,
                    best_ask_vol,
                }))
            } else {
                Ok(None)
            }
        },
    )
    .await;
}

pub async fn coinbase_legacy_listener(crypto: Crypto, event_tx: Arc<CountingSender>) {
    let product_id = format!("{}-USD", crypto.to_string());
    let url = "wss://ws-feed.exchange.coinbase.com";
    // MODIFIED: Added "heartbeat" to the channels array.
    let subscribe_msg = json!({
        "type": "subscribe",
        "product_ids": [product_id.clone()],
        "channels": ["ticker", "heartbeat"]
    })
    .to_string();
    let name = format!("Coinbase_Legacy_{}_Spot", crypto);

    websocket_handler(
        Box::leak(name.into_boxed_str()),
        url,
        Some(subscribe_msg),
        Exchange::CoinbaseLegacy,
        Instrument::Spot,
        crypto,
        event_tx,
        move |bytes| {
            let mut vec = bytes.to_vec();
            let msg: CoinbaseLegacyMsg = simd_json::from_slice(&mut vec)?;
            // MODIFIED: Changed from `if let` to `match` to handle heartbeats.
            match msg {
                CoinbaseLegacyMsg::Ticker(ticker) => {
                    if let Some(price_str) = ticker.price {
                        let price = price_str.parse::<f64>()?;
                        // if let Some(rate) = rates.get_usd_usdt_coinbase() {
                        //     let adjusted = price / rate;
                        //     return Ok(Some(CryptoPriceUpdate {
                        //         symbol: Some(product_id.clone()),
                        //         best_bid_price: adjusted,
                        //         best_bid_vol: 0.0,
                        //         best_ask_price: adjusted,
                        //         best_ask_vol: 0.0,
                        //     }));
                        // }
                    }
                    Err(anyhow::anyhow!("Could not process ticker"))
                }
                CoinbaseLegacyMsg::Heartbeat(_) => {
                    // Acknowledged heartbeat. This is not a price update, so we return Err to signal the handler to continue listening.
                    Err(anyhow::anyhow!("Heartbeat received"))
                }
                CoinbaseLegacyMsg::Other => Err(anyhow::anyhow!("Other message type received")),
            }
        },
    )
    .await;
}

pub async fn bitstamp_listener(crypto: Crypto, event_tx: Arc<CountingSender>) {
    let symbol = crypto.to_string().to_lowercase();
    let url = "wss://ws.bitstamp.net";
    let subscribe_msg = json!({ "event": "bts:subscribe", "data": {"channel": format!("order_book_{}usd", symbol)} }).to_string();
    let name = format!("Bitstamp_{}_Spot", crypto);

    websocket_handler(
        Box::leak(name.into_boxed_str()),
        url,
        Some(subscribe_msg),
        Exchange::Bitstamp,
        Instrument::Spot,
        crypto,
        event_tx,
        move |bytes| {
            let mut vec = bytes.to_vec();
            let msg: BitstampMsg = simd_json::from_slice(&mut vec)?;
            if let BitstampMsg::Data(book) = msg {
                if let (Some(best_bid), Some(best_ask)) =
                    (book.data.bids.first(), book.data.asks.first())
                {
                    let bid_price = best_bid[0].parse::<f64>()?;
                    let ask_price = best_ask[0].parse::<f64>()?;
                    let bid_vol = best_bid[1].parse::<f64>().unwrap_or(0.0);
                    let ask_vol = best_ask[1].parse::<f64>().unwrap_or(0.0);
                    // if let Some(rate) = rates.get_usd_usdt_coinbase() {
                    //     let symbol = format!("{}USD", crypto);
                    //     return Ok(Some(CryptoPriceUpdate {
                    //         symbol: Some(symbol),
                    //         best_bid_price: bid_price / rate,
                    //         best_bid_vol: bid_vol,
                    //         best_ask_price: ask_price / rate,
                    //         best_ask_vol: ask_vol,
                    //     }));
                    // }
                }
            }
            Err(anyhow::anyhow!("Not a data message"))
        },
    )
    .await;
}

pub async fn bybit_listener(crypto: Crypto, is_perp: bool, event_tx: Arc<CountingSender>) {
    let (name_prefix, url, instrument) = if is_perp {
        (
            "Perp",
            "wss://stream.bybit.com/v5/public/linear",
            Instrument::Perpetual,
        )
    } else {
        (
            "Spot",
            "wss://stream.bybit.com/v5/public/spot",
            Instrument::Spot,
        )
    };
    let symbol = format!("{}USDT", crypto);
    let subscribe_msg =
        json!({"op": "subscribe", "args": [format!("orderbook.1.{}", symbol)]}).to_string();
    let name = format!("Bybit_{}_{}", crypto, name_prefix);

    websocket_handler(
        Box::leak(name.into_boxed_str()),
        url,
        Some(subscribe_msg),
        Exchange::Bybit,
        instrument,
        crypto,
        event_tx,
        move |bytes| {
            let mut vec = bytes.to_vec();
            let msg: BybitMsg = simd_json::from_slice(&mut vec)?;
            if let BybitMsg::Orderbook(book) = msg {
                if let (Some(best_bid), Some(best_ask)) =
                    (book.data.bids.first(), book.data.asks.first())
                {
                    let bid_price = best_bid[0].parse::<f64>()?;
                    let ask_price = best_ask[0].parse::<f64>()?;
                    let bid_vol = best_bid[1].parse::<f64>().unwrap_or(0.0);
                    let ask_vol = best_ask[1].parse::<f64>().unwrap_or(0.0);
                    return Ok(Some(CryptoPriceUpdate {
                        symbol: Some(symbol.clone()),
                        best_bid_price: bid_price,
                        best_bid_vol: bid_vol,
                        best_ask_price: ask_price,
                        best_ask_vol: ask_vol,
                    }));
                }
            }
            Err(anyhow::anyhow!("Not an orderbook message"))
        },
    )
    .await;
}

pub async fn bitmex_listener(crypto: Crypto, event_tx: Arc<CountingSender>) {
    let symbol = if crypto == Crypto::BTC {
        "XBTUSD".to_string()
    } else {
        format!("{}USD", crypto)
    };
    let url = "wss://ws.bitmex.com/realtime";
    let subscribe_msg =
        json!({"op": "subscribe", "args": [format!("quote:{}", symbol)]}).to_string();
    let name = format!("BitMEX_{}_Perp", crypto);

    websocket_handler(
        Box::leak(name.into_boxed_str()),
        url,
        Some(subscribe_msg),
        Exchange::Bitmex,
        Instrument::Perpetual,
        crypto,
        event_tx,
        move |bytes| {
            let mut vec = bytes.to_vec();
            let msg: BitmexMsg = simd_json::from_slice(&mut vec)?;
            if let BitmexMsg::Data(msg_data) = msg {
                // if let Some(quote) = msg_data.data.first() {
                //     if let Some(rate) = rates.get_usd_usdt_coinbase() {
                //         return Ok(Some(CryptoPriceUpdate {
                //             symbol: Some(symbol.clone()),
                //             best_bid_price: quote.bid_price / rate,
                //             best_bid_vol: 0.0,
                //             best_ask_price: quote.ask_price / rate,
                //             best_ask_vol: 0.0,
                //         }));
                //     }
                // }
            }
            Err(anyhow::anyhow!("Not a data message"))
        },
    )
    .await;
}

// Listeners with custom loops for unique handshakes/heartbeats.
pub async fn deribit_listener(crypto: Crypto, is_perp: bool, event_tx: Arc<CountingSender>) {
    let (name_prefix, channel_str, instrument) = if is_perp {
        (
            "Perp",
            format!("book.{}-PERPETUAL.raw", crypto),
            Instrument::Perpetual,
        )
    } else {
        return;
    };
    let name = format!("Deribit_{}_{}", crypto, name_prefix);
    let url = "wss://www.deribit.com/ws/api/v2";
    let clear_event = SocketEvent::ClearPrice {
        exchange: Exchange::Deribit,
        instrument,
        crypto,
    };

    let auth_msg = json!({ "jsonrpc": "2.0", "id": 9929, "method": "public/auth", "params": {"grant_type": "client_credentials", "client_id": "sXew6aEt", "client_secret": "QnchuQTfGfm53uES9JVSYpzQan7M8QvGOapJid0zRFE"} }).to_string();
    let subscribe_msg = json!({ "jsonrpc": "2.0", "id": 42, "method": "public/subscribe", "params": {"channels": [channel_str]} }).to_string();
    let set_heartbeat_msg = json!({ "jsonrpc": "2.0", "id": 9098, "method": "public/set_heartbeat", "params": {"interval": 30} }).to_string();
    let test_response_msg =
        json!({ "jsonrpc": "2.0", "id": 8008, "method": "public/test", "params": {} }).to_string();

    loop {
        if let Ok((ws_stream, _)) = connect_async(url).await {
            let (mut write, mut read) = ws_stream.split();
            if write.send(Message::Text(auth_msg.clone())).await.is_err()
                || write
                    .send(Message::Text(subscribe_msg.clone()))
                    .await
                    .is_err()
                || write
                    .send(Message::Text(set_heartbeat_msg.clone()))
                    .await
                    .is_err()
            {
                warn!("[{}] Failed to send initial messages. Retrying...", name);
                let _ = event_tx.send(clear_event.clone());
                time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            'message_loop: while let Some(Ok(msg)) = read.next().await {
                if !msg.is_text() {
                    continue;
                }
                let mut text = msg.into_text().unwrap();

                match unsafe { simd_json::from_str::<DeribitMsg>(&mut text) } {
                    Ok(DeribitMsg::Heartbeat(hb)) if hb.params.heartbeat_type == "test_request" => {
                        if write
                            .send(Message::Text(test_response_msg.clone()))
                            .await
                            .is_err()
                        {
                            break 'message_loop;
                        }
                    }
                    Ok(DeribitMsg::Subscription(sub)) => {
                        let data = sub.params.data;

                        // The .raw stream's first message has a null prev_change_id. This is how we find the snapshot.
                        let is_snapshot = data.prev_change_id.is_none();

                        let bids: Vec<OrderbookLevel> = data
                            .bids
                            .into_iter()
                            .map(|(action, price, size)| {
                                let final_size = if action == "delete" { 0.0 } else { size };
                                OrderbookLevel::new(price, final_size)
                            })
                            .collect();

                        let asks: Vec<OrderbookLevel> = data
                            .asks
                            .into_iter()
                            .map(|(action, price, size)| {
                                let final_size = if action == "delete" { 0.0 } else { size };
                                OrderbookLevel::new(price, final_size)
                            })
                            .collect();

                        let event = if is_snapshot {
                            SocketEvent::L2Snapshot {
                                exchange: Exchange::Deribit,
                                instrument,
                                crypto,
                                bids,
                                asks,
                            }
                        } else {
                            SocketEvent::L2Update {
                                exchange: Exchange::Deribit,
                                instrument,
                                crypto,
                                bids,
                                asks,
                            }
                        };

                        if event_tx.send(event).is_err() {
                            error!(
                                "[{}] Failed to forward L2 event. Processor channel closed.",
                                name
                            );
                            return;
                        }
                    }
                    Ok(_) => {}
                    Err(_) => {
                        // This error will trigger if parsing fails, which is what's happening now.
                        // With the correct structs, this should no longer be hit for data messages.
                    }
                }
            }
        } else {
            error!("[{}] Connection failed", name);
        }

        warn!(
            "[{}] Listener DOWN. Clearing price and reconnecting in 5s...",
            name
        );
        let _ = event_tx.send(clear_event.clone());
        time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn coinbase_advanced_listener(crypto: Crypto, event_tx: Arc<CountingSender>) {
    let product_id = format!("{}-USD", crypto);
    let url = "wss://advanced-trade-ws.coinbase.com";
    let heartbeat_sub_msg = json!({"type": "subscribe", "channel": "heartbeats"}).to_string();
    let level2_sub_msg =
        json!({"type": "subscribe", "product_ids": [product_id], "channel": "level2" }).to_string();
    let name = format!("Coinbase_Advanced_{}_Spot", crypto);
    let clear_event = SocketEvent::ClearPrice {
        exchange: Exchange::CoinbaseAdvanced,
        instrument: Instrument::Spot,
        crypto,
    };

    loop {
        if let Ok((ws_stream, _)) = connect_async(url).await {
            let (mut write, mut read) = ws_stream.split();
            if write
                .send(Message::Text(heartbeat_sub_msg.clone()))
                .await
                .is_err()
                || write
                    .send(Message::Text(level2_sub_msg.clone()))
                    .await
                    .is_err()
            {
                let _ = event_tx.send(clear_event.clone());
                time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            let mut bids = BTreeMap::new();
            let mut asks = BTreeMap::new();

            let mut ping_interval = time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    Some(msg_result) = read.next() => {
                        let msg = match msg_result {
                            Ok(m) => m,
                            Err(e) => {
                                error!("[{}] WebSocket stream error: {}. Reconnecting...", name, e);
                                break;
                            }
                        };

                        if !msg.is_text() { continue; }
                        let mut vec = match msg.into_text() {
                            Ok(text) => text.into_bytes(),
                            Err(_) => continue,
                        };


                        match simd_json::from_slice::<CoinbaseAdvancedMsg>(&mut vec) {
                            Ok(CoinbaseAdvancedMsg::L2Data(l2_data)) => {
                                for event in l2_data.events {
                                    if event.event_type == "snapshot" { bids.clear(); asks.clear(); }
                                    for update in event.updates {
                                        let book = if update.side == "bid" { &mut bids } else { &mut asks };
                                        if let Ok(price) = update.price_level.parse::<f64>() {
                                            if update.new_quantity == "0" { book.remove(&OrderedFloat(price)); }
                                            else { book.insert(OrderedFloat(price), BString::from(update.new_quantity)); }
                                        }
                                    }
                                }
                                if let (Some((bid_key, _)), Some((ask_key, _))) = (bids.last_key_value(), asks.first_key_value()) {
                                    // if let Some(rate) = rates.get_usd_usdt_coinbase() {
                                    //     let bid = **bid_key / rate;
                                    //     let ask = **ask_key / rate;
                                    //     let price_update = CryptoPriceUpdate {
                                    //         symbol: None,
                                    //         best_bid_price: bid,
                                    //         best_bid_vol: 0.0,
                                    //         best_ask_price: ask,
                                    //         best_ask_vol: 0.0,
                                    //     };
                                    //     if event_tx.send(SocketEvent::Price {
                                    //         exchange: Exchange::CoinbaseAdvanced,
                                    //         instrument: Instrument::Spot,
                                    //         crypto,
                                    //         depth: OrderbookDepth::L1,
                                    //         price_update,
                                    //     }).is_err() {
                                    //         error!("[{}] Failed to forward price update", name);
                                    //         break;
                                    //     }
                                    // }
                                }
                            }
                            Ok(CoinbaseAdvancedMsg::Heartbeats(_)) => {
                                // Heartbeat received. No action needed.
                            }
                            _ => { /* Other messages or parse errors are ignored */ }
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(e) = write.send(Message::Ping(vec![])).await {
                            error!("[{}] Failed to send proactive ping: {}", name, e);
                            break;
                        }
                    }
                }
            }
        }
        warn!(
            "[{}] Listener DOWN. Clearing price and reconnecting in 5s...",
            name
        );
        let _ = event_tx.send(clear_event.clone());
        time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn okx_listener(crypto: Crypto, is_perp: bool, event_tx: Arc<CountingSender>) {
    let (name_prefix, inst_id, instrument) = if is_perp {
        (
            "Perp",
            format!("{}-USDT-SWAP", crypto),
            Instrument::Perpetual,
        )
    } else {
        ("Spot", format!("{}-USDT", crypto), Instrument::Spot)
    };
    let name = format!("OKX_{}_{}", crypto, name_prefix);
    let url = "wss://ws.okx.com:8443/ws/v5/public";
    let subscribe_msg =
        json!({"op": "subscribe", "args": [{"channel": "bbo-tbt", "instId": inst_id}]}).to_string();
    let clear_event = SocketEvent::ClearPrice {
        exchange: Exchange::Okx,
        instrument,
        crypto,
    };

    loop {
        if let Ok((ws_stream, _)) = connect_async(url).await {
            let (mut write, mut read) = ws_stream.split();
            if write
                .send(Message::Text(subscribe_msg.clone()))
                .await
                .is_err()
            {
                let _ = event_tx.send(clear_event.clone());
                time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            let mut ping_interval = time::interval(Duration::from_secs(30));

            loop {
                tokio::select! {
                    Some(Ok(msg)) = read.next() => {
                        if !msg.is_text() { continue; }
                        let text = msg.into_text().unwrap();
                        if text == "pong" { continue; }
                        let mut v = text.into_bytes();
                        if let Ok(OkxMsg::Data(msg_data)) = simd_json::from_slice::<OkxMsg>(&mut v) {
                            if let Some(tick) = msg_data.data.first() {
                                if let (Some(best_bid), Some(best_ask)) = (tick.bids.first(), tick.asks.first()) {
                                    if let (Ok(bid), Ok(ask)) = (best_bid[0].parse::<f64>(), best_ask[0].parse::<f64>()) {
                                        let price_update = CryptoPriceUpdate {
                                            symbol: None,
                                            best_bid_price: bid,
                                            best_bid_vol: 0.0,
                                            best_ask_price: ask,
                                            best_ask_vol: 0.0,
                                        };
                                        if event_tx.send(SocketEvent::Price {
                                            exchange: Exchange::Okx,
                                            instrument,
                                            crypto,
                                            depth: OrderbookDepth::L1,
                                            price_update,
                                        }).is_err() {
                                            error!("[{}] Failed to forward price update", name);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    },
                    _ = ping_interval.tick() => {
                        if write.send(Message::Text("ping".to_string())).await.is_err() { break; }
                    }
                }
            }
        }
        warn!(
            "[{}] Listener DOWN. Clearing price and reconnecting in 5s...",
            name
        );
        let _ = event_tx.send(clear_event.clone());
        time::sleep(Duration::from_secs(5)).await;
    }
}

pub async fn kraken_listener(crypto: Crypto, is_perp: bool, event_tx: Arc<CountingSender>) {
    // We only care about perpetuals for Kraken, as requested.
    if !is_perp {
        return;
    }

    let instrument = Instrument::Perpetual;
    let name = format!("Kraken_{}_Perp", crypto);
    let url = "wss://futures.kraken.com/ws/v1";

    // --- THIS IS THE FIX ---
    // Change product IDs from "...usdt" to "...usd" to match the liquid markets.
    // Also, Kraken uses XBT for Bitcoin in its futures markets.
    let symbol = match crypto {
        Crypto::BTC => "pf_xbtusd", // Correct: XBT instead of BTC, and usd instead of usdt
        Crypto::ETH => "pf_ethusd",
        Crypto::SOL => "pf_solusd",
        Crypto::XRP => "pf_xrpusd",
    };

    let subscribe_msg = json!({
        "event": "subscribe",
        "feed": "book",
        "product_ids": [symbol]
    })
    .to_string();

    let clear_event = SocketEvent::ClearPrice {
        exchange: Exchange::Kraken,
        instrument,
        crypto,
    };

    loop {
        if let Ok((ws_stream, _)) = connect_async(url).await {
            let (mut write, mut read) = ws_stream.split();
            info!("[{}] Connected.", name);

            if let Err(e) = write.send(Message::Text(subscribe_msg.clone())).await {
                error!("[{}] Failed to subscribe: {}. Retrying...", name, e);
                let _ = event_tx.send(clear_event.clone());
                time::sleep(Duration::from_secs(5)).await;
                continue;
            }

            while let Some(Ok(msg)) = read.next().await {
                if !msg.is_text() {
                    continue;
                }
                let mut text = msg.into_text().unwrap();

                match unsafe { simd_json::from_str::<KrakenMsg>(&mut text) } {
                    Ok(KrakenMsg::BookSnapshot { feed, data }) if feed == "book_snapshot" => {
                        let bids: Vec<OrderbookLevel> = data
                            .bids
                            .into_iter()
                            .map(|level| OrderbookLevel::new(level.price, level.qty))
                            .collect();
                        let asks: Vec<OrderbookLevel> = data
                            .asks
                            .into_iter()
                            .map(|level| OrderbookLevel::new(level.price, level.qty))
                            .collect();

                        if event_tx
                            .send(SocketEvent::L2Snapshot {
                                exchange: Exchange::Kraken,
                                instrument,
                                crypto,
                                bids,
                                asks,
                            })
                            .is_err()
                        {
                            error!("[{}] Failed to forward L2 snapshot event.", name);
                            break;
                        }
                    }
                    Ok(KrakenMsg::BookUpdate { feed, data }) if feed == "book" => {
                        let level = OrderbookLevel::new(data.price, data.qty);
                        let (mut bids, mut asks) = (Vec::new(), Vec::new());

                        if data.side == "buy" {
                            bids.push(level);
                        } else {
                            asks.push(level);
                        }

                        if event_tx
                            .send(SocketEvent::L2Update {
                                exchange: Exchange::Kraken,
                                instrument,
                                crypto,
                                bids,
                                asks,
                            })
                            .is_err()
                        {
                            error!("[{}] Failed to forward L2 update event.", name);
                            break;
                        }
                    }
                    Ok(KrakenMsg::Subscribed { event, .. }) if event == "subscribed" => {
                        info!(
                            "[{}] Successfully subscribed to book feed for {}.",
                            name, symbol
                        );
                    }
                    Ok(_) => {
                        // All other valid messages (events, etc.) are ignored.
                    }
                    Err(e) => {
                        warn!(
                            "[{}] Failed to parse message: {}. Raw: {}",
                            name,
                            e,
                            text.trim()
                        );
                    }
                }
            }
        } else {
            error!("[{}] Connection failed", name);
        }

        warn!(
            "[{}] Listener DOWN. Clearing price and reconnecting in 5s...",
            name
        );
        let _ = event_tx.send(clear_event.clone());
        time::sleep(Duration::from_secs(5)).await;
    }
}
