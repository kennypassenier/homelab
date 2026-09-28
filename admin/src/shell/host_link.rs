//! arch-host-link: ONE long-lived session to `homelab-host`.
//!
//! Browsers never talk to the host. This task keeps the line open, asks the
//! host for the fleet every `poll` interval, keeps the newest snapshot for
//! pages that load later, and republishes every change over the chassis
//! `live` channel. A dropped line is reopened with a capped backoff; nothing
//! mutating is ever resent on reconnect (the skeleton sends only reads).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chassis::shell::live::Live;
use futures_util::{SinkExt, StreamExt};
use homelab_proto::{Command, RpcRequest, ServerMsg};
use tokio::sync::RwLock;
use tokio_tungstenite::tungstenite::Message;

use crate::core::fleet::{fleet_view, FleetView};

/// Where the host lives and how to prove who we are.
#[derive(Clone)]
pub struct HostTarget {
    /// `host:port`, e.g. `10.10.10.250:8443`.
    pub addr: String,
    pub token: String,
}

/// What the pages read between two events.
#[derive(Default)]
pub struct Snapshot {
    pub fleet: Option<FleetView>,
    /// The host's version as it said in Hello, e.g. "3.61.4".
    pub host_version: Option<String>,
    /// Why the line is down, when it is.
    pub link_error: Option<String>,
    /// arch-exposure, lock 2: the house's public address as the host last
    /// read it from the router.
    pub home_address: Option<String>,
}

pub type Shared = Arc<RwLock<Snapshot>>;

pub struct LinkConfig {
    pub poll: Duration,
    pub backoff_min: Duration,
    pub backoff_max: Duration,
}

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Runs until the process ends.
pub async fn run(target: HostTarget, cfg: LinkConfig, shared: Shared, live: Live) {
    let mut backoff = cfg.backoff_min;
    loop {
        match session(&target, &cfg, &shared, &live).await {
            Ok(()) => backoff = cfg.backoff_min,
            Err(e) => {
                tracing::warn!(error = %e, "host link down");
                shared.write().await.link_error = Some(e.clone());
                let _ = live.publish("link", &serde_json::json!({ "up": false, "error": e }));
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(cfg.backoff_max);
    }
}

async fn session(
    target: &HostTarget,
    cfg: &LinkConfig,
    shared: &Shared,
    live: &Live,
) -> Result<(), String> {
    let built_in = homelab_client::repo_config::built_in_pin();
    let link = homelab_client::link::connect(&target.addr, &target.token, None, built_in).await?;
    let (mut tx, mut rx) = link.ws.split();
    let mut next_id: u64 = 1;
    let mut tick = tokio::time::interval(cfg.poll);
    let mut greeted = false;

    loop {
        tokio::select! {
            _ = tick.tick(), if greeted => {
                let req = RpcRequest { id: next_id, command: Command::GetState };
                next_id += 1;
                let text = serde_json::to_string(&req).map_err(|e| e.to_string())?;
                tx.send(Message::Text(text.into())).await.map_err(|e| format!("send: {e}"))?;
            }
            msg = rx.next() => {
                let Some(msg) = msg else { return Err("the host closed the line".into()) };
                let msg = msg.map_err(|e| format!("read: {e}"))?;
                let Message::Text(text) = msg else { continue };
                let Ok(server_msg) = serde_json::from_str::<ServerMsg>(&text) else { continue };
                match server_msg {
                    ServerMsg::Hello { version, .. } => {
                        greeted = true;
                        // arch-host-link: this session tells replies apart
                        // by id, so its reads need not wait behind a deploy.
                        let opts = RpcRequest {
                            id: next_id,
                            command: Command::SessionOptions { reads_beside_queue: true },
                        };
                        next_id += 1;
                        let text = serde_json::to_string(&opts).map_err(|e| e.to_string())?;
                        tx.send(Message::Text(text.into())).await.map_err(|e| format!("send: {e}"))?;
                        let mut s = shared.write().await;
                        s.host_version = Some(version.clone());
                        s.link_error = None;
                        drop(s);
                        let _ = live.publish("link", &serde_json::json!({ "up": true, "host_version": version }));
                    }
                    ServerMsg::State(state) => {
                        let view = fleet_view(&state, now_s());
                        let changed = {
                            let s = shared.read().await;
                            s.fleet.as_ref().map(|f| (&f.stacks, &f.counts)) != Some((&view.stacks, &view.counts))
                        };
                        {
                            let mut s = shared.write().await;
                            s.fleet = Some(view.clone());
                            s.home_address = state.host.home_address.clone();
                        }
                        // Every snapshot carries a new "measured at"; the
                        // event goes out on every poll so the page's
                        // "measured x ago" stays honest, and `changed` lets
                        // the page flash only what moved.
                        let _ = live.publish("fleet", &serde_json::json!({ "changed": changed, "fleet": view }));
                    }
                    _ => {}
                }
            }
        }
    }
}
