use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::json;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info};
use url::Url;

/// Connects to Helius WS endpoint and subscribes to transactions mentioning `target_pubkey`
/// using `transactionSubscribe` with `mentions`.
///
/// Yields raw JSON messages (as serde_json::Value).
pub async fn stream_transactions(
    ws_endpoint: &str,
    target_pubkey: &str,
) -> Result<impl futures_util::Stream<Item = serde_json::Value>> {
    let url = Url::parse(ws_endpoint)?;
    let (ws_stream, _) = connect_async(url).await?;
    let (mut write, read) = ws_stream.split();

    // Helius supports standard Solana WS methods; we use transactionSubscribe.
    // Using "processed" for low latency.
    let sub = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "transactionSubscribe",
        "params": [
            { "mentions": [target_pubkey] },
            {
              "commitment": "processed",
              "encoding": "base64",
              "transactionDetails": "full",
              "showRewards": false,
              "maxSupportedTransactionVersion": 0
            }
        ]
    });

    write.send(Message::Text(sub.to_string())).await?;
    info!("Subscribed to Helius WS transaction stream for TARGET_PUBKEY={target_pubkey}");

    // Convert tungstenite messages -> JSON Values
    let stream = read.filter_map(|msg| async move {
        match msg {
            Ok(Message::Text(t)) => match serde_json::from_str::<serde_json::Value>(&t) {
                Ok(v) => Some(v),
                Err(e) => {
                    debug!("Non-json text msg: {e}");
                    None
                }
            },
            Ok(Message::Binary(b)) => {
                // Sometimes servers send binary; try parse as utf8 json.
                match String::from_utf8(b) {
                    Ok(s) => serde_json::from_str::<serde_json::Value>(&s).ok(),
                    Err(_) => None,
                }
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => None,
            Ok(Message::Close(_)) => {
                error!("WS closed by server");
                None
            }
            Err(e) => {
                error!("WS error: {e}");
                None
            }
            _ => None,
        }
    });

    Ok(stream)
}

/// Small reconnect helper: tries to connect forever and returns a stream each time.
/// (Used internally by engine.)
pub async fn connect_forever(
    ws_endpoint: String,
    target_pubkey: String,
) -> Result<impl futures_util::Stream<Item = serde_json::Value>> {
    loop {
        match stream_transactions(&ws_endpoint, &target_pubkey).await {
            Ok(s) => return Ok(s),
            Err(e) => {
                error!("WS connect failed: {e}. Reconnecting in 3s...");
                sleep(Duration::from_secs(3)).await;
            }
        }
    }
}
