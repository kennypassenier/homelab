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
//!
//! The last result is kept (Kenny, 2026-09-29, form "Trage pagina's": the
//! last result at once, refreshed when the page is opened; no timer). A page
//! that opens while a result exists gets it immediately, marked with when it
//! was read (`read_at`) and the run now reading again (`refreshing`); the
//! page then waits for that run as before. Every finished run is announced
//! on the live channel (`slow_read`), so every open page and tab can fetch
//! it by id without starting another.

use std::collections::VecDeque;
use std::future::Future;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Duration;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use tokio::sync::watch;

use super::actions::Publish;
use super::host_link::now_s;

/// fix-181: one process-wide "we are shutting down" flag, set the moment
/// chassis's own graceful-shutdown signal fires (SIGTERM or Ctrl-C) — the
/// same signal `chassis::shell::lifecycle` races the drain against
/// (tokio multiplexes a signal to every listener, so this one more
/// subscriber changes nothing about the kit's own handling of it). Lazily
/// started by the first `SlowRead` built, so a binary that never makes one
/// (no admin dashboard route uses it) spawns nothing extra.
///
/// Why this exists: a `SlowRead` answer can wait up to `WAIT` (20 s) for a
/// run to finish, which is longer than the shutdown bound
/// (`shutdown_timeout_ms`, 10 s by default). Without this, every request
/// parked in that wait when SIGTERM arrives sits there regardless, and
/// `chassis::shell::lifecycle::bounded` logs "shutdown exceeded its bound;
/// exiting 0 anyway (norm N1)" on every restart even though nothing is
/// actually wrong — it is simply waiting out a poll interval that outlives
/// the shutdown bound by design.
fn shutdown_flag() -> watch::Receiver<bool> {
    static CELL: OnceLock<watch::Receiver<bool>> = OnceLock::new();
    CELL.get_or_init(|| {
        let (tx, rx) = watch::channel(false);
        tokio::spawn(async move {
            chassis::shell::lifecycle::wait_for_stop_signal().await;
            let _ = tx.send(true);
        });
        rx
    })
    .clone()
}

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

#[derive(Clone)]
struct Finished {
    id: u64,
    status: StatusCode,
    body: serde_json::Value,
    /// When the run answered (unix seconds).
    read_at: u64,
}

impl Finished {
    /// The answer, with when it was read and by which run (`read_at`,
    /// `read_run`), so a page can tell a `slow_read` event for what it
    /// already shows from a newer one.
    fn body(&self) -> serde_json::Value {
        let mut b = self.body.clone();
        if let Some(o) = b.as_object_mut() {
            o.insert("read_at".into(), self.read_at.into());
            o.insert("read_run".into(), self.id.into());
        }
        b
    }
}

#[derive(Default)]
struct State {
    next: u64,
    /// The run on its way: its id and when it started (unix seconds).
    running: Option<(u64, u64)>,
    finished: VecDeque<Finished>,
    /// The newest run that answered 2xx: what a page opened later sees at
    /// once while the next run reads again.
    last: Option<Finished>,
}

/// One slow read (today, the fleet check, the doctor, and — since fix-177 —
/// one per stack of the backup calendar, so a stack name can be the key
/// too, not only a fixed string known at compile time).
pub struct SlowRead {
    what: String,
    /// The read's name on the live channel (`slow_read` events).
    key: String,
    publish: Option<Arc<dyn Publish>>,
    state: Mutex<State>,
    /// The id of the newest finished run.
    done: watch::Sender<u64>,
    /// fix-181: becomes `true` once SIGTERM/Ctrl-C arrives, so `read` can
    /// stop waiting at once instead of riding out `wait` regardless.
    shutdown: watch::Receiver<bool>,
}

impl SlowRead {
    pub fn new(what: impl Into<String>) -> Arc<Self> {
        let what = what.into();
        Self::build(what.clone(), what, None)
    }

    /// A read whose finished runs are announced on the live channel as
    /// `slow_read {read: key, run, ok}`.
    pub fn announced(
        what: impl Into<String>,
        key: impl Into<String>,
        publish: Arc<dyn Publish>,
    ) -> Arc<Self> {
        Self::build(what.into(), key.into(), Some(publish))
    }

    fn build(what: String, key: String, publish: Option<Arc<dyn Publish>>) -> Arc<Self> {
        Arc::new(SlowRead {
            what,
            key,
            publish,
            state: Mutex::new(State {
                next: 1,
                ..State::default()
            }),
            done: watch::Sender::new(0),
            shutdown: shutdown_flag(),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Answer one request. Without `run` the read starts, or the page joins
    /// the run already on its way; with it, the page asks after that run.
    /// Waits at most `wait`, then answers 202 `{running, run, started_at}`.
    ///
    /// Without `run`, when an earlier run answered: that answer at once,
    /// with `read_at` and `refreshing: {run, started_at}`, the run the page
    /// then asks after.
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
                        return (f.status, Json(f.body())).into_response();
                    }
                    match s.running {
                        Some((r, at)) if r == id => (r, at),
                        _ => return self.gone(id),
                    }
                }
                None => {
                    let (id, at) = match s.running {
                        Some(r) => r,
                        None => {
                            let id = s.next;
                            s.next += 1;
                            let at = now_s();
                            s.running = Some((id, at));
                            self.spawn(id, start());
                            (id, at)
                        }
                    };
                    if let Some(last) = &s.last {
                        let mut b = last.body();
                        if let Some(o) = b.as_object_mut() {
                            o.insert(
                                "refreshing".into(),
                                serde_json::json!({ "run": id, "started_at": at }),
                            );
                        }
                        return (last.status, Json(b)).into_response();
                    }
                    (id, at)
                }
            }
        };
        // The watch holds the newest finished id; runs finish in order.
        // fix-181: raced against the shutdown flag, not just the clock — a
        // request parked here when SIGTERM arrives answers at once instead
        // of riding out the rest of `wait` (up to 20 s, longer than the
        // 10 s shutdown bound) and forcing chassis to log a false "shutdown
        // exceeded its bound" on every restart.
        let mut shutdown = self.shutdown.clone();
        tokio::select! {
            _ = tokio::time::timeout(wait, rx.wait_for(|d| *d >= id)) => {}
            _ = shutdown.wait_for(|down| *down) => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({
                        "running": true,
                        "restarting": true,
                        "run": id,
                        "started_at": started,
                        "why": "the dashboard is restarting; read again shortly",
                    })),
                )
                    .into_response();
            }
        }
        let s = self.lock();
        if let Some(f) = s.finished.iter().find(|f| f.id == id) {
            return (f.status, Json(f.body())).into_response();
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
                        "what": &me.what,
                        "why": format!("the read stopped before it answered: {e}"),
                        "fix": "read again; if it stays, look at the dashboard's log",
                    }),
                )
            });
            let ok = status.is_success();
            {
                let mut s = me.lock();
                if s.running.is_some_and(|(r, _)| r == id) {
                    s.running = None;
                }
                let f = Finished {
                    id,
                    status,
                    body,
                    read_at: now_s(),
                };
                // A failed run is that run's answer; the last good result
                // stays what a page opened later sees first.
                if ok {
                    s.last = Some(f.clone());
                }
                s.finished.push_back(f);
                while s.finished.len() > KEEP {
                    s.finished.pop_front();
                }
            }
            me.done.send_replace(id);
            if let Some(p) = &me.publish {
                p.publish(
                    "slow_read",
                    serde_json::json!({ "read": &me.key, "run": id, "ok": ok }),
                );
            }
        });
    }

    fn gone(&self, id: u64) -> Response {
        (
            StatusCode::GONE,
            Json(serde_json::json!({
                "what": &self.what,
                "why": format!("read {id} is no longer on this dashboard (it restarted, or newer reads replaced it)"),
                "fix": "read again",
            })),
        )
            .into_response()
    }

    /// fix-181 test seam: a `SlowRead` wired to a shutdown flag the test
    /// drives itself, instead of `shutdown_flag()`'s real SIGTERM/Ctrl-C
    /// listener — sending an actual signal would hit the whole test binary,
    /// not one test.
    #[cfg(test)]
    fn with_shutdown(what: impl Into<String>, shutdown: watch::Receiver<bool>) -> Arc<Self> {
        let what = what.into();
        Arc::new(SlowRead {
            key: what.clone(),
            what,
            publish: None,
            state: Mutex::new(State {
                next: 1,
                ..State::default()
            }),
            done: watch::Sender::new(0),
            shutdown,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// fix-181: `homelab-admin` logged "shutdown exceeded its bound;
    /// exiting 0 anyway (norm N1) timeout_ms=10000 still_open=\"in-flight
    /// requests\"" on every restart, because a request parked in
    /// `SlowRead::read`'s wait (up to 20 s) outlives the 10 s shutdown
    /// bound regardless of the signal. The wait must end at once once
    /// shutdown starts, not ride out the rest of `wait`.
    #[tokio::test]
    async fn fix_181_c_shutdown_ends_the_wait_before_the_poll_interval_does() {
        let (tx, rx) = watch::channel(false);
        let read = SlowRead::with_shutdown("test", rx);

        // A run that would not finish on its own inside this test, so the
        // only way `read` returns promptly is the shutdown branch of the
        // `select!`, not the run finishing or `wait` elapsing.
        let fut = read.read(None, Duration::from_secs(20), || async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            (StatusCode::OK, serde_json::json!({}))
        });
        tokio::pin!(fut);

        // Let the request actually reach the wait before shutdown starts.
        tokio::time::sleep(Duration::from_millis(5)).await;
        tx.send(true).unwrap();

        let resp = tokio::time::timeout(Duration::from_millis(500), fut)
            .await
            .expect(
                "the request did not return promptly after shutdown — it is riding out `wait` \
                 instead, which is exactly the false N1 alarm",
            );
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Shutdown already under way before the request even starts waiting
    /// (the common case: SIGTERM arrived, then a long-poll client's next
    /// request lands) must not wait at all either.
    #[tokio::test]
    async fn fix_181_c_shutdown_already_true_answers_at_once() {
        let (tx, rx) = watch::channel(false);
        tx.send(true).unwrap();
        let read = SlowRead::with_shutdown("test", rx);

        let fut = read.read(None, Duration::from_secs(20), || async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            (StatusCode::OK, serde_json::json!({}))
        });
        let resp = tokio::time::timeout(Duration::from_millis(200), fut)
            .await
            .expect("a request starting after shutdown already began must not wait either");
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// The ordinary path — no shutdown — must still behave as before:
    /// answers once the run finishes, well inside `wait`.
    #[tokio::test]
    async fn shutdown_wiring_does_not_change_the_ordinary_path() {
        let (_tx, rx) = watch::channel(false);
        let read = SlowRead::with_shutdown("test", rx);

        let resp = read
            .read(None, Duration::from_secs(1), || async {
                (StatusCode::OK, serde_json::json!({"ok": true}))
            })
            .await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
