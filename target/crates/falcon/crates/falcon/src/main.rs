//! falcon/src/main.rs — latency-measuring Polymarket WS login + ping-pong
use anyhow::Result;
use chrono::Utc;
use futures_util::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .json()
        .init();

    let url = "wss://ws-subscriptions-clob.polymarket.com/realtime";
    let api_key = std::env::var("POLY_API_KEY")?;
    let secret = std::env::var("POLY_SECRET")?;
    let passphrase = std::env::var("POLY_PASSPHRASE")?;

    loop {
        match run_once(url, &api_key, &secret, &passphrase).await {
            Ok(_) => tracing::info!("WS loop ended cleanly; reconnecting"),
            Err(e) => tracing::error!("WS error: {}", e),
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn run_once(
    url: &str,
    api_key: &str,
    secret: &str,
    passphrase: &str,
) -> Result<()> {
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, mut read) = ws_stream.split();
    tracing::info!("TCP+TLS handshake complete");

    // --- login ---
    let ts = Utc::now().timestamp_millis();
    let sig = hex::encode(
        hmacsha256::HMAC::mac(
            format!("{}{}", ts, "GET/users/self"),
            secret.as_bytes(),
        ),
    );
    let login = serde_json::json!({
        "type": "login",
        "key": api_key,
        "signature": sig,
        "timestamp": ts,
        "passphrase": passphrase
    });
    let login_str = serde_json::to_string(&login)?;
    write.send(tokio_tungstenite::tungstenite::Message::Text(login_str)).await?;
    tracing::info!("login sent");

    // --- latency loop ---
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
