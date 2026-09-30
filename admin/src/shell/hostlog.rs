//! TUI parity (LOG_STREAM and DATA_TRANSFERS): every line the host prints
//! for every operation, whoever started it (the CLI, the TUI, the nightly
//! round, this dashboard), and the live byte counters of its transfers.
//!
//! The host sends every session each `Log` and `Transfer` (the broadcast
//! sink); the link task hands them on (`HostClient::subscribe`). This keeps
//! the newest lines in a ring for a page that opens later, masks secrets by
//! shape before anything leaves the server (arch-secrets-read), and pushes
//! each line (`host_log`) and each counter (`transfer`, at most a few times
//! a second per transfer) to every open page. A page that opens while an
//! operation runs catches up from the ring, which is seeded from the
//! host's own `CurrentOp` at start.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use homelab_proto::{Command, LogLevel, ServerMsg, StepMark};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast::error::RecvError;

use super::actions::{Clock, HostPort, Publish};

/// Lines kept for a page that opens later.
pub const RING: usize = 2000;
/// A transfer's counter is pushed at most this often (the last one always).
const TRANSFER_EVERY_MS: i64 = 250;
/// A finished or silent transfer is dropped after this long.
const TRANSFER_KEEP_S: i64 = 30;

/// One host line as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineView {
    /// Counts up from 1 per dashboard start; a page asks for what came
    /// after the last one it has.
    pub seq: u64,
    pub ts: i64,
    pub level: LogLevel,
    /// The stack the line is about, or HOST, NIGHT, …; the page filters on
    /// it as the TUI's source selector does.
    pub source: String,
    pub msg: String,
    /// The request of the session that asked; None for the host's own work.
    pub req: Option<u64>,
    /// The token of the session that asked ("admin", "wsl"); None for the
    /// nightly round and other work nobody asked for over the line.
    pub by: Option<String>,
    pub step: Option<StepMark>,
}

/// One transfer's counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferView {
    pub op: String,
    pub label: String,
    pub done: u64,
    pub total: Option<u64>,
    /// Unix seconds of the last counter.
    pub at: i64,
    #[serde(skip)]
    pushed_ms: i64,
}

struct Ring {
    next: u64,
    lines: VecDeque<LineView>,
    transfers: BTreeMap<(String, String), TransferView>,
    /// Decision "23 constants": [`RING`] by default,
    /// `HOMELAB_ADMIN_HOSTLOG_RING` in `mount()`.
    cap: usize,
}

impl Default for Ring {
    fn default() -> Self {
        Ring {
            next: 0,
            lines: VecDeque::new(),
            transfers: BTreeMap::new(),
            cap: RING,
        }
    }
}

impl Ring {
    fn push(&mut self, mut line: LineView) -> LineView {
        self.next += 1;
        line.seq = self.next;
        self.lines.push_back(line.clone());
        while self.lines.len() > self.cap {
            self.lines.pop_front();
        }
        line
    }
}

/// The ring and its pushes.
#[derive(Clone)]
pub struct HostLog {
    ring: Arc<Mutex<Ring>>,
    publish: Arc<dyn Publish>,
    clock: Clock,
}

/// A host `Log` as a line, the message masked.
pub fn line_of(m: &ServerMsg, now: i64) -> Option<LineView> {
    let ServerMsg::Log {
        level,
        source,
        msg,
        req,
        ts,
        step,
        by,
    } = m
    else {
        return None;
    };
    Some(LineView {
        seq: 0,
        ts: ts.map(|t| t as i64).unwrap_or(now),
        level: *level,
        source: source.clone(),
        msg: homelab_core::executor::mask_secrets(msg),
        req: *req,
        by: by.clone(),
        step: step.clone(),
    })
}

#[derive(Debug, Deserialize)]
pub struct After {
    #[serde(default)]
    pub after: u64,
    /// Only this source (a stack's name, HOST, …); empty: every line.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default = "limit")]
    pub limit: usize,
}
fn limit() -> usize {
    RING
}

impl HostLog {
    pub fn new(publish: Arc<dyn Publish>, clock: Clock) -> Self {
        Self::with_ring(publish, clock, RING)
    }

    /// Decision "23 constants": the ring size from `ActConfig`
    /// (`HOMELAB_ADMIN_HOSTLOG_RING`); `mount()` only.
    pub fn with_ring(publish: Arc<dyn Publish>, clock: Clock, ring: usize) -> Self {
        HostLog {
            ring: Arc::new(Mutex::new(Ring {
                cap: ring,
                ..Ring::default()
            })),
            publish,
            clock,
        }
    }

    fn cap(&self) -> usize {
        self.lock().cap
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Ring> {
        self.ring.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Take one message of the line.
    pub fn take(&self, m: &ServerMsg) {
        let now = (self.clock)();
        if let Some(line) = line_of(m, now) {
            let line = self.lock().push(line);
            self.publish
                .publish("host_log", serde_json::to_value(&line).unwrap_or_default());
            return;
        }
        if let ServerMsg::Transfer {
            op,
            label,
            done,
            total,
        } = m
        {
            let push = {
                let mut r = self.lock();
                r.transfers
                    .retain(|_, t| now - t.at < TRANSFER_KEEP_S || t.total != Some(t.done));
                let key = (op.clone(), label.clone());
                let finished = total.is_some_and(|t| *done >= t);
                let t = r.transfers.entry(key).or_insert(TransferView {
                    op: op.clone(),
                    label: label.clone(),
                    done: 0,
                    total: None,
                    at: now,
                    pushed_ms: i64::MIN,
                });
                t.done = *done;
                t.total = *total;
                t.at = now;
                // The clock counts seconds; the throttle needs finer steps.
                let ms = monotonic_ms();
                if finished || ms.saturating_sub(t.pushed_ms) >= TRANSFER_EVERY_MS {
                    t.pushed_ms = ms;
                    Some(t.clone())
                } else {
                    None
                }
            };
            if let Some(t) = push {
                self.publish
                    .publish("transfer", serde_json::to_value(&t).unwrap_or_default());
            }
        }
    }

    /// Lines after `after` (optionally one source), oldest first.
    pub fn lines(&self, q: &After) -> Vec<LineView> {
        let r = self.lock();
        let mut out: Vec<LineView> = r
            .lines
            .iter()
            .filter(|l| l.seq > q.after)
            .filter(|l| {
                q.source
                    .as_deref()
                    .is_none_or(|s| s.is_empty() || l.source == s)
            })
            .cloned()
            .collect();
        let n = q.limit.clamp(1, r.cap);
        if out.len() > n {
            out.drain(..out.len() - n);
        }
        out
    }

    /// Every source seen, for the filter.
    pub fn sources(&self) -> Vec<String> {
        let r = self.lock();
        let mut s: Vec<String> = r.lines.iter().map(|l| l.source.clone()).collect();
        s.sort();
        s.dedup();
        s
    }

    pub fn transfers(&self) -> Vec<TransferView> {
        let now = (self.clock)();
        self.lock()
            .transfers
            .values()
            .filter(|t| now - t.at < TRANSFER_KEEP_S)
            .cloned()
            .collect()
    }

    /// Seed from the host's `CurrentOp` (the lines of the operation running
    /// now), skipping what the ring already has.
    pub fn seed(&self, view: &homelab_proto::CurrentOpView) {
        let now = (self.clock)();
        let mut r = self.lock();
        for m in &view.lines {
            if let Some(line) = line_of(m, now) {
                let dup = r
                    .lines
                    .iter()
                    .any(|l| l.ts == line.ts && l.msg == line.msg && l.source == line.source);
                if !dup {
                    r.push(line);
                }
            }
        }
    }

    /// Follow the line for as long as the process runs.
    pub fn spawn(&self, host: Arc<dyn HostPort>) {
        let me = self.clone();
        let mut rx = host.subscribe();
        tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(m) => me.take(&m),
                    Err(RecvError::Lagged(n)) => {
                        let now = (me.clock)();
                        let line = me.lock().push(LineView {
                            seq: 0,
                            ts: now,
                            level: LogLevel::Warn,
                            source: "ADMIN".into(),
                            msg: format!(
                                "{n} host line(s) were missed here (the dashboard fell behind); \
                                 the host's journal has them all"
                            ),
                            req: None,
                            by: None,
                            step: None,
                        });
                        me.publish
                            .publish("host_log", serde_json::to_value(&line).unwrap_or_default());
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        });
        // Catch up with an operation already running when the dashboard
        // started (the host keeps its lines in a ring, feat-platform-3).
        let me = self.clone();
        tokio::spawn(async move {
            for _ in 0..30 {
                let r = host
                    .ask_traced(Command::CurrentOp, Duration::from_secs(20), None)
                    .await;
                if let Ok(r) = r {
                    if let Ok(v) = serde_json::from_str::<homelab_proto::CurrentOpView>(&r.message)
                    {
                        me.seed(&v);
                    }
                    return;
                }
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        });
    }
}

fn monotonic_ms() -> i64 {
    use std::sync::OnceLock;
    static START: OnceLock<std::time::Instant> = OnceLock::new();
    let start = *START.get_or_init(std::time::Instant::now);
    start.elapsed().as_millis() as i64
}

async fn lines(State(h): State<HostLog>, Query(q): Query<After>) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "lines": h.lines(&q),
        "sources": h.sources(),
        "transfers": h.transfers(),
        "ring": h.cap(),
    }))
}

/// Mounted with `dashboard_routes`: behind the login and both locks.
pub fn router(h: HostLog) -> Router {
    Router::new()
        .route("/data/host-log", get(lines))
        .with_state(h)
}
