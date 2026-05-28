//! HTTP Push API and WebSocket telemetry logic for Homelab CLIENT.
//!
//! `trigger_deployment` is the main entry point called from the event loop when the
//! user presses 's' on the Scaffolding tab. It:
//!   1. POSTs to the LXC daemon's /api/sync endpoint to queue a GitOps sync.
//!   2. Connects to /api/logs/ws (WebSocket) and streams log lines into `log_buffer`.
#![allow(dead_code)]

use anyhow::Result;
use futures::StreamExt;
use reqwest::Client;
use std::sync::{Arc, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Shared log buffer — background WebSocket task appends lines here.
pub type LogBuffer = Arc<Mutex<Vec<String>>>;

/// Sends a sync trigger POST to `post_url`, then connects to the WebSocket log
/// stream at `ws_url`, pushing received lines into `log_buffer` until the stream
/// closes.
///
/// The function is cheap to spawn in a background `tokio::spawn` task.
pub async fn trigger_deployment(
    post_url: &str,
    ws_url: &str,
    token: &str,
    log_buffer: LogBuffer,
) -> Result<()> {
    let client = Client::new();

    // ── Step 1: Trigger sync ───────────────────────────────────────────────
    let mut req = client.post(post_url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let res = req.send().await?;
    if !res.status().is_success() {
        return Err(anyhow::anyhow!(
            "POST {} returned {}",
            post_url,
            res.status()
        ));
    }

    // ── Step 2: Stream WebSocket log lines ────────────────────────────────
    // The LXC daemon sends raw logfmt text as WebSocket Text frames.
    let (ws_stream, _) = connect_async(ws_url)
        .await
        .map_err(|e| anyhow::anyhow!("WebSocket connect to {} failed: {}", ws_url, e))?;

    let (_, mut read) = ws_stream.split();

    while let Some(msg_result) = read.next().await {
        match msg_result {
            Ok(Message::Text(text)) => {
                let mut logs = log_buffer.lock().unwrap();
                logs.push(text.into());
                if logs.len() > 500 {
                    logs.remove(0);
                }
            }
            Ok(Message::Close(_)) => break,
            Err(e) => {
                log_buffer.lock().unwrap().push(format!("[WS error] {}", e));
                break;
            }
            _ => {} // ignore binary/ping/pong
        }
    }

    Ok(())
}
