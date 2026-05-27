//! HTTP Push API and SSE Telemetry logic for Homelab CLIENT.
//!
//! `trigger_deployment` is the main entry point called from the event loop when the
//! user presses 's' on the Scaffolding tab. It:
//!   1. POSTs to the LXC daemon's /api/sync endpoint to queue a GitOps sync.
//!   2. Connects to /api/logs/stream (SSE) and streams log lines into `log_buffer`.
use std::sync::{Arc, Mutex};
use anyhow::Result;
use futures::StreamExt;
use reqwest::Client;

/// Shared log buffer — background SSE task appends lines here.
pub type LogBuffer = Arc<Mutex<Vec<String>>>;

/// Sends a sync trigger POST to `post_url`, then subscribes to the SSE log stream
/// at `sse_url`, pushing received lines into `log_buffer` until the stream closes.
///
/// The function is cheap to spawn in a background `tokio::spawn` task.
pub async fn trigger_deployment(post_url: &str, sse_url: &str, token: &str, log_buffer: LogBuffer) -> Result<()> {
    let client = Client::new();

    // ── Step 1: Trigger sync ───────────────────────────────────────────────
    let mut req = client.post(post_url);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let res = req.send().await?;
    if !res.status().is_success() {
        return Err(anyhow::anyhow!("POST {} returned {}", post_url, res.status()));
    }

    // ── Step 2: Stream SSE log lines ──────────────────────────────────────
    // The LXC daemon sends newline-delimited SSE text:
    //   data: <logfmt line>\n\n
    // We parse it manually to avoid reqwest-eventsource version coupling.
    let sse_res = client.get(sse_url).send().await?;
    let mut byte_stream = sse_res.bytes_stream();
    let mut buf = String::new();

    while let Some(chunk_result) = byte_stream.next().await {
        let bytes = match chunk_result {
            Ok(b) => b,
            Err(e) => {
                let mut logs = log_buffer.lock().unwrap();
                logs.push(format!("[SSE error] {}", e));
                break;
            }
        };
        if let Ok(text) = std::str::from_utf8(&bytes) {
            buf.push_str(text);
        }

        // SSE events are separated by a blank line (\n\n)
        while let Some(sep) = buf.find("\n\n") {
            let raw_event = buf[..sep].to_string();
            buf.drain(..sep + 2);

            for line in raw_event.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    let mut logs = log_buffer.lock().unwrap();
                    logs.push(data.to_string());
                    if logs.len() > 500 {
                        logs.remove(0);
                    }
                }
            }
        }
    }

    Ok(())
}
