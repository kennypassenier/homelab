//! HTTP Push API and SSE Telemetry logic for Homelab Client.
use std::sync::{Arc, Mutex};
use anyhow::Result;
use reqwest::Client;
use reqwest_eventsource::{EventSource, Event};
use tokio_stream::StreamExt;

/// Shared log buffer for SSE events.
pub type LogBuffer = Arc<Mutex<Vec<String>>>;

/// Triggers a deployment via HTTP POST and starts SSE log streaming.
pub async fn trigger_deployment(api_url: &str, token: &str, log_buffer: LogBuffer) -> Result<()> {
    let client = Client::new();
    // Send HTTP POST with Bearer token
    let res = client.post(api_url)
        .bearer_auth(token)
        .send()
        .await?;
    if !res.status().is_success() {
        return Err(anyhow::anyhow!("POST failed: {}", res.status()));
    }
    // Start SSE connection
    let mut es = EventSource::new(client.get(api_url)).unwrap();
    while let Some(event) = es.next().await {
        match event {
            Ok(Event::Open) => {},
            Ok(Event::Message(msg)) => {
                let mut logs = log_buffer.lock().unwrap();
                logs.push(msg.data);
                if logs.len() > 100 { logs.remove(0); }
            },
            Ok(Event::Retry) => {},
            Err(e) => {
                let mut logs = log_buffer.lock().unwrap();
                logs.push(format!("SSE error: {}", e));
            }
        }
    }
    Ok(())
}
