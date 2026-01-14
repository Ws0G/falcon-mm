//! falcon/src/main.rs — latency-measuring Polymarket WS login + ping-pong + Coinbase BTC feed
use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

mod coinbase;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .json()
        .init();

    // BTC fair-value feed
    let (btc_tx, mut btc_rx) = mpsc::channel::<Decimal>(16);
    tokio::spawn(coinbase::btc_mid_loop(btc_tx));

    // Polymarket WS
    let url = "wss://ws-subscriptions-clob.polymarket.com/realtime";
    let api_key = std::env::var("POLY_API_KEY")?;
    let secret = std::env::var("POLY_SECRET")?;
    let passphrase = std::env::var("POLY_PASSPHRASE")?;

    loop {
        tokio::select! {
            _ = btc_rx.recv() => {}, // consume BTC mid for later skew
            res = run_ws(url, &api_key, &secret, &passphrase) => {
                if let Err(e) = res { tracing::error!(ws_error = %e); }
            }
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn run_ws(url: &str, api_key: &str, secret: &str, passphrase: &str) -> Result<()> {
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    tracing::info!("TCP+TLS handshake complete");

    // login
    let ts = Utc::now().timestamp_millis();
    let sig = hex::encode(hmacsha256::HMAC::mac(format!("{}{}", ts, "GET/users/self"), secret.as_bytes()));
    let login = serde_json::json!({
        "type": "login",
        "key": api_key,
        "signature": sig,
        "timestamp": ts,
        "passphrase": passphrase
    });
    write.send(tokio_tungstenite::tungstenite::Message::Text(serde_json::to_string(&login)?)).await?;
    tracing::info!("login sent");

    // latency loop
    while let Some(msg) = read.next().await {
        let msg = msg?;
        let recv_ts = Instant::now();
        match msg {
            tokio_tungstenite::tungstenite::Message::Text(txt) => {
                if let Ok(json) = serde_json::Value::from_str(&txt) {
                    if json["type"] == "subscribed" {
                        tracing::info!(latency_us = ?recv_ts.elapsed().as_micros(), "login confirmed");
                    }
                }
            }
            tokio_tungstenite::tungstenite::Message::Ping(data) => {
                let pong_ts = Instant::now();
                write.send(tokio_tungstenite::tungstenite::Message::Pong(data)).await?;
                tracing::info!(ping_latency_us = ?pong_ts.elapsed().as_micros(), "pong sent");
            }
            _ => {}
        }
    }
    Ok(())
}
