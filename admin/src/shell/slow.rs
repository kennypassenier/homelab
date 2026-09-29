//! A read of the host that takes longer than one request may last (Kenny,
//! 2026-09-29: "Today seems to load, but nothing loads").
//!
//! `homelab today` and `homelab check` take about 90 s on pve (measured
//! 2026-09-29: 93 s and 92 s), the doctor about 30 s. A request is cut at
//! 30 s by the chassis request guard and at 100 s by Cloudflare in front of
//! admin.kp-soft.dev, so one long request either fails or lives on the
//! edge of failing. Instead a read is started once, runs to its end on the
//! server, and the page asks again with the run's id until the answer is
//! there: each request waits at most `wait` (well under both limits) and
//! then says "still running" with 202.
//!
//! One run at a time per read: a second page, or a reload, joins the run
//! that is already on its way instead of asking the host again.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use tokio::sync::watch;

use super::host_link::now_s;

/// How long one request waits for a run before it answers "still running":
/// under the chassis request timeout (30 s) and every proxy's.
pub const WAIT: Duration = Duration::from_secs(20);

/// Finished runs kept for a page that asks after another run started.
const KEEP: usize = 4;

/// `?run=<id>`: the run this page started or joined.
#[derive(Debug, Deserialize, Default)]
pub struct RunQuery {
    pub run: Option<u64>,
}

struct Finished {
    id: u64,
    status: StatusCode,
    body: serde_json::Value,
}

#[derive(Default)]
struct State {
    next: u64,
    /// The run on its way: its id and when it started (unix seconds).
    running: Option<(u64, u64)>,
    finished: VecDeque<Finished>,
}

/// One slow read (today, the fleet check, the doctor).
pub struct SlowRead {
    what: &'static str,
    state: Mutex<State>,
    /// The id of the newest finished run.
    done: watch::Sender<u64>,
}

impl SlowRead {
    pub fn new(what: &'static str) -> Arc<Self> {
        Arc::new(SlowRead {
            what,
            state: Mutex::new(State {
                next: 1,
                ..State::default()
            }),
            done: watch::Sender::new(0),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answer one request. Without `run` the read starts, or the page joins
    /// the run already on its way; with it, the page asks after that run.
    /// Waits at most `wait`, then answers 202 `{running, run, started_at}`.
    pub async fn read<F, Fut>(
        self: &Arc<Self>,
        run: Option<u64>,
        wait: Duration,
        start: F,
    ) -> Response
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = (StatusCode, serde_json::Value)> + Send + 'static,
    {
        let mut rx = self.done.subscribe();
        let (id, started) = {
            let mut s = self.lock();
            match run {
                Some(id) => {
                    if let Some(f) = s.finished.iter().find(|f| f.id == id) {
                        return (f.status, Json(f.body.clone())).into_response();
                    }
                    match s.running {
                        Some((r, at)) if r == id => (r, at),
                        _ => return self.gone(id),
                    }
                }
                None => match s.running {
                    Some(r) => r,
                    None => {
                        let id = s.next;
                        s.next += 1;
                        let at = now_s();
                        s.running = Some((id, at));
                        self.spawn(id, start());
                        (id, at)
                    }
                },
            }
        };
        // The watch holds the newest finished id; runs finish in order.
        let _ = tokio::time::timeout(wait, rx.wait_for(|d| *d >= id)).await;
        let s = self.lock();
        if let Some(f) = s.finished.iter().find(|f| f.id == id) {
            return (f.status, Json(f.body.clone())).into_response();
        }
        (
            StatusCode::ACCEPTED,
            Json(serde_json::json!({
                "running": true,
                "run": id,
                "started_at": started,
            })),
        )
            .into_response()
    }

    fn spawn<Fut>(self: &Arc<Self>, id: u64, work: Fut)
    where
        Fut: Future<Output = (StatusCode, serde_json::Value)> + Send + 'static,
    {
        let me = self.clone();
        tokio::spawn(async move {
            // Its own task, so a panic in the read is an answer, not a run
            // that never ends.
            let (status, body) = tokio::spawn(work).await.unwrap_or_else(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    serde_json::json!({
                        "what": me.what,
                        "why": format!("the read stopped before it answered: {e}"),
                        "fix": "read again; if it stays, look at the dashboard's log",
                    }),
                )
            });
            {
                let mut s = me.lock();
                if s.running.is_some_and(|(r, _)| r == id) {
                    s.running = None;
                }
                s.finished.push_back(Finished { id, status, body });
                while s.finished.len() > KEEP {
                    s.finished.pop_front();
                }
            }
            me.done.send_replace(id);
        });
    }

    fn gone(&self, id: u64) -> Response {
        (
            StatusCode::GONE,
            Json(serde_json::json!({
                "what": self.what,
                "why": format!("read {id} is no longer on this dashboard (it restarted, or newer reads replaced it)"),
                "fix": "read again",
            })),
        )
            .into_response()
    }
}
