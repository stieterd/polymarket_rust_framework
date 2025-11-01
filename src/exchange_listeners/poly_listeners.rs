use crate::credentials::{POLY_API_KEY, POLY_API_PASSPHRASE, POLY_API_SECRET};
use crate::exchange_listeners::poly_models::Listener;

use super::event_processor::{CountingSender, SocketEvent};
use super::poly_models::{ClobAuth, Subscription, SubscriptionRequest};
use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use log::{error, info, warn};
use rustls::{OwnedTrustAnchor, RootCertStore};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time;
use tokio_rustls::rustls::{ClientConfig, ServerName};
use tokio_rustls::TlsConnector;
use tokio_tungstenite::{
    client_async, tungstenite::client::IntoClientRequest, tungstenite::protocol::Message,
    WebSocketStream,
};

// --- Configuration (Unchanged) ---
const POLY_WEBSOCKET_URL: &str = "wss://ws-live-data.polymarket.com/";
const POLY_WEBSOCKET_URL_OLD: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
const POLY_USER_WEBSOCKET_URL_OLD: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/user";
const PING_INTERVAL_S: u64 = 15;
const MAX_ASSETS_PER_SUB: usize = 500;

/// Establishes a WebSocket connection forcing TLS 1.2.
async fn connect_with_tls12(
    url: &str,
) -> Result<WebSocketStream<tokio_rustls::client::TlsStream<TcpStream>>> {
    let mut root_cert_store = RootCertStore::empty();
    root_cert_store.add_server_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.0.iter().map(|ta| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            ta.subject,
            ta.spki,
            ta.name_constraints,
        )
    }));
    let config = ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(root_cert_store)
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));
    let request = url.into_client_request()?;
    let host = request
        .uri()
        .host()
        .ok_or_else(|| anyhow!("URL has no host"))?;
    let port = request.uri().port_u16().unwrap_or(443);
    let server_name = ServerName::try_from(host)?;
    let tcp_stream = TcpStream::connect(format!("{}:{}", host, port)).await?;
    let tls_stream = connector.connect(server_name, tcp_stream).await?;
    let (ws_stream, _) = client_async(request, tls_stream).await?;
    Ok(ws_stream)
}

/// Generic handler for Polymarket WebSocket connections.
async fn polymarket_websocket_handler(
    listener: Listener,
    initial_subscription: SubscriptionRequest<'_>,
    event_tx: Arc<CountingSender>,
) {
    let sub_msg_str = serde_json::to_string(&initial_subscription).unwrap();
    polymarket_websocket_handler_with_message(listener, POLY_WEBSOCKET_URL, sub_msg_str, event_tx)
        .await;
}

async fn polymarket_websocket_handler_with_message(
    listener: Listener,
    url: &str,
    subscription_message: String,
    event_tx: Arc<CountingSender>,
) {
    loop {
        match connect_with_tls12(url).await {
            Ok(ws_stream) => {
                info!("[{}] TLS 1.2 connection established.", listener);
                let (mut write, mut read) = ws_stream.split();
                if let Err(e) = write
                    .send(Message::Text(subscription_message.clone()))
                    .await
                {
                    error!("[{}] Failed to subscribe: {}. Retrying...", listener, e);
                    time::sleep(Duration::from_secs(5)).await;
                    continue;
                }
                info!("[{}] Subscription message sent.", listener);
                let mut ping_interval = time::interval(Duration::from_secs(PING_INTERVAL_S));
                loop {
                    tokio::select! {
                        Some(msg_result) = read.next() => {
                            match msg_result {
                                Ok(Message::Text(text)) => {
                                    if listener.is_market() {
                                        if let Err(e) = event_tx.send(SocketEvent::Market {
                                            listener,
                                            payload: text.into_bytes(),
                                        }) {
                                            error!("[{}] Failed to forward market event: {}", listener, e);
                                        }
                                    }
                                    else if listener.is_user() {
                                        if let Err(e) = event_tx.send(SocketEvent::User {
                                            listener,
                                            payload: text.into_bytes(),
                                        }) {
                                            error!("[{}] Failed to forward market event: {}", listener, e);
                                        }
                                    }
                                }
                                Ok(Message::Ping(p)) => { if write.send(Message::Pong(p)).await.is_err() { break; } },
                                Ok(Message::Close(_)) => { warn!("[{}] Connection closed by server.", listener); break; },
                                Err(e) => { error!("[{}] WebSocket stream error: {}.", listener, e); break; },
                                _ => {}
                            }
                        }
                        _ = ping_interval.tick() => {
                            if write.send(Message::Text(r#"{"type":"ping"}"#.to_string())).await.is_err() {
                                error!("[{}] Failed to send app-level ping.", listener); break;
                            }
                        }
                    }
                }
            }
            Err(e) => error!("[{}] Connection failed: {}", listener, e),
        }
        warn!("[{}] Listener DOWN. Reconnecting in 5s...", listener);
        time::sleep(Duration::from_secs(5)).await;
    }
}

// --- Public Listener Functions (Unchanged) ---
pub async fn polymarket_market_listener(asset_ids: &[String], event_tx: Arc<CountingSender>) {
    if asset_ids.is_empty() {
        warn!(
            "[{}] No asset IDs provided. Listener will not start.",
            Listener::PolyMarket
        );
        return;
    }
    let market_types = ["agg_orderbook", "price_change", "tick_size_change"];
    let mut subscriptions = Vec::new();
    for chunk in asset_ids.chunks(MAX_ASSETS_PER_SUB) {
        let filters = serde_json::to_string(chunk).unwrap();
        for &market_type in &market_types {
            subscriptions.push(Subscription {
                topic: "clob_market",
                sub_type: market_type,
                filters: Some(filters.clone()),
                clob_auth: None,
            });
        }
    }
    let sub_request = SubscriptionRequest {
        action: "subscribe",
        subscriptions,
    };
    polymarket_websocket_handler(Listener::PolyMarket, sub_request, event_tx.clone()).await;
}

pub async fn polymarket_market_listener_legacy(
    asset_ids: &Vec<&str>,
    event_tx: Arc<CountingSender>,
) {
    if asset_ids.is_empty() {
        warn!(
            "[{}] No asset IDs provided. Listener will not start.",
            Listener::PolyMarketLegacy
        );
        return;
    }

    let subscription_msg = json!({
        "assets_ids": asset_ids,
        "type": "market"
    })
    .to_string();

    polymarket_websocket_handler_with_message(
        Listener::PolyMarketLegacy,
        POLY_WEBSOCKET_URL_OLD,
        subscription_msg,
        event_tx.clone(),
    )
    .await;
}

pub async fn polymarket_user_listener(event_tx: Arc<CountingSender>) {
    let auth = ClobAuth {
        key: POLY_API_KEY,
        secret: POLY_API_SECRET,
        passphrase: POLY_API_PASSPHRASE,
    };
    let subscription = Subscription {
        topic: "clob_user",
        sub_type: "*",
        filters: None,
        clob_auth: Some(auth),
    };
    let sub_request = SubscriptionRequest {
        action: "subscribe",
        subscriptions: vec![subscription],
    };
    polymarket_websocket_handler(Listener::PolyUser, sub_request, event_tx.clone()).await;
}

pub async fn polymarket_user_listener_legacy(event_tx: Arc<CountingSender>) {
    let subscription_msg = json!({
        "auth": {
            "apiKey": POLY_API_KEY,
            "secret": POLY_API_SECRET,
            "passphrase": POLY_API_PASSPHRASE
        },
        "markets": [],
        "type": "user"
    })
    .to_string();

    polymarket_websocket_handler_with_message(
        Listener::PolyUserLegacy,
        POLY_USER_WEBSOCKET_URL_OLD,
        subscription_msg,
        event_tx.clone(),
    )
    .await;
}
