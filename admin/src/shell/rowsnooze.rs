//! ask-kp11be-7: the Inbox's per-row snooze and remind-me. The file is
//! `inbox-snoozes.json` under the data dir, written temp + fsync + rename
//! (arch-state), so a power cut leaves the old file or the new one. A file
//! that does not read is set aside as `.bad` and the snoozes start empty:
//! the rows simply show again, which loses nothing that mattered.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use tokio::sync::Mutex;

use super::actions::{Clock, Publish};
use super::actions_notify::NotifyCenter;
use super::actions_state::{read_json, write_json};
use crate::core::notify::{Detail, Draft, Kind, Level};
use crate::core::rowsnooze::{Entry, File, Request, SCHEMA};

pub struct RowSnoozes {
    path: PathBuf,
    clock: Clock,
    publish: Arc<dyn Publish>,
    state: Mutex<File>,
}

impl RowSnoozes {
    pub fn load(path: PathBuf, clock: Clock, publish: Arc<dyn Publish>) -> Arc<Self> {
        let now = clock();
        let mut file = match read_json::<File>(&path) {
            Ok(Some(f)) => f,
            Ok(None) => File::default(),
            Err(e) => {
                tracing::warn!("inbox snoozes not read, starting empty: {e}");
                let _ = std::fs::rename(&path, path.with_extension("json.bad"));
                File::default()
            }
        };
        // Snoozes that ended while the dashboard was off just show again;
        // a reminder asked for is told by the first tick.
        file.rows.retain(|e| e.until > now - 30 * 86_400);
        Arc::new(RowSnoozes {
            path,
            clock,
            publish,
            state: Mutex::new(file),
        })
    }

    fn save(&self, f: &File) {
        let mut out = f.clone();
        out.schema_version = SCHEMA;
        if let Err(e) = write_json(&self.path, &out) {
            tracing::warn!("inbox snoozes not written: {e}");
        }
    }

    fn view(&self, f: &File) -> serde_json::Value {
        let now = (self.clock)();
        serde_json::json!({ "snoozes": f.active(now), "now": now })
    }

    pub async fn apply(&self, r: Request) -> Result<serde_json::Value, String> {
        let now = (self.clock)();
        let mut f = self.state.lock().await;
        f.apply(r, now)?;
        self.save(&f);
        let v = self.view(&f);
        drop(f);
        self.publish.publish("inbox_snoozes", v.clone());
        Ok(v)
    }

    /// End the snoozes whose time has come; tell the ones that asked for a
    /// reminder as a notice.
    pub async fn tick(&self, center: &NotifyCenter) {
        let now = (self.clock)();
        let (due, view) = {
            let mut f = self.state.lock().await;
            if !f.rows.iter().any(|e| e.until <= now) {
                return;
            }
            let due = f.take_ended(now);
            self.save(&f);
            (due, self.view(&f))
        };
        self.publish.publish("inbox_snoozes", view);
        for e in due {
            center.notify(reminder(&e)).await;
        }
    }
}

/// The notice a "remind me" leaves when its time comes.
pub fn reminder(e: &Entry) -> Draft {
    let mut d = Draft::new(
        Kind::Reminder,
        &format!("reminder-{}", e.key),
        &format!("Reminder: {}", e.title),
        "You asked to be reminded of this row.",
    );
    d.detail = Detail {
        level: Level::Info,
        link: e.href.clone(),
        source: Some("reminder".into()),
        key: Some(e.key.clone()),
        ..Default::default()
    };
    d
}

async fn list(State(s): State<Arc<RowSnoozes>>) -> Json<serde_json::Value> {
    let f = s.state.lock().await;
    Json(s.view(&f))
}

async fn set(
    State(s): State<Arc<RowSnoozes>>,
    body: Result<Json<Request>, axum::extract::rejection::JsonRejection>,
) -> Response {
    let Json(r) = match body {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "what": "the snooze", "why": e.body_text(),
                    "fix": "send {key, until} (unix seconds, or null to end it)",
                })),
            )
                .into_response();
        }
    };
    match s.apply(r).await {
        Ok(v) => Json(v).into_response(),
        Err(why) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "what": "the snooze", "why": why, "fix": "pick a moment within the next year",
            })),
        )
            .into_response(),
    }
}

/// Mounted with `dashboard_routes`.
pub fn router(s: Arc<RowSnoozes>) -> Router {
    Router::new()
        .route("/data/inbox-snoozes", get(list).post(set))
        .with_state(s)
}

/// The loop that ends snoozes on time.
pub fn spawn(s: Arc<RowSnoozes>, center: Arc<NotifyCenter>) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(Duration::from_secs(30));
        loop {
            t.tick().await;
            s.tick(&center).await;
        }
    });
}
