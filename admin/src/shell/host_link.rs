//! arch-host-link: ONE long-lived session to `homelab-host`.
//!
//! Browsers never talk to the host. This task keeps the line open, asks the
//! host for the fleet every `poll` interval, keeps the newest snapshot for
//! pages that load later, and republishes every change over the chassis
//! `live` channel. A dropped line is reopened with a capped backoff; nothing
//! mutating is ever resent on reconnect.
//!
//! Pages ask the host through a [`HostClient`]: a command goes out on the one
//! session with its own id, and the reply with that id comes back to the
//! caller. A request still open when the line drops is answered "outcome
//! unknown", never sent again (arch-host-link).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chassis::shell::live::Live;
use futures_util::{SinkExt, StreamExt};
use homelab_proto::{Command, RpcRequest, RpcResponse, ServerMsg};
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};
use tokio_tungstenite::tungstenite::Message;

use crate::core::asks::Asks;
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
    /// `git describe` of the tree the host was built from (Hello `build`).
    pub host_build: Option<String>,
    /// feat-ops-2: the questions the host asked and may still wait on.
    pub asks: Asks,
    /// Why the line is down, when it is.
    pub link_error: Option<String>,
    /// arch-exposure, lock 2: the house's public address as the host last
    /// read it from the router.
    pub home_address: Option<String>,
}

pub type Shared = Arc<RwLock<Snapshot>>;

type Reply = oneshot::Sender<Result<RpcResponse, String>>;

/// One command for the link task, with where its reply goes and (milestone
/// act) where the id it went out under is told, so the caller can pick its
/// own lines out of the host's `Log` stream.
pub struct Asked {
    command: Command,
    reply: Reply,
    sent: Option<oneshot::Sender<u64>>,
}

/// What the link task drains: the commands, and the channel it republishes
/// the host's `Log`, `Ask` and `Transfer` messages on (milestone act).
pub struct HostAsks {
    rx: mpsc::Receiver<Asked>,
    events: broadcast::Sender<ServerMsg>,
}

/// The pages' way to ask the host something.
#[derive(Clone)]
pub struct HostClient {
    tx: mpsc::Sender<Asked>,
    timeout: Duration,
    events: broadcast::Sender<ServerMsg>,
}

impl HostClient {
    /// A client and the receiving end the link task drains.
    pub fn new(timeout: Duration) -> (Self, HostAsks) {
        let (tx, rx) = mpsc::channel(64);
        let (events, _) = broadcast::channel(1024);
        (
            HostClient {
                tx,
                timeout,
                events: events.clone(),
            },
            HostAsks { rx, events },
        )
    }

    /// milestone act: every `Log`, `Ask` and `Transfer` the host sends on
    /// the one line, from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.events.subscribe()
    }

    /// Send one command and wait for its reply, at most `timeout`.
    pub async fn ask(&self, command: Command) -> Result<RpcResponse, String> {
        self.ask_traced(command, self.timeout, None).await
    }

    /// milestone act: like `ask`, with its own time limit, and the id the
    /// command went out under sent to `sent` the moment it is on the line.
    pub async fn ask_traced(
        &self,
        command: Command,
        timeout: Duration,
        sent: Option<oneshot::Sender<u64>>,
    ) -> Result<RpcResponse, String> {
        let (reply, wait) = oneshot::channel();
        self.tx
            .send(Asked {
                command,
                reply,
                sent,
            })
            .await
            .map_err(|_| "the host link is not running".to_string())?;
        match tokio::time::timeout(timeout, wait).await {
            Ok(Ok(answer)) => answer,
            Ok(Err(_)) => Err("the host link stopped before the answer came".into()),
            Err(_) => Err(format!(
                "no answer from the host within {} s",
                timeout.as_secs()
            )),
        }
    }
}

pub struct LinkConfig {
    pub poll: Duration,
    pub backoff_min: Duration,
    pub backoff_max: Duration,
    /// feat-ops-2: how long the host waits for an answer (arch-config).
    pub ask_timeout_s: u64,
}

/// feat-ops-2: tell every open page which questions are open now.
pub async fn publish_asks(shared: &Shared, live: &Live) {
    let open = shared.read().await.asks.open(now_s());
    let _ = live.publish("asks", &serde_json::json!({ "asks": open, "now": now_s() }));
}

pub fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Runs until the process ends.
pub async fn run(
    target: HostTarget,
    cfg: LinkConfig,
    shared: Shared,
    live: Live,
    mut asks: HostAsks,
) {
    let mut backoff = cfg.backoff_min;
    loop {
        match session(&target, &cfg, &shared, &live, &mut asks).await {
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
    asks: &mut HostAsks,
) -> Result<(), String> {
    let mut pending: HashMap<u64, Reply> = HashMap::new();
    let result = session_loop(target, cfg, shared, live, asks, &mut pending).await;
    // arch-host-link: whatever was asked and not answered is not sent again.
    for (_, reply) in pending.drain() {
        let _ = reply.send(Err(
            "the line to the host dropped before the answer; the outcome is unknown".into(),
        ));
    }
    result
}

async fn session_loop(
    target: &HostTarget,
    cfg: &LinkConfig,
    shared: &Shared,
    live: &Live,
    asks: &mut HostAsks,
    pending: &mut HashMap<u64, Reply>,
) -> Result<(), String> {
    let built_in = homelab_client::repo_config::built_in_pin();
    let link = homelab_client::link::connect(&target.addr, &target.token, None, built_in).await?;
    let (mut tx, mut rx) = link.ws.split();
    // milestone act: ids start at the second the line opened, shifted past
    // any id a CLI or TUI session uses (they count from 1), so a `Log`'s
    // `req` names this session's request and an id is never reused after a
    // reconnect. Stays below 2^53, so the browser reads it exactly.
    let mut next_id: u64 = now_s() << 20;
    let mut tick = tokio::time::interval(cfg.poll);
    let mut greeted = false;

    loop {
        tokio::select! {
            Some(Asked { command, reply, sent }) = asks.rx.recv(), if greeted => {
                let id = next_id;
                next_id += 1;
                let req = RpcRequest { id, command };
                let text = serde_json::to_string(&req).map_err(|e| e.to_string())?;
                pending.insert(id, reply);
                tx.send(Message::Text(text.into())).await.map_err(|e| format!("send: {e}"))?;
                if let Some(sent) = sent {
                    let _ = sent.send(id);
                }
            }
            _ = tick.tick(), if greeted => {
                // feat-ops-2: a question past the host's wait is gone there.
                let expired = shared.write().await.asks.prune(now_s());
                if expired {
                    publish_asks(shared, live).await;
                }
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
                    ServerMsg::Hello { version, build, .. } => {
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
                        // feat-platform-10: `homelab ui` steps come here. A
                        // host older than this dashboard does not know the
                        // command and would log it as an unreadable frame.
                        if !homelab_client::version::older(&version, env!("CARGO_PKG_VERSION")) {
                            let attach = RpcRequest { id: next_id, command: Command::UiAttach };
                            next_id += 1;
                            let text = serde_json::to_string(&attach).map_err(|e| e.to_string())?;
                            tx.send(Message::Text(text.into())).await.map_err(|e| format!("send: {e}"))?;
                        }
                        let mut s = shared.write().await;
                        s.host_version = Some(version.clone());
                        s.host_build = build.clone();
                        s.link_error = None;
                        drop(s);
                        let _ = live.publish("link", &serde_json::json!({ "up": true, "host_version": version, "host_build": build }));
                    }
                    // feat-ops-2: an operation stopped and waits for a person.
                    // It also goes on to whoever follows the line (act).
                    ask @ ServerMsg::Ask { .. } => {
                        if let ServerMsg::Ask { id, op, step, what, if_allowed, if_stopped, boot } = ask.clone() {
                            shared.write().await.asks.heard(
                                id, boot, op, step, what, if_allowed, if_stopped,
                                now_s(), cfg.ask_timeout_s,
                            );
                        }
                        publish_asks(shared, live).await;
                        let _ = asks.events.send(ask);
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
                    ServerMsg::RpcDone(resp) => {
                        if let Some(reply) = pending.remove(&resp.id) {
                            let _ = reply.send(Ok(resp));
                        }
                    }
                    // milestone act: operation lines, questions and byte
                    // counters go to whoever follows them (actions, progress).
                    // feat-platform-10: a UI step goes to the driver.
                    other @ (ServerMsg::Log { .. } | ServerMsg::Transfer { .. } | ServerMsg::Ui { .. }) => {
                        let _ = asks.events.send(other);
                    }
                    _ => {}
                }
            }
        }
    }
}

// ── the demo host ───────────────────────────────────────────────────────

/// feat-platform-10: a host that lives in this process, for the browser
/// tests (`HOMELAB_ADMIN_DEMO_HOST=1`). The real line is never opened: the
/// fleet is the stacks named in `stacks` as the working copy describes
/// them, reads answer empty, and every command that would change something
/// only prints three step lines and answers "complete". Nothing leaves the
/// machine.
pub async fn run_demo(
    shared: Shared,
    live: Live,
    mut asks: HostAsks,
    repo: std::path::PathBuf,
    stacks: Vec<String>,
) {
    use homelab_proto::{AppView, FleetState, HostView, LogLevel, StackView, StepMark};
    // Rebuilt on every tick: the working copy is cloned beside this start.
    let build = || FleetState {
        host: HostView {
            name: "demo".into(),
            cpu_pct: 7,
            ram_pct: 40,
            disk_pct: 31,
            tls_fingerprint: String::new(),
            ram_total_mb: 65536,
            ram_used_mb: 26000,
            ram_committed_mb: 30000,
            cores_total: 16,
            load1_x100: 80,
            home_address: None,
        },
        stacks: stacks
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let m = homelab_client::spec::build_manifest(&repo.join("stacks").join(name)).ok();
                StackView {
                    name: name.clone(),
                    vmid: m.as_ref().map(|m| m.vmid).unwrap_or(900 + i as u16),
                    hostname: m
                        .as_ref()
                        .map(|m| m.hostname.clone())
                        .unwrap_or_else(|| name.clone()),
                    apps: m
                        .map(|m| m.apps)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|a| AppView {
                            name: a,
                            running: true,
                            restarts: 0,
                        })
                        .collect(),
                    drift: false,
                    applied_hash: String::new(),
                    env_sealed: false,
                    online: true,
                    enabled: true,
                    usage: None,
                    applied_source: None,
                }
            })
            .collect(),
        status_measured_at: Some(now_s()),
    };
    {
        let mut s = shared.write().await;
        s.fleet = Some(fleet_view(&build(), now_s()));
        s.host_version = Some("demo".into());
        s.host_build = Some("demo host in homelab-admin".into());
        s.link_error = None;
    }
    tracing::warn!(
        "HOMELAB_ADMIN_DEMO_HOST is set: a demo host answers; no real host is contacted"
    );
    let _ = live.publish("link", &serde_json::json!({ "up": true, "host_version": "demo", "host_build": "demo host in homelab-admin" }));
    let mut next: u64 = now_s() << 20;
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            Some(Asked { command, reply, sent }) = asks.rx.recv() => {
                let id = next;
                next += 1;
                if let Some(s) = sent {
                    let _ = s.send(id);
                }
                let answer = move |message: String| RpcResponse { id, ok: true, message, deferred: None };
                if command.is_read_only() {
                    let body = match command {
                        Command::History { .. } => serde_json::json!({ "entries": [] }).to_string(),
                        Command::Incidents { .. } => serde_json::json!({ "incidents": [] }).to_string(),
                        Command::CurrentOp => serde_json::to_string(&homelab_proto::CurrentOpView::default()).unwrap_or_default(),
                        _ => "{}".into(),
                    };
                    let _ = reply.send(Ok(answer(body)));
                    continue;
                }
                let events = asks.events.clone();
                let name = command.name().to_string();
                tokio::spawn(async move {
                    let op = format!("{}-demo", name.split('_').next().unwrap_or("op"));
                    for step in ["prepare", "apply", "verify"] {
                        for finished in [false, true] {
                            let _ = events.send(ServerMsg::Log {
                                level: LogLevel::Info,
                                source: "DEMO".into(),
                                msg: format!("[{op}] {step} {}", if finished { "done" } else { "started" }),
                                req: Some(id),
                                ts: Some(now_s()),
                                step: Some(StepMark { op: op.clone(), step: step.into(), finished, changed: finished }),
                                by: Some("admin".into()),
                            });
                            tokio::time::sleep(Duration::from_millis(600)).await;
                        }
                    }
                    let _ = reply.send(Ok(answer(format!("{name} complete (demo host: nothing ran)"))));
                });
            }
            _ = tick.tick() => {
                let f = fleet_view(&build(), now_s());
                shared.write().await.fleet = Some(f.clone());
                let _ = live.publish("fleet", &serde_json::json!({ "changed": false, "fleet": f }));
            }
        }
    }
}
