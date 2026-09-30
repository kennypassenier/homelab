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
    /// The dashboard's public address, for a push's `click_url`.
    base_url: std::sync::RwLock<String>,
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
            base_url: std::sync::RwLock::new(
                homelab_core::notify::DEFAULT_DASHBOARD_URL.to_string(),
            ),
        }))
    }

    /// Where a push's link points (`HOMELAB_ADMIN_PUBLIC_URL`).
    pub fn set_base_url(&self, url: &str) {
        if let Ok(mut b) = self.base_url.write() {
            *b = url.trim_end_matches('/').to_string();
        }
    }

    fn base(&self) -> String {
        self.base_url
            .read()
            .map(|b| b.clone())
            .unwrap_or_else(|_| homelab_core::notify::DEFAULT_DASHBOARD_URL.into())
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
                let payload = notify::push_payload(&draft, env!("CARGO_PKG_VERSION"), &self.base());
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

    /// Tell the open pages about a stored notice.
    fn announce(&self, notice: &Notice, unread: usize, now: i64) {
        let pop_up = !self
            .state
            .try_lock()
            .map(|s| s.settings.snoozed(now))
            .unwrap_or(false)
            && matches!(
                notice.level,
                notify::Level::Critical | notify::Level::Warning
            )
            && !notice.read;
        self.publish.publish(
            "notification",
            serde_json::json!({ "notice": notice, "pop_up": pop_up, "unread": unread }),
        );
    }

    /// Decision notify-routing: the host's notices after the cursor. The
    /// first read only moves the cursor (what the host kept before is
    /// history, not news). `job_of` names the dashboard job that sent a
    /// request, so its notice takes the host's words instead of a second
    /// one being added. How many notices changed.
    pub async fn host_notices(
        &self,
        notices: Vec<homelab_core::notify::HostNotice>,
        last_seq: u64,
        job_of: &(dyn Fn(u64) -> Option<u64> + Send + Sync),
    ) -> usize {
        let now = (self.clock)();
        let mut out = Vec::new();
        {
            let mut s = self.state.lock().await;
            if !s.host_seeded {
                s.host_seeded = true;
                s.host_cursor = last_seq;
                self.save(&s);
                return 0;
            }
            for n in &notices {
                if n.seq <= s.host_cursor {
                    continue;
                }
                let job = n.req.and_then(job_of);
                if let Some(x) = s.import_host(n, job, now) {
                    out.push(x);
                }
            }
            s.host_cursor = s
                .host_cursor
                .max(last_seq.min(notices.iter().map(|n| n.seq).max().unwrap_or(s.host_cursor)));
            self.save(&s);
        }
        let unread = self.state.lock().await.unread();
        for n in &out {
            self.announce(n, unread, now);
        }
        out.len()
    }

    /// Alertmanager's webhook: one notice per alert (a repeat of one still
    /// firing adds nothing). How many were stored.
    pub async fn alerts(&self, body: &serde_json::Value) -> usize {
        let now = (self.clock)();
        let mut out = Vec::new();
        {
            let mut s = self.state.lock().await;
            for a in notify::alert_drafts(body) {
                if let Some(n) = s.add_alert(a, now) {
                    out.push(n);
                }
            }
            if !out.is_empty() {
                self.save(&s);
            }
        }
        let unread = self.state.lock().await.unread();
        for n in &out {
            self.announce(n, unread, now);
        }
        out.len()
    }

    /// Decision daily-digest: once a day at the set time, when something
    /// waits, one push with the worst first and a link to this list.
    /// `today` reads the open Today items (a slow read, only asked when the
    /// digest is due). Nothing is pushed when all is clear, snoozed or off.
    pub async fn digest_tick<F, Fut>(&self, today: F) -> Option<notify::DigestRecord>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Vec<notify::DigestLine>, String>>,
    {
        let now = (self.clock)();
        let (day, settings, notices) = {
            let s = self.state.lock().await;
            let day = notify::digest_due(&s.settings, s.last_digest.as_ref(), now)?;
            (day, s.settings.clone(), s.notices.clone())
        };
        let lines = match today().await {
            Ok(l) => l,
            Err(why) => {
                tracing::warn!(why = %why, "the digest reads no Today items");
                vec![notify::DigestLine {
                    level: notify::Level::Warning,
                    text: format!("Today could not be read: {why}"),
                }]
            }
        };
        let d = notify::digest(&notices, &lines);
        let count = d.as_ref().map(|d| d.count).unwrap_or(0);
        let push = match &d {
            None => PushOutcome::Skipped {
                why: "all clear: nothing waits".into(),
            },
            Some(_) if settings.snoozed(now) => PushOutcome::Skipped {
                why: "snoozed".into(),
            },
            Some(_) if !settings.push => PushOutcome::Skipped {
                why: "pushes are off".into(),
            },
            Some(d) => {
                let payload = notify::digest_payload(d, env!("CARGO_PKG_VERSION"), &self.base());
                match self.pusher.push(payload).await {
                    Ok(()) => PushOutcome::Sent,
                    Err(why) => PushOutcome::Failed { why },
                }
            }
        };
        let rec = notify::DigestRecord {
            day,
            at: now,
            count,
            push,
        };
        let mut s = self.state.lock().await;
        s.last_digest = Some(rec.clone());
        self.save(&s);
        drop(s);
        self.publish
            .publish("notify_digest", serde_json::json!({ "digest": rec }));
        Some(rec)
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
            "last_digest": s.last_digest,
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
            // Decision notify-routing: once the host's own notices arrive,
            // each failure comes with its bundle named in it; a second
            // notice per bundle would say the same thing twice.
            if s.host_seeded {
                Vec::new()
            } else {
                fresh
            }
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

// ── Alertmanager's webhook (decision notify-routing) ────────────────────

/// What the hook needs, filled once the notification centre exists. The
/// route is mounted before it (chassis takes its public routes first), so a
/// call that comes earlier is told to retry.
#[derive(Clone, Default)]
pub struct HookSlot(Arc<std::sync::OnceLock<(Arc<NotifyCenter>, Option<String>)>>);

impl HookSlot {
    /// The centre and the bearer token Alertmanager must send
    /// (`HOMELAB_ADMIN_ALERTS_TOKEN`); None refuses every call.
    pub fn fill(&self, center: Arc<NotifyCenter>, token: Option<String>) {
        let _ = self.0.set((center, token));
    }
}

/// The path Alertmanager posts to (stacks/metrics/alertmanager).
pub const ALERTS_HOOK: &str = "/hooks/alertmanager";

async fn alertmanager_hook(
    State(slot): State<HookSlot>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let Some((center, token)) = slot.0.get() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Refusal::new(
                "alerts",
                "the notification centre is not running yet",
                "Alertmanager retries on its own",
            )),
        )
            .into_response();
    };
    let Some(token) = token.as_deref().filter(|t| !t.is_empty()) else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(Refusal::new(
                "alerts",
                "no token is configured for this hook",
                "set HOMELAB_ADMIN_ALERTS_TOKEN in admin.env and the same value in the alert sender's credentials file, both through the stacks' latch_files",
            )),
        )
            .into_response();
    };
    let sent = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    if !chassis::core::crypto::ct_eq(sent.as_bytes(), token.as_bytes()) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(Refusal::new(
                "alerts",
                "missing or wrong bearer token",
                "send Authorization: Bearer <HOMELAB_ADMIN_ALERTS_TOKEN>",
            )),
        )
            .into_response();
    }
    let value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            return bad(
                "alerts",
                format!("not JSON: {e}"),
                "send Alertmanager's webhook body",
            )
        }
    };
    let stored = center.alerts(&value).await;
    Json(serde_json::json!({ "stored": stored })).into_response()
}

/// The hook's public router: outside the dashboard's login and its locks
/// (main exempts the path from the request guards); its own bearer token
/// decides.
pub fn hooks_router(slot: HookSlot) -> Router {
    Router::new()
        .route(ALERTS_HOOK, post(alertmanager_hook))
        .with_state(slot)
}

/// Decision notify-routing: read the host's notices every `every` and add
/// them to the centre. A host older than `Command::Notices` does not answer
/// it; the read then waits a while before it asks again.
pub fn spawn_host_notice_poll(
    host: Arc<dyn super::actions::HostPort>,
    center: Arc<NotifyCenter>,
    job_of: Arc<dyn Fn(u64) -> Option<u64> + Send + Sync>,
    every: Duration,
) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(every);
        let mut rest = 0u32;
        loop {
            t.tick().await;
            if rest > 0 {
                rest -= 1;
                continue;
            }
            let after = center.state.lock().await.host_cursor;
            let reply = host
                .ask_traced(
                    homelab_proto::Command::Notices { after, limit: 200 },
                    Duration::from_secs(30),
                    None,
                )
                .await;
            let parsed = reply
                .ok()
                .filter(|r| r.ok)
                .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.message).ok());
            let Some(v) = parsed else {
                // An older host, or the line is down: ask again in ten rounds.
                rest = 10;
                continue;
            };
            let notices: Vec<homelab_core::notify::HostNotice> = v
                .get("notices")
                .cloned()
                .and_then(|n| serde_json::from_value(n).ok())
                .unwrap_or_default();
            let last = v.get("last_seq").and_then(|x| x.as_u64()).unwrap_or(0);
            center.host_notices(notices, last, job_of.as_ref()).await;
        }
    });
}

/// Reads the open Today items for the digest (a slow read on the host).
pub type TodayRead = Arc<
    dyn Fn() -> Pin<Box<dyn Future<Output = Result<Vec<notify::DigestLine>, String>> + Send>>
        + Send
        + Sync,
>;

/// Decision daily-digest: look every `every` whether the digest is due.
pub fn spawn_digest(center: Arc<NotifyCenter>, today: TodayRead, every: Duration) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(every);
        loop {
            t.tick().await;
            let read = today.clone();
            center.digest_tick(move || read()).await;
        }
    });
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
