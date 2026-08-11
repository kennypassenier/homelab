//! Backend trait (AR6): the TUI talks to "something that streams ServerMsg and
//! accepts Commands". Production is the real wss+pinned connection; tests use a
//! scripted TestBackend. The TUI cannot tell them apart.

use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;

use homelab_proto::{Command, RpcRequest, ServerMsg};

use crate::tls;

/// What the TUI receives from whatever it's connected to.
#[derive(Debug, Clone)]
pub enum BackendEvent {
    Connected {
        version: String,
        fingerprint: Option<String>,
    },
    Server(ServerMsg),
    Disconnected(String),
}

pub struct Channels {
    pub cmd_tx: mpsc::Sender<Command>,
    pub evt_rx: mpsc::Receiver<BackendEvent>,
}

pub trait Backend {
    /// Spawn the backend's own task and return channels to talk to it.
    fn start(self: Box<Self>) -> Channels;
}

// ── Remote (real wss + pinning) ─────────────────────────────────────────────

pub struct RemoteBackend {
    pub host: String,
    pub token: String,
}

impl Backend for RemoteBackend {
    fn start(self: Box<Self>) -> Channels {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<BackendEvent>(256);

        tokio::spawn(async move {
            let url = format!("wss://{}/api/ws", self.host);
            let mut request = match url.clone().into_client_request() {
                Ok(r) => r,
                Err(e) => {
                    let _ = evt_tx
                        .send(BackendEvent::Disconnected(format!("bad url: {}", e)))
                        .await;
                    return;
                }
            };
            request.headers_mut().insert(
                "Authorization",
                format!("Bearer {}", self.token).parse().unwrap(),
            );

            let pin = crate::load_pin();
            let first = pin.is_none();
            let verifier = tls::PinnedVerifier::new(pin);
            let tls_config = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(verifier.clone())
                .with_no_client_auth();
            let connector = Connector::Rustls(Arc::new(tls_config));

            let ws = match tokio_tungstenite::connect_async_tls_with_config(
                request,
                None,
                false,
                Some(connector),
            )
            .await
            {
                Ok((ws, _)) => ws,
                Err(e) => {
                    let _ = evt_tx
                        .send(BackendEvent::Disconnected(format!("connect: {}", e)))
                        .await;
                    return;
                }
            };
            let fingerprint = verifier.observed();
            if first {
                if let Some(fp) = &fingerprint {
                    crate::save_pin(fp);
                }
            }
            let (mut tx, mut rx) = ws.split();
            let _ = evt_tx
                .send(BackendEvent::Connected {
                    version: String::new(),
                    fingerprint,
                })
                .await;

            let mut req_id = 1u64;
            loop {
                tokio::select! {
                    Some(cmd) = cmd_rx.recv() => {
                        req_id += 1;
                        let req = RpcRequest { id: req_id, command: cmd };
                        if tx.send(Message::Text(serde_json::to_string(&req).unwrap())).await.is_err() {
                            let _ = evt_tx.send(BackendEvent::Disconnected("send failed".into())).await;
                            break;
                        }
                    }
                    msg = rx.next() => {
                        match msg {
                            Some(Ok(Message::Text(t))) => {
                                if let Ok(sm) = serde_json::from_str::<ServerMsg>(&t) {
                                    if evt_tx.send(BackendEvent::Server(sm)).await.is_err() { break; }
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                let _ = evt_tx.send(BackendEvent::Disconnected("closed".into())).await;
                                break;
                            }
                            Some(Err(e)) => {
                                let _ = evt_tx.send(BackendEvent::Disconnected(format!("ws: {}", e))).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Channels { cmd_tx, evt_rx }
    }
}

// ── Test backend (scripted, no network) ─────────────────────────────────────

/// Emits a scripted sequence of events; used by snapshot tests and the
/// `--offline` demo path. Every command it receives is acknowledged.
pub struct TestBackend {
    pub script: Vec<BackendEvent>,
}

impl Backend for TestBackend {
    fn start(self: Box<Self>) -> Channels {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<BackendEvent>(256);
        tokio::spawn(async move {
            for ev in self.script {
                if evt_tx.send(ev).await.is_err() {
                    return;
                }
            }
            // Keep draining commands so senders don't error.
            while cmd_rx.recv().await.is_some() {}
        });
        Channels { cmd_tx, evt_rx }
    }
}
