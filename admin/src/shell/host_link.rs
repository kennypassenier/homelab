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
use std::time::Duration;

use chassis::shell::live::Live;
use futures_util::{SinkExt, StreamExt};
use homelab_proto::{Command, RpcRequest, RpcResponse, ServerMsg};
use tokio::sync::{RwLock, broadcast, mpsc, oneshot};
use tokio_tungstenite::tungstenite::Message;

use crate::core::asks::Asks;
use crate::core::fleet::{FleetView, fleet_view};

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
    /// TUI parity: the newest homelab release on GitHub, for the "host
    /// update available" badge, and when it was read.
    pub latest_release: Option<String>,
    pub latest_checked_at: Option<u64>,
    /// The line's own facts for the host page: the address and where it was
    /// configured (the TUI's ping shows the same).
    pub host_addr: Option<String>,
    pub host_addr_source: Option<String>,
    /// redesign-stacks: the last day of fleet snapshots, for the Stacks
    /// page's sparklines (`/data/fleet-trend`).
    pub trend: crate::core::trend::Trend,
}

pub type Shared = Arc<RwLock<Snapshot>>;

type Reply = oneshot::Sender<Result<RpcResponse, String>>;

/// One command for the link task, with where its reply goes and (milestone
/// act) where the id it went out under is told, so the caller can pick its
/// own lines out of the host's `Log` stream.
pub struct Asked {
    pub(crate) command: Command,
    pub(crate) reply: Reply,
    pub(crate) sent: Option<oneshot::Sender<u64>>,
}

/// What the link task drains: the commands, and the channel it republishes
/// the host's `Log`, `Ask` and `Transfer` messages on (milestone act).
pub struct HostAsks {
    pub(crate) rx: mpsc::Receiver<Asked>,
    pub(crate) events: broadcast::Sender<ServerMsg>,
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
    /// feat-platform-10: whether this dashboard takes `homelab ui` steps
    /// (`UiAttach`). fix-158: never for one running `dev_without_locks`.
    pub ui_attach: bool,
    /// fix-trend-restart: where the Stacks page's last day of fleet
    /// snapshots is kept (the app database, feat-kpadopt-2), so a restart (a
    /// rollout) does not wipe it; `None`: kept in memory only.
    pub trend_db: Option<chassis::storage::AppDb>,
}

impl LinkConfig {
    /// The line's settings from the dashboard's own.
    pub fn from_admin(c: &crate::core::config::AdminConfig) -> Self {
        let (backoff_min, backoff_max) = c.backoff();
        LinkConfig {
            poll: c.poll(),
            backoff_min,
            backoff_max,
            ask_timeout_s: c.ask_timeout_s,
            // fix-158: a developer's dashboard on the same host token took
            // the steps over from CT 120's, and on exit left it detached.
            ui_attach: !c.dev_without_locks,
            trend_db: None,
        }
    }
}

/// The stored buckets from `oldest_at` (unix seconds) on, oldest first, each
/// with its stacks.
pub(crate) fn trend_since(
    c: &chassis::storage::rusqlite::Connection,
    oldest_at: i64,
) -> Result<Vec<crate::core::trend::Bucket>, chassis::Error> {
    use crate::core::trend::{Bucket, Mean};
    let mean = |sum: i64, n: i64| Mean {
        sum: sum as u64,
        n: n as u64,
    };
    let mut out = Vec::new();
    let mut stmt = c.prepare(
        "SELECT slot, host_cpu_sum, host_cpu_n, load_sum, load_n FROM trend_buckets \
         WHERE at >= ?1 ORDER BY slot",
    )?;
    let rows = stmt
        .query_map([oldest_at], |r| {
            Ok(Bucket {
                slot: r.get::<_, i64>(0)? as u64,
                host_cpu: mean(r.get(1)?, r.get(2)?),
                load_x100: mean(r.get(3)?, r.get(4)?),
                stacks: Default::default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut stacks = c.prepare("SELECT stack, sum, n FROM trend_stacks WHERE slot = ?1")?;
    for mut b in rows {
        let found = stacks
            .query_map([b.slot as i64], |r| {
                Ok((r.get::<_, String>(0)?, mean(r.get(1)?, r.get(2)?)))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        b.stacks = found.into_iter().collect();
        out.push(b);
    }
    Ok(out)
}

/// Store one finished bucket and its stacks, replacing a bucket of the same
/// slot (a restart inside a bucket stores it again), in the caller's
/// transaction.
pub(crate) fn put_bucket(
    tx: &chassis::storage::rusqlite::Transaction,
    b: &crate::core::trend::Bucket,
) -> Result<(), chassis::Error> {
    use chassis::storage::rusqlite::params;
    // The stacks go with the bucket (ON DELETE CASCADE).
    tx.execute(
        "DELETE FROM trend_buckets WHERE slot = ?1",
        params![b.slot as i64],
    )?;
    tx.execute(
        "INSERT INTO trend_buckets (slot, at, host_cpu_sum, host_cpu_n, load_sum, load_n) \
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![
            b.slot as i64,
            b.slot.saturating_mul(crate::core::trend::STEP_S) as i64,
            b.host_cpu.sum as i64,
            b.host_cpu.n as i64,
            b.load_x100.sum as i64,
            b.load_x100.n as i64
        ],
    )?;
    for (name, m) in &b.stacks {
        tx.execute(
            "INSERT INTO trend_stacks (slot, stack, sum, n) VALUES (?1,?2,?3,?4)",
            params![b.slot as i64, name, m.sum as i64, m.n as i64],
        )?;
    }
    Ok(())
}

/// The first host release that knows `UiAttach` (feat-platform-10); an
/// older host would log the frame as unreadable.
pub const UI_ATTACH_SINCE: &str = "3.63.0";

/// What the dashboard sends once the host said Hello (`host_version`), in
/// order: reads beside the queue (arch-host-link), then `UiAttach` when
/// `cfg.ui_attach` and the host knows the command at all (fix-208,
/// CLAUDE.md rule 7: a version check says what the other side supports, it
/// never blocks — this used to demand a host at least as new as the
/// dashboard, so a dashboard updated before its host could not be driven).
pub fn greeting(cfg: &LinkConfig, host_version: &str) -> Vec<Command> {
    let mut out = vec![Command::SessionOptions {
        reads_beside_queue: true,
    }];
    if cfg.ui_attach && !homelab_client::version::older(host_version, UI_ATTACH_SINCE) {
        out.push(Command::UiAttach);
    }
    out
}

/// feat-ops-2: tell every open page which questions are open now.
pub async fn publish_asks(shared: &Shared, live: &Live) {
    let open = shared.read().await.asks.open(now_s());
    let _ = live.publish("asks", &serde_json::json!({ "asks": open, "now": now_s() }));
}

/// Now, unix seconds (redesign-final: the one clock, `clock::now_s`, the
/// injected one in a demo run).
pub fn now_s() -> u64 {
    super::clock::now_s()
}

/// Runs until the process ends.
pub async fn run(
    target: HostTarget,
    cfg: LinkConfig,
    shared: Shared,
    live: Live,
    mut asks: HostAsks,
) {
    // fix-trend-restart: the day the last run saw, back before the first
    // snapshot lands.
    if let Some(db) = &cfg.trend_db {
        let now = now_s();
        let oldest_at = (now / crate::core::trend::STEP_S)
            .saturating_sub(crate::core::trend::POINTS as u64 - 1)
            * crate::core::trend::STEP_S;
        match db.read(move |c| trend_since(c, oldest_at as i64)).await {
            Ok(buckets) => {
                shared.write().await.trend = crate::core::trend::Trend::from_buckets(buckets, now);
            }
            Err(e) => {
                tracing::warn!(error = %e.message, "the saved fleet trend could not be read; starting an empty one")
            }
        }
    }
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
                        // by id, so its reads need not wait behind a deploy;
                        // feat-platform-10: `homelab ui` steps come here
                        // (never to a dev dashboard, fix-158).
                        for command in greeting(cfg, &version) {
                            let req = RpcRequest { id: next_id, command };
                            next_id += 1;
                            let text = serde_json::to_string(&req).map_err(|e| e.to_string())?;
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
                        if let ServerMsg::Ask { id, op, step, what, if_allowed, if_stopped, boot, wait_s } = ask.clone() {
                            // fix-240: the host's own wait when it says it;
                            // this dashboard's setting for an older host.
                            shared.write().await.asks.heard(
                                id, boot, op, step, what, if_allowed, if_stopped,
                                now_s(), wait_s.unwrap_or(cfg.ask_timeout_s),
                            );
                        }
                        publish_asks(shared, live).await;
                        let _ = asks.events.send(ask);
                    }
                    // fix-240: answered from another session, or its wait
                    // ran out: the banner goes in every tab.
                    ServerMsg::AskSettled { id, boot, .. } => {
                        shared.write().await.asks.forget(&boot, id);
                        publish_asks(shared, live).await;
                    }
                    ServerMsg::State(state) => {
                        let view = fleet_view(&state, now_s());
                        let changed = {
                            let s = shared.read().await;
                            s.fleet.as_ref().map(|f| (&f.stacks, &f.counts)) != Some((&view.stacks, &view.counts))
                        };
                        let started = {
                            let mut s = shared.write().await;
                            let before = s.trend.newest_slot();
                            s.trend.record(&view, now_s());
                            s.fleet = Some(view.clone());
                            s.home_address = state.host.home_address.clone();
                            (s.trend.newest_slot() != before).then(|| s.trend.finished().cloned()).flatten()
                        };
                        // fix-trend-restart: the bucket that just finished is
                        // stored once per bucket (every five minutes), so a
                        // restart loses at most the one being filled.
                        if let (Some(done), Some(db)) = (started, cfg.trend_db.clone()) {
                            tokio::spawn(async move {
                                if let Err(e) = db.write(move |tx| put_bucket(tx, &done)).await {
                                    tracing::warn!(error = %e.message, "the fleet trend could not be saved");
                                }
                            });
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
