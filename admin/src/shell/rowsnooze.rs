//! ask-kp11be-7: the Inbox's per-row snooze and remind-me. The rows are the
//! `inbox_snoozes` table of the app database (`shell::store`); every change
//! is one transaction, so a power cut leaves the state before or after it.
//! Until 4.0.0 this was `inbox-snoozes.json`; the one-time import moved it.
//!
//! An ended snooze is finished by definition: the tick removes it the
//! moment it ends (telling a reminder first), and the kit's daily clean-up
//! removes one that ended while the dashboard was off, after the same 30
//! days of grace the JSON store gave.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chassis::storage::AppDb;
use chassis::storage::rusqlite::{Connection, Row, params};

use super::actions::{Clock, Publish};
use super::actions_notify::NotifyCenter;
use crate::core::notify::{Detail, Draft, Kind, Level};
use crate::core::rowsnooze::{self, Entry, MAX_ROWS, Op, Request};

pub struct RowSnoozes {
    db: AppDb,
    clock: Clock,
    publish: Arc<dyn Publish>,
}

const COLS: &str = "row_key, until, title, href, remind, set_at";

fn entry(r: &Row) -> chassis::storage::rusqlite::Result<Entry> {
    Ok(Entry {
        key: r.get(0)?,
        until: r.get(1)?,
        title: r.get(2)?,
        href: r.get(3)?,
        remind: r.get::<_, i64>(4)? != 0,
        set_at: r.get(5)?,
    })
}

fn active(c: &Connection, now: i64) -> Result<Vec<Entry>, chassis::Error> {
    let mut stmt = c.prepare(&format!(
        "SELECT {COLS} FROM inbox_snoozes WHERE until > ?1 ORDER BY until, row_key"
    ))?;
    Ok(stmt
        .query_map(params![now], entry)?
        .collect::<Result<Vec<_>, _>>()?)
}

fn view(rows: &[Entry], now: i64) -> serde_json::Value {
    serde_json::json!({ "snoozes": rows, "now": now })
}

impl RowSnoozes {
    /// Nothing is read here: the database opens at start, and a request or
    /// the first tick finds the rows.
    pub fn load(db: AppDb, clock: Clock, publish: Arc<dyn Publish>) -> Arc<Self> {
        Arc::new(RowSnoozes { db, clock, publish })
    }

    /// The snoozes still running, soonest first.
    pub async fn snapshot(&self) -> Result<serde_json::Value, String> {
        let now = (self.clock)();
        let rows = self
            .db
            .read(move |c| active(c, now))
            .await
            .map_err(|e| e.message)?;
        Ok(view(&rows, now))
    }

    pub async fn apply(&self, r: Request) -> Result<serde_json::Value, String> {
        let now = (self.clock)();
        let op = rowsnooze::plan(r, now)?;
        let outcome = self
            .db
            .write(move |tx| {
                match op {
                    Op::End(key) => {
                        tx.execute("DELETE FROM inbox_snoozes WHERE row_key = ?1", params![key])?;
                    }
                    Op::Set(e) => {
                        let known: i64 = tx.query_row(
                            "SELECT COUNT(*) FROM inbox_snoozes WHERE row_key = ?1",
                            params![e.key],
                            |r| r.get(0),
                        )?;
                        if known == 0 {
                            let n: i64 = tx.query_row(
                                "SELECT COUNT(*) FROM inbox_snoozes",
                                [],
                                |r| r.get(0),
                            )?;
                            if n as usize >= MAX_ROWS {
                                return Ok(Err(rowsnooze::full_message()));
                            }
                        }
                        tx.execute(
                            "INSERT INTO inbox_snoozes (row_key, until, title, href, remind, set_at) \
                             VALUES (?1,?2,?3,?4,?5,?6) ON CONFLICT(row_key) DO UPDATE SET \
                             until = excluded.until, title = excluded.title, href = excluded.href, \
                             remind = excluded.remind, set_at = excluded.set_at",
                            params![e.key, e.until, e.title, e.href, e.remind as i64, e.set_at],
                        )?;
                    }
                }
                Ok(Ok(active(tx, now)?))
            })
            .await
            .map_err(|e| e.message)??;
        let v = view(&outcome, now);
        self.publish.publish("inbox_snoozes", v.clone());
        Ok(v)
    }

    /// End the snoozes whose time has come; tell the ones that asked for a
    /// reminder as a notice.
    pub async fn tick(&self, center: &NotifyCenter) {
        let now = (self.clock)();
        let taken = self
            .db
            .write(move |tx| {
                let mut stmt = tx.prepare(&format!(
                    "SELECT {COLS} FROM inbox_snoozes WHERE until <= ?1 ORDER BY until, row_key"
                ))?;
                let ended = stmt
                    .query_map(params![now], entry)?
                    .collect::<Result<Vec<_>, _>>()?;
                drop(stmt);
                if ended.is_empty() {
                    return Ok(None);
                }
                tx.execute("DELETE FROM inbox_snoozes WHERE until <= ?1", params![now])?;
                Ok(Some((ended, active(tx, now)?)))
            })
            .await;
        let (ended, rows) = match taken {
            Ok(Some(t)) => t,
            Ok(None) => return,
            Err(e) => {
                tracing::warn!(error = %e.message, "inbox snoozes not ended");
                return;
            }
        };
        self.publish.publish("inbox_snoozes", view(&rows, now));
        for e in ended.into_iter().filter(|e| e.remind) {
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

async fn list(State(s): State<Arc<RowSnoozes>>) -> Response {
    match s.snapshot().await {
        Ok(v) => Json(v).into_response(),
        Err(why) => failed(why),
    }
}

fn failed(why: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({
            "what": "the snoozes", "why": why,
            "fix": "the dashboard's log has the detail; the database file may need a restore",
        })),
    )
        .into_response()
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
