//! WebSocket client for continuous log streaming from HOST and LXC stacks.

use futures::StreamExt;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

const READ_TIMEOUT_SECS: u64 = 30;
const STALE_TIMEOUT_WINDOWS: u32 = 3;

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

    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws_stream, _)) => {
                let mut idle_windows = 0u32;
                let _ = tx.send(WsEvent::ConnectionStateChanged {
                    source: "HOST".to_string(),
                    connected: true,
                    error: None,
                });

                let (_, mut read) = ws_stream.split();
                loop {
                    match tokio::time::timeout(Duration::from_secs(READ_TIMEOUT_SECS), read.next())
                        .await
                    {
                        Ok(Some(Ok(Message::Text(line)))) => {
                            idle_windows = 0;
                            let _ = tx.send(WsEvent::LogMessage {
                                source: "HOST".to_string(),
                                line,
                            });
                        }
                        Ok(Some(Ok(Message::Close(_)))) | Ok(None) => {
                            let _ = tx.send(WsEvent::ConnectionStateChanged {
                                source: "HOST".to_string(),
                                connected: false,
                                error: Some("Connection closed".to_string()),
                            });
                            break;
                        }
                        Ok(Some(Err(e))) => {
                            let _ = tx.send(WsEvent::ConnectionStateChanged {
                                source: "HOST".to_string(),
                                connected: false,
                                error: Some(e.to_string()),
                            });
                            break;
                        }
                        Err(_) => {
                            idle_windows += 1;
                            if idle_windows >= STALE_TIMEOUT_WINDOWS {
                                let _ = tx.send(WsEvent::ConnectionStateChanged {
                                    source: "HOST".to_string(),
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
            Err(e) => {
                let _ = tx.send(WsEvent::ConnectionStateChanged {
                    source: "HOST".to_string(),
                    connected: false,
                    error: Some(e.to_string()),
                });
            }
        }

        // Wait before reconnecting
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
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

    loop {
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws_stream, _)) => {
                let mut idle_windows = 0u32;
                let _ = tx.send(WsEvent::ConnectionStateChanged {
                    source: source.clone(),
                    connected: true,
                    error: None,
                });

                let (_, mut read) = ws_stream.split();
                loop {
                    match tokio::time::timeout(Duration::from_secs(READ_TIMEOUT_SECS), read.next())
                        .await
                    {
                        Ok(Some(Ok(Message::Text(line)))) => {
                            idle_windows = 0;
                            let _ = tx.send(WsEvent::LogMessage {
                                source: source.clone(),
                                line,
                            });
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
                            idle_windows += 1;
                            if idle_windows >= STALE_TIMEOUT_WINDOWS {
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
            Err(e) => {
                let _ = tx.send(WsEvent::ConnectionStateChanged {
                    source: source.clone(),
                    connected: false,
                    error: Some(e.to_string()),
                });
            }
        }

        // Wait before reconnecting
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
