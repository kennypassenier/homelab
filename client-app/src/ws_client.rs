//! WebSocket client for continuous log streaming from HOST and LXC stacks.

use futures::{SinkExt, StreamExt};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const READ_TIMEOUT_SECS: u64 = 30;
const PING_INTERVAL_SECS: u64 = 20;
const STALE_TIMEOUT_SECS: u64 = 240;

/// After this many consecutive failures, stop logging each reconnect attempt.
/// One suppression notice is logged instead, then only once per SPAM_QUIET_SECS.
const SPAM_SUPPRESS_AFTER: u32 = 3;
const SPAM_QUIET_SECS: u64 = 120;

/// Reconnect backoff: capped at 60 seconds.
const RECONNECT_BASE_SECS: u64 = 4;
const RECONNECT_MAX_SECS: u64 = 60;

#[derive(Debug, Clone)]
pub enum WsEvent {
    /// Log message from a source (stack_name or "HOST")
    LogMessage { source: String, line: String },
    /// Connection state changed
    ConnectionStateChanged {
        source: String,
        connected: bool,
        error: Option<String>,
    },
}

/// Connect to HOST WebSocket at host:8080/api/logs/ws and stream logs.
/// Sends WsEvent messages on the channel.
pub async fn connect_host_logs(host: &str, port: u16, tx: mpsc::UnboundedSender<WsEvent>) {
    let url = format!("ws://{}:{}/api/logs/ws", host, port);
    stream_logs("HOST".to_string(), &url, tx).await;
}

/// Connect to a LXC stack WebSocket at ip:8080/api/logs/ws and stream logs.
pub async fn connect_lxc_logs(
    stack: &str,
    ip: &str,
    port: u16,
    tx: mpsc::UnboundedSender<WsEvent>,
) {
    let source = format!("lxc-{}", stack);
    let url = format!("ws://{}:{}/api/logs/ws", ip, port);
    stream_logs(source, &url, tx).await;
}

async fn stream_logs(source: String, url: &str, tx: mpsc::UnboundedSender<WsEvent>) {
    let mut consecutive_failures: u32 = 0;
    let mut last_spam_notice = Instant::now()
        .checked_sub(Duration::from_secs(9999))
        .unwrap_or_else(Instant::now);

    loop {
        match tokio_tungstenite::connect_async(url).await {
            Ok((ws_stream, _)) => {
                // Reset backoff on successful connection.
                consecutive_failures = 0;

                let _ = tx.send(WsEvent::ConnectionStateChanged {
                    source: source.clone(),
                    connected: true,
                    error: None,
                });

                let (mut write, mut read) = ws_stream.split();
                let mut ping_tick = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
                let mut last_activity = Instant::now();

                loop {
                    tokio::select! {
                        _ = ping_tick.tick() => {
                            if last_activity.elapsed().as_secs() >= STALE_TIMEOUT_SECS {
                                let _ = tx.send(WsEvent::ConnectionStateChanged {
                                    source: source.clone(),
                                    connected: false,
                                    error: Some("stale websocket: reconnecting".to_string()),
                                });
                                break;
                            }
                            if write.send(Message::Ping(Vec::new())).await.is_err() {
                                let _ = tx.send(WsEvent::ConnectionStateChanged {
                                    source: source.clone(),
                                    connected: false,
                                    error: Some("keepalive ping failed".to_string()),
                                });
                                break;
                            }
                        }
                        frame = tokio::time::timeout(Duration::from_secs(READ_TIMEOUT_SECS), read.next()) => {
                            match frame {
                                Ok(Some(Ok(Message::Text(line)))) => {
                                    last_activity = Instant::now();
                                    // Daemons emit JSON keepalive frames; don't render these as logs.
                                    if line.contains("\"kind\":\"ws_keepalive\"") {
                                        continue;
                                    }
                                    let _ = tx.send(WsEvent::LogMessage {
                                        source: source.clone(),
                                        line,
                                    });
                                }
                                Ok(Some(Ok(Message::Ping(payload)))) => {
                                    last_activity = Instant::now();
                                    if write.send(Message::Pong(payload)).await.is_err() {
                                        let _ = tx.send(WsEvent::ConnectionStateChanged {
                                            source: source.clone(),
                                            connected: false,
                                            error: Some("failed to send pong".to_string()),
                                        });
                                        break;
                                    }
                                }
                                Ok(Some(Ok(Message::Pong(_)))) | Ok(Some(Ok(Message::Binary(_)))) => {
                                    last_activity = Instant::now();
                                }
                                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
                                    let _ = tx.send(WsEvent::ConnectionStateChanged {
                                        source: source.clone(),
                                        connected: false,
                                        error: Some("Connection closed".to_string()),
                                    });
                                    break;
                                }
                                Ok(Some(Err(e))) => {
                                    let _ = tx.send(WsEvent::ConnectionStateChanged {
                                        source: source.clone(),
                                        connected: false,
                                        error: Some(e.to_string()),
                                    });
                                    break;
                                }
                                Err(_) => {
                                    if last_activity.elapsed().as_secs() >= STALE_TIMEOUT_SECS {
                                        let _ = tx.send(WsEvent::ConnectionStateChanged {
                                            source: source.clone(),
                                            connected: false,
                                            error: Some("stale websocket: reconnecting".to_string()),
                                        });
                                        break;
                                    }
                                }
                                _ => continue,
                            }
                        }
                    }
                }
            }
            Err(e) => {
                consecutive_failures += 1;

                if consecutive_failures <= SPAM_SUPPRESS_AFTER {
                    // Log the first few failures verbatim so the user sees what's happening.
                    let _ = tx.send(WsEvent::ConnectionStateChanged {
                        source: source.clone(),
                        connected: false,
                        error: Some(e.to_string()),
                    });
                } else if last_spam_notice.elapsed().as_secs() >= SPAM_QUIET_SECS {
                    // After spam threshold: emit one suppressed notice every SPAM_QUIET_SECS.
                    let _ = tx.send(WsEvent::ConnectionStateChanged {
                        source: source.clone(),
                        connected: false,
                        error: Some(format!(
                            "still unreachable (attempt {}) — waiting for container to come online",
                            consecutive_failures
                        )),
                    });
                    last_spam_notice = Instant::now();
                }
                // else: silently skip this reconnect attempt
            }
        }

        // Exponential backoff: 4s → 8s → 16s → 32s → 60s (cap).
        let wait_secs =
            (RECONNECT_BASE_SECS * (1u64 << consecutive_failures.min(4))).min(RECONNECT_MAX_SECS);
        tokio::time::sleep(Duration::from_secs(wait_secs)).await;
    }
}
