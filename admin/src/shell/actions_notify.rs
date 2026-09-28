//! feat-overview-5, feat-ops-8, feat-ops-9: the notification center's shell.
//! The list lives in `notifications.json` under the data dir (arch-state);
//! every notice is stored, published to open pages as the SSE event
//! `notification`, and pushed through kyu when `core::notify::route` says so.
//! The push goes through [`Pusher`], so tests use a double and never reach
//! kyu.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::actions::{Clock, Publish};
use super::actions_state::{read_json, write_json, StateError};
use crate::core::notify::{self, Draft, Notice, NotifyFile, PushOutcome, Settings};

/// Where a push goes.
pub trait Pusher: Send + Sync + 'static {
    fn push(
        &self,
        payload: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>>;
}

/// feat-ops-8: kyu's publish endpoint, the way the host notifies (a JSON
/// POST with a bearer token; only 2xx counts as delivered, G16).
pub struct KyuPusher {
    url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl KyuPusher {
    pub fn new(url: String, token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        KyuPusher { url, token, client }
    }
}

impl Pusher for KyuPusher {
    fn push(
        &self,
        payload: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            let mut req = self
                .client
                .post(&self.url)
                .header("content-type", "application/json")
                .body(payload);
            if let Some(t) = &self.token {
                req = req.bearer_auth(t);
            }
            // fix-123: a route in a message shows scheme and host only.
            let shown = homelab_core::notify::route_for_log(&self.url);
            match req.send().await {
                Ok(r) if r.status().is_success() => Ok(()),
                Ok(r) => Err(format!("{shown} answered HTTP {}", r.status().as_u16())),
                Err(e) => Err(format!(
                    "{shown} did not answer ({})",
                    if e.is_timeout() {
                        "timeout"
                    } else {
                        "connection"
                    }
                )),
            }
        })
    }
}

/// No push route configured: every push is skipped with that reason.
pub struct NoPusher;

impl Pusher for NoPusher {
    fn push(
        &self,
        _payload: String,
    ) -> Pin<Box<dyn Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async { Err("no push route: HOMELAB_ADMIN_NOTIFY_URL is not set".to_string()) })
    }
}

pub struct NotifyCenter {
    path: PathBuf,
    state: Mutex<NotifyFile>,
    pusher: Arc<dyn Pusher>,
    publish: Arc<dyn Publish>,
    clock: Clock,
}

impl NotifyCenter {
    /// The center from its file; a file this build cannot read is an error
    /// (arch-state: never silently replaced).
    pub fn load(
        path: PathBuf,
        pusher: Arc<dyn Pusher>,
        publish: Arc<dyn Publish>,
        clock: Clock,
    ) -> Result<Arc<Self>, StateError> {
        let state: NotifyFile = read_json(&path)?.unwrap_or_default();
        state.check().map_err(|why| StateError::Parse {
            path: path.display().to_string(),
            why,
        })?;
        Ok(Arc::new(NotifyCenter {
            path,
            state: Mutex::new(state),
            pusher,
            publish,
            clock,
        }))
    }

    fn save(&self, s: &NotifyFile) {
        if let Err(e) = write_json(&self.path, s) {
            tracing::warn!(error = %e, "notifications not saved");
        }
    }

    /// Store, push when the rules say so, and tell the open pages.
    pub async fn notify(&self, draft: Draft) -> Notice {
        let now = (self.clock)();
        let settings = self.state.lock().await.settings.clone();
        let route = notify::route(&settings, &draft, now);
        let push = match route.push {
            Ok(()) => {
                let payload = notify::push_payload(&draft, env!("CARGO_PKG_VERSION"));
                match self.pusher.push(payload).await {
                    Ok(()) => PushOutcome::Sent,
                    Err(why) => PushOutcome::Failed { why },
                }
            }
            Err(why) => PushOutcome::Skipped { why },
        };
        let (notice, unread) = {
            let mut s = self.state.lock().await;
            let n = s.add(draft, now, push);
            self.save(&s);
            (n, s.unread())
        };
        self.publish.publish(
            "notification",
            serde_json::json!({ "notice": notice, "pop_up": route.pop_up, "unread": unread }),
        );
        notice
    }

    pub async fn snapshot(&self) -> serde_json::Value {
        let s = self.state.lock().await;
        let now = (self.clock)();
        serde_json::json!({
            "notices": s.notices.iter().rev().collect::<Vec<_>>(),
            "unread": s.unread(),
            "unread_by_stack": notify::unread_by_stack(&s),
            "settings": s.settings,
            "snoozed": s.settings.snoozed(now),
        })
    }

    pub async fn mark(&self, ids: Option<Vec<u64>>, read: bool) -> usize {
        let mut s = self.state.lock().await;
        let n = s.mark(ids.as_deref(), read);
        if n > 0 {
            self.save(&s);
        }
        let unread = s.unread();
        drop(s);
        self.publish.publish(
            "notifications_read",
            serde_json::json!({ "changed": n, "unread": unread }),
        );
        n
    }

    pub async fn settings(&self) -> Settings {
        self.state.lock().await.settings.clone()
    }

    pub async fn set_settings(&self, new: Settings) -> Result<Settings, String> {
        new.validate()?;
        let mut s = self.state.lock().await;
        s.settings = new;
        self.save(&s);
        let out = s.settings.clone();
        drop(s);
        self.publish
            .publish("notify_settings", serde_json::json!({ "settings": out }));
        Ok(out)
    }

    pub async fn set_stack_muted(&self, stack: &str, muted: bool) -> Settings {
        let mut s = self.state.lock().await;
        notify::set_stack_muted(&mut s.settings, stack, muted);
        self.save(&s);
        let out = s.settings.clone();
        drop(s);
        self.publish
            .publish("notify_settings", serde_json::json!({ "settings": out }));
        out
    }

    pub async fn snooze(&self, seconds: i64) -> Result<Option<i64>, String> {
        let now = (self.clock)();
        let mut s = self.state.lock().await;
        let until = s.snooze(now, seconds)?;
        self.save(&s);
        let settings = s.settings.clone();
        drop(s);
        self.publish.publish(
            "notify_settings",
            serde_json::json!({ "settings": settings }),
        );
        Ok(until)
    }

    /// The host's incident list, as read; a notice per bundle not seen
    /// before (the first list only seeds the memory).
    pub async fn incidents(&self, listed: Vec<String>) -> usize {
        let fresh = {
            let mut s = self.state.lock().await;
            let fresh = s.new_incidents(&listed);
            self.save(&s);
            fresh
        };
        let n = fresh.len();
        for name in fresh {
            self.notify(notify::incident_draft(&name)).await;
        }
        n
    }
}

// ── routes ──────────────────────────────────────────────────────────────

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::core::actions::Refusal;

fn bad(what: &str, why: impl Into<String>, fix: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(Refusal::new(what, why, fix))).into_response()
}

fn read_body<T>(b: Result<Json<T>, JsonRejection>, what: &str) -> Result<T, Refusal> {
    b.map(|Json(t)| t)
        .map_err(|e| Refusal::new(what, e.body_text(), "send the JSON this route documents"))
}

async fn list(State(c): State<Arc<NotifyCenter>>) -> Json<serde_json::Value> {
    Json(c.snapshot().await)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkBody {
    /// Absent: every notice.
    #[serde(default)]
    ids: Option<Vec<u64>>,
    #[serde(default = "yes")]
    read: bool,
}
fn yes() -> bool {
    true
}

async fn mark(
    State(c): State<Arc<NotifyCenter>>,
    b: Result<Json<MarkBody>, JsonRejection>,
) -> Response {
    let b = match read_body(b, "mark notifications") {
        Ok(b) => b,
        Err(r) => return (StatusCode::BAD_REQUEST, Json(r)).into_response(),
    };
    let changed = c.mark(b.ids, b.read).await;
    Json(serde_json::json!({ "changed": changed })).into_response()
}

async fn get_settings(State(c): State<Arc<NotifyCenter>>) -> Json<Settings> {
    Json(c.settings().await)
}

async fn put_settings(
    State(c): State<Arc<NotifyCenter>>,
    b: Result<Json<Settings>, JsonRejection>,
) -> Response {
    let s = match read_body(b, "notification settings") {
        Ok(s) => s,
        Err(r) => return (StatusCode::BAD_REQUEST, Json(r)).into_response(),
    };
    match c.set_settings(s).await {
        Ok(s) => Json(s).into_response(),
        Err(why) => bad("notification settings", why, "fix the value and save again"),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StackBody {
    muted: bool,
}

async fn put_stack(
    State(c): State<Arc<NotifyCenter>>,
    UrlPath(stack): UrlPath<String>,
    b: Result<Json<StackBody>, JsonRejection>,
) -> Response {
    let b = match read_body(b, "a stack's notifications") {
        Ok(b) => b,
        Err(r) => return (StatusCode::BAD_REQUEST, Json(r)).into_response(),
    };
    if !crate::core::actions::valid_stack_name(&stack) {
        return bad(
            "a stack's notifications",
            "not a stack name",
            "use the name the fleet page shows",
        );
    }
    Json(c.set_stack_muted(&stack, b.muted).await).into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnoozeBody {
    /// 0 ends the snooze.
    minutes: i64,
}

async fn snooze(
    State(c): State<Arc<NotifyCenter>>,
    b: Result<Json<SnoozeBody>, JsonRejection>,
) -> Response {
    let b = match read_body(b, "snooze") {
        Ok(b) => b,
        Err(r) => return (StatusCode::BAD_REQUEST, Json(r)).into_response(),
    };
    match c.snooze(b.minutes.saturating_mul(60)).await {
        Ok(until) => Json(serde_json::json!({ "snooze_until": until })).into_response(),
        Err(why) => bad("snooze", why, "pick 0 to 10080 minutes"),
    }
}

/// Mounted with `dashboard_routes`.
pub fn router(center: Arc<NotifyCenter>) -> Router {
    Router::new()
        .route("/data/notifications", get(list))
        .route("/data/notifications/read", post(mark))
        .route(
            "/data/notifications/settings",
            get(get_settings).put(put_settings),
        )
        .route("/data/notifications/stacks/{stack}", put(put_stack))
        .route("/data/notifications/snooze", post(snooze))
        .with_state(center)
}

/// feat-overview-5: read the host's incident list every `every` and turn
/// each new bundle into a notice.
pub fn spawn_incident_poll(
    host: Arc<dyn super::actions::HostPort>,
    center: Arc<NotifyCenter>,
    every: Duration,
) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(every);
        loop {
            t.tick().await;
            let reply = host
                .ask_traced(
                    homelab_proto::Command::Incidents { json: true },
                    Duration::from_secs(30),
                    None,
                )
                .await;
            let listed: Option<Vec<String>> = reply
                .ok()
                .filter(|r| r.ok)
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.message).ok())
                .and_then(|v| serde_json::from_value(v.get("incidents")?.clone()).ok());
            if let Some(listed) = listed {
                center.incidents(listed).await;
            }
        }
    });
}
