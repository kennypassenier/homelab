//! The TUI parity round's read routes (Kenny, 2026-09-28: "Wat nu in de TUI
//! kan, moet nog altijd kunnen in ons systeem"): today with its verdict, the
//! fleet check, one incident bundle, the templates, ping with where the
//! address came from, the versions and the update badge, drift per stack,
//! the apply plan and a stack's deploy plan, and the files a browser
//! downloads (the runbook, a stack's export bundle).
//!
//! Every write goes through the action queue (`shell::actions`), so a press
//! here and a driven press are the same job; these routes only read.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use axum::extract::{Path as UrlPath, Query, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use homelab_proto::Command;
use serde::Deserialize;

use super::actions::{Actions, HostPort, LocalStacks, StackFiles};
use super::host_link::{Shared, now_s};
use super::slow::{RunQuery, SlowRead, WAIT};
use crate::core::actions::{Refusal, valid_stack_name};
use crate::core::drift::drift_state;

/// How long a drift reading is reused (it runs latch once per stack).
pub const DRIFT_REUSE_S: u64 = 300;

#[derive(Clone)]
pub struct ParityCtx {
    pub host: Arc<dyn HostPort>,
    pub shared: Shared,
    pub actions: Actions,
    pub files: Arc<dyn StackFiles>,
    /// The working copy (`…/repo`); its `stacks/` is what the CLI reads.
    pub repo: PathBuf,
    /// Where a download is written before it is read back and removed.
    pub scratch: PathBuf,
    drift: Arc<Mutex<Option<(u64, LocalStacks)>>>,
    /// Today and the fleet check take about 90 s: started once, asked after
    /// (`shell::slow`).
    today_read: Arc<SlowRead>,
    check_read: Arc<SlowRead>,
    /// feat-overview-10: the backup calendar's own read of restic, over the
    /// network — the whole-fleet shape, kept for a caller that has not
    /// learned the fleet's stack names yet (`stacks: Vec::new()`, the same
    /// one the TUI's own path asks the host directly).
    backup_calendar_read: Arc<SlowRead>,
    /// fix-177: one restic read is one stack, never the whole fleet — a
    /// slow or hung repository (an rclone remote stuck on a stalled
    /// upload, say) then times out alone, instead of its `ask()` budget
    /// swallowing the 20-odd other stacks that already answered. Each
    /// stack gets its own `SlowRead` (so a page reload or second tab joins
    /// the run already on its way, same as `today_read`), created the
    /// first time that stack is asked for.
    backup_calendar_reads: Arc<Mutex<HashMap<String, Arc<SlowRead>>>>,
    /// Kept to build a `backup_calendar_reads` entry lazily, announced the
    /// same way as every other slow read.
    publish: Arc<dyn super::actions::Publish>,
    /// Decision "23 constants": [`DRIFT_REUSE_S`] by default,
    /// `HOMELAB_ADMIN_DRIFT_REUSE_S` in `mount()`.
    drift_reuse_s: u64,
}

impl ParityCtx {
    pub fn new(
        host: Arc<dyn HostPort>,
        shared: Shared,
        actions: Actions,
        files: Arc<dyn StackFiles>,
        repo: PathBuf,
        scratch: PathBuf,
        publish: Arc<dyn super::actions::Publish>,
    ) -> Self {
        Self::with_drift_reuse(
            host,
            shared,
            actions,
            files,
            repo,
            scratch,
            publish,
            DRIFT_REUSE_S,
        )
    }

    /// Decision "23 constants": the drift reuse window from `ActConfig`
    /// (`HOMELAB_ADMIN_DRIFT_REUSE_S`); `mount()` only.
    #[allow(clippy::too_many_arguments)]
    pub fn with_drift_reuse(
        host: Arc<dyn HostPort>,
        shared: Shared,
        actions: Actions,
        files: Arc<dyn StackFiles>,
        repo: PathBuf,
        scratch: PathBuf,
        publish: Arc<dyn super::actions::Publish>,
        drift_reuse_s: u64,
    ) -> Self {
        ParityCtx {
            host,
            shared,
            actions,
            files,
            repo,
            scratch,
            drift: Arc::new(Mutex::new(None)),
            today_read: SlowRead::announced("today", "today", publish.clone()),
            check_read: SlowRead::announced("the fleet check", "fleet-check", publish.clone()),
            backup_calendar_read: SlowRead::announced(
                "the backup calendar",
                "backup-calendar",
                publish.clone(),
            ),
            backup_calendar_reads: Arc::new(Mutex::new(HashMap::new())),
            publish,
            drift_reuse_s,
        }
    }

    /// fix-177: the per-stack `SlowRead` for the backup calendar, created
    /// the first time a page asks for that stack.
    fn backup_calendar_read_for(&self, stack: &str) -> Arc<SlowRead> {
        let mut m = self
            .backup_calendar_reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        m.entry(stack.to_string())
            .or_insert_with(|| {
                SlowRead::announced(
                    format!("{stack}'s backup calendar"),
                    format!("backup-calendar:{stack}"),
                    self.publish.clone(),
                )
            })
            .clone()
    }
}

fn refused(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

fn bad_gateway(what: &str, why: String) -> Response {
    refused(
        StatusCode::BAD_GATEWAY,
        Refusal::new(
            what,
            why,
            "the dashboard asks the host over its one line; check that it answers (Ping on the host page)",
        ),
    )
}

async fn ask(
    c: &ParityCtx,
    command: Command,
    secs: u64,
) -> Result<homelab_proto::RpcResponse, String> {
    c.host
        .ask_traced(command, Duration::from_secs(secs), None)
        .await
}

/// The working copy's stack files with the vmid each claims, and their
/// digests, as `homelab check` and `homelab today` send them. Empty (with
/// the reason) without a working copy: the host's half is still asked.
fn stack_side(
    repo: &std::path::Path,
) -> (
    Vec<(String, u16)>,
    Vec<homelab_core::ops::fleetcheck::StackDigest>,
    Option<String>,
) {
    let base = repo.join("stacks");
    if !base.is_dir() {
        return (
            Vec::new(),
            Vec::new(),
            Some(
                "the dashboard has no working copy, so only what the host can see was checked; \
                 the half that compares the stack files with the fleet was skipped"
                    .into(),
            ),
        );
    }
    let files = homelab_client::spec::stack_files_with_vmids(&base.display().to_string());
    let digests = files
        .iter()
        .filter_map(|(dir, _)| homelab_client::spec::stack_digest(std::path::Path::new(dir)).ok())
        .collect();
    (files, digests, None)
}

/// fix-110 (fix-170: also drops host-held keys): `config/host.toml` as the
/// dashboard's working copy reads it (non-secret, non-host-held keys only),
/// for `Today`/`FleetCheck` to compare against the host's running settings.
/// `None` when the working copy has no such file.
fn host_config_side(
    repo: &std::path::Path,
) -> Option<std::collections::BTreeMap<String, serde_json::Value>> {
    let raw = std::fs::read_to_string(repo.join("config/host.toml")).ok()?;
    let table: toml::Table = toml::from_str(&raw).ok()?;
    let mut out = std::collections::BTreeMap::new();
    for (key, value) in &table {
        if homelab_core::hostconfig::is_secret(key) || homelab_core::hostconfig::is_host_held(key) {
            continue;
        }
        if let Ok(v) = serde_json::to_value(value) {
            out.insert(key.clone(), v);
        }
    }
    Some(out)
}

// ── today, the fleet check ───────────────────────────────────────────────

/// A failed host read as the slow read's answer: `{what, why, fix}`, 502.
fn gateway_value(what: &str, why: String) -> (StatusCode, serde_json::Value) {
    (
        StatusCode::BAD_GATEWAY,
        serde_json::to_value(Refusal::new(
            what,
            why,
            "the dashboard asks the host over its one line; check that it answers (Ping on the host page)",
        ))
        .unwrap_or_default(),
    )
}

/// fix-68 (`homelab today`): doctor, the fleet check with its manual checks
/// and the open incidents as one list and one verdict. About 90 s on pve,
/// so it is a slow read: started once, asked after with `?run=`.
async fn today(State(c): State<ParityCtx>, Query(q): Query<RunQuery>) -> Response {
    let read = c.today_read.clone();
    if q.last {
        return read.kept();
    }
    read.read(q.run, WAIT, move || read_today(c)).await
}

/// The host's Today reading with the working copy's stack files: the list,
/// how many stack files went with it, and why a half was skipped. The page
/// and the 09:00 digest both read it here.
async fn today_with_files(
    host: &Arc<dyn HostPort>,
    repo: &std::path::Path,
) -> Result<(homelab_core::ops::today::Today, usize, Option<String>), String> {
    let host_config = host_config_side(repo);
    let repo = repo.to_path_buf();
    let (stack_files, digests, skipped) = tokio::task::spawn_blocking(move || stack_side(&repo))
        .await
        .unwrap_or_default();
    let n = stack_files.len();
    let r = host
        .ask_traced(
            Command::Today {
                stack_files,
                digests,
                host_config,
            },
            Duration::from_secs(180),
            None,
        )
        .await?;
    serde_json::from_str::<homelab_core::ops::today::Today>(&r.message)
        .map(|t| (t, n, skipped))
        .map_err(|_| {
            format!(
                "the host answered: {}",
                r.message.chars().take(200).collect::<String>()
            )
        })
}

/// Kenny, 2026-09-30 09:16: every row (a Today item, a finding) whose
/// remedy names a command the dashboard runs carries it as `fix`, the
/// action dialog its button opens (`core::notify::fix_for`).
pub fn with_fixes(mut rows: serde_json::Value) -> serde_json::Value {
    if let Some(list) = rows.as_array_mut() {
        for row in list.iter_mut() {
            let fix = row.get("remedy").and_then(|r| r.as_str()).and_then(|r| {
                crate::core::notify::fix_for(&crate::core::notify::FixSource::Text(r))
            });
            if let (Some(o), Some(f)) = (row.as_object_mut(), fix) {
                o.insert("fix".into(), serde_json::to_value(f).unwrap_or_default());
            }
        }
    }
    rows
}

/// fix-219 (drift-finding-names-only-filenames): marks every row that is a
/// repo-drift finding from `evaluate_repo_drift` — the one kind whose `what`
/// names bare filenames where Kenny wants the actual change — with
/// `drift_diffable` and the stack name it is about (`drift_stack`), so the
/// Health page knows which findings can expand into a diff (fetched from
/// `/data/fleet-check/{stack}/diff`, only when opened). `has_subject` is
/// `true` for a fleet-check finding row (its own `subject` field names the
/// stack directly) and `false` for a `Today` item (`ops::today::assemble`
/// folds subject and what into one `"<subject>: <what>"` string).
pub fn with_repo_drift(mut rows: serde_json::Value, has_subject: bool) -> serde_json::Value {
    if let Some(list) = rows.as_array_mut() {
        for row in list.iter_mut() {
            let what = row
                .get("what")
                .and_then(|w| w.as_str())
                .unwrap_or_default()
                .to_string();
            let diffable = homelab_core::ops::fleetcheck::is_repo_file_drift_text(&what);
            let stack = diffable
                .then(|| {
                    if has_subject {
                        row.get("subject")
                            .and_then(|s| s.as_str())
                            .map(str::to_string)
                    } else {
                        what.split_once(": ").map(|(s, _)| s.to_string())
                    }
                })
                .flatten();
            if let Some(o) = row.as_object_mut() {
                o.insert(
                    "drift_diffable".into(),
                    serde_json::Value::Bool(stack.is_some()),
                );
                if let Some(s) = stack {
                    o.insert("drift_stack".into(), serde_json::Value::String(s));
                }
            }
        }
    }
    rows
}

/// Decision daily-digest: the open Today items.
pub async fn fetch_today(
    host: &Arc<dyn HostPort>,
    repo: &std::path::Path,
) -> Result<homelab_core::ops::today::Today, String> {
    today_with_files(host, repo).await.map(|(t, _, _)| t)
}

async fn read_today(c: ParityCtx) -> (StatusCode, serde_json::Value) {
    match today_with_files(&c.host, &c.repo).await {
        Ok((t, n, skipped)) => {
            let mut today = serde_json::to_value(&t).unwrap_or_default();
            today["items"] = with_repo_drift(with_fixes(today["items"].clone()), false);
            (
                StatusCode::OK,
                serde_json::json!({
                    "today": today,
                    "verdict": t.verdict(),
                    "needs_you": t.needs_you(),
                    "stack_files": n,
                    "skipped": skipped,
                    "measured_at": now_s(),
                }),
            )
        }
        Err(e) => gateway_value("today", e),
    }
}

/// Y4 (`homelab check`, the TUI's c): the repository against the fleet.
/// The edge and pin comparisons need a workstation's Cloudflare token and
/// registry access; the page says so. About 90 s on pve: a slow read.
async fn fleet_check(State(c): State<ParityCtx>, Query(q): Query<RunQuery>) -> Response {
    let read = c.check_read.clone();
    read.read(q.run, WAIT, move || read_fleet_check(c)).await
}

async fn read_fleet_check(c: ParityCtx) -> (StatusCode, serde_json::Value) {
    let host_config = host_config_side(&c.repo);
    let repo = c.repo.clone();
    let (stack_files, digests, skipped) = tokio::task::spawn_blocking(move || stack_side(&repo))
        .await
        .unwrap_or_default();
    let n = stack_files.len();
    match ask(
        &c,
        Command::FleetCheck {
            stack_files,
            digests,
            json: true,
            host_config,
        },
        180,
    )
    .await
    {
        Ok(r) => match serde_json::from_str::<serde_json::Value>(&r.message) {
            Ok(v) => (
                StatusCode::OK,
                serde_json::json!({
                    "passes": v["passes"],
                    "findings": with_repo_drift(with_fixes(v["findings"].clone()), true),
                    "stack_files": n,
                    "skipped": skipped,
                    "not_here": "The Cloudflare edge and the registries' pinned digests are compared from a workstation: homelab check.",
                    "measured_at": now_s(),
                }),
            ),
            Err(_) => gateway_value(
                "the fleet check",
                format!(
                    "the host answered text, not JSON: {}",
                    r.message.chars().take(200).collect::<String>()
                ),
            ),
        },
        Err(e) => gateway_value("the fleet check", e),
    }
}

/// `?stack=`: fix-177's per-stack read. Omitted asks the whole fleet in one
/// call (the page's own fallback before it has learned the fleet's stack
/// names, and anything else still pointed at the old shape).
#[derive(Debug, Deserialize, Default)]
struct BackupCalendarQuery {
    run: Option<u64>,
    stack: Option<String>,
    /// fix-180: the page's own Refresh button — asks the host to read
    /// restic again for every owner involved, in the background, even
    /// though this call itself still answers at once from whatever is
    /// cached.
    #[serde(default, deserialize_with = "crate::shell::queryflag::flag")]
    refresh: bool,
}

/// feat-overview-10 (backup calendar), fix-177 (per-stack progress): every
/// snapshot night, per stack, read straight from restic through the host —
/// its own `BackupCalendar` command. `?stack=` reads one stack alone, on
/// its own `SlowRead`, so one repository stuck behind a slow or hung
/// network path (restic over rclone, say) times out by itself instead of
/// an `ask()` budget sized for the whole fleet swallowing every stack that
/// already answered (measured live on CT 120, 2026-10-02: a whole-fleet
/// read hit its 180 s budget and answered 502 with no stack singled out).
/// Without `?stack=` the old whole-fleet shape still answers, on its own
/// `SlowRead` so the two never restart each other.
async fn backup_calendar(
    State(c): State<ParityCtx>,
    Query(q): Query<BackupCalendarQuery>,
) -> Response {
    let force = q.refresh;
    match q.stack {
        Some(stack) => {
            let read = c.backup_calendar_read_for(&stack);
            let wanted = stack.clone();
            read.read(q.run, WAIT, move || {
                read_backup_calendar(c, vec![wanted], force)
            })
            .await
        }
        None => {
            let read = c.backup_calendar_read.clone();
            read.read(q.run, WAIT, move || {
                read_backup_calendar(c, Vec::new(), force)
            })
            .await
        }
    }
}

/// `stacks`: empty asks every stack the host knows (the whole-fleet path);
/// one name is fix-177's per-stack read. `ASK_SECS` is per stack: a single
/// repository's `restic snapshots --json` is capped at 120 s host-side
/// (`core::ops::backup::snapshot_nights_unix`), so one stack with more than
/// one owner group can legitimately run past that; the whole-fleet path
/// reuses the same per-call budget since it is the TUI/CLI's own fallback,
/// not the dashboard's main path any more.
const ASK_SECS: u64 = 170;

async fn read_backup_calendar(
    c: ParityCtx,
    stacks: Vec<String>,
    force: bool,
) -> (StatusCode, serde_json::Value) {
    let what = match stacks.first() {
        Some(s) if stacks.len() == 1 => format!("{s}'s backup calendar"),
        _ => "the backup calendar".to_string(),
    };
    match ask(
        &c,
        Command::BackupCalendar {
            stacks: stacks.clone(),
            force,
        },
        ASK_SECS,
    )
    .await
    {
        Ok(r) => match serde_json::from_str::<serde_json::Value>(&r.message) {
            Ok(v) => (
                StatusCode::OK,
                serde_json::json!({
                    "stacks": v["stacks"],
                    // fix-180: the host's OWN per-stack reading, not this
                    // call's clock — the cache can answer with data read
                    // minutes ago. `SlowRead` already stamps `read_at` with
                    // when THIS call happened.
                    "measured_at": v["measured_at"],
                    "skipped": v["skipped"],
                    // fix-202: a name that will never get a restic read
                    // (backs up nothing by design, or has no manifest on
                    // record / isn't a known stack) — forwarded as-is so the
                    // dashboard can show it at once instead of retrying it
                    // for minutes.
                    "no_backup": v["no_backup"],
                    "reasons": v["reasons"],
                }),
            ),
            Err(_) => {
                tracing::warn!(
                    stacks = ?stacks,
                    "backup-calendar: the host answered text, not JSON"
                );
                gateway_value(
                    &what,
                    format!(
                        "the host answered text, not JSON: {}",
                        r.message.chars().take(200).collect::<String>()
                    ),
                )
            }
        },
        Err(e) => {
            // fix-177: the live symptom was a 502 with nothing in either
            // journal to say why — the host doesn't log a read-only RPC's
            // start or end at all, and this ask() failure wasn't logged
            // either, so the only trace of it ever lived in this one HTTP
            // response. It now lands in the dashboard's own log too.
            tracing::warn!(stacks = ?stacks, error = %e, "backup-calendar: the host did not answer");
            gateway_value(&what, e)
        }
    }
}

/// feat-stacks-10 (overview of stale docker images): the same fleet-check
/// run the Health page uses (`c.check_read`, shared so visiting both pages
/// does not start the ~90 s read twice), its findings filtered down to the
/// ones fix-83's pin check writes (`crate::core::stale_images`).
async fn stale_images(State(c): State<ParityCtx>, Query(q): Query<RunQuery>) -> Response {
    let read = c.check_read.clone();
    let repo = c.repo.clone();
    // redesign-flows-2: `?last=1` (the Inbox counter's read on every page)
    // filters the kept answer without starting a fleet check.
    let resp = if q.last {
        read.kept()
    } else {
        read.read(q.run, WAIT, move || read_fleet_check(c)).await
    };
    // read_fleet_check's own 202/error shapes (still running, or the host
    // could not be reached) pass straight through unfiltered; only a
    // finished 200 answer has `findings` to filter.
    let status = resp.status();
    if status != StatusCode::OK {
        return resp;
    }
    let bytes = match axum::body::to_bytes(resp.into_body(), usize::MAX).await {
        Ok(b) => b,
        Err(_) => {
            let (status, body) = gateway_value(
                "stale images",
                "the fleet check's answer did not read".to_string(),
            );
            return (status, Json(body)).into_response();
        }
    };
    let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
    let findings: Vec<crate::core::stale_images::FindingLike> =
        serde_json::from_value(body["findings"].clone()).unwrap_or_default();
    let mut rows = crate::core::stale_images::from_findings(&findings);
    // fix-231: which `<app>/<service>` of the working copy each row is, so
    // the page offers Update only on a pin a stack file holds.
    rows = tokio::task::spawn_blocking(move || {
        let mut texts: std::collections::BTreeMap<String, crate::core::stackedit::StackTexts> =
            Default::default();
        for row in &mut rows {
            let Some((stack, container)) = row.where_.split_once('/') else {
                continue;
            };
            if !crate::core::actions::valid_stack_name(stack) {
                continue;
            }
            let t = texts
                .entry(stack.to_string())
                .or_insert_with(|| super::workcopy::read_texts(&repo.join("stacks").join(stack)));
            row.key = crate::core::stale_images::locate(t, container);
        }
        rows
    })
    .await
    .unwrap_or_default();
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "images": rows,
            "measured_at": body["measured_at"],
        })),
    )
        .into_response()
}

// ── one incident, the templates, ping, versions ─────────────────────────

/// A bundle name as `Incidents` lists them (`1790530911-deploy-uptime`).
pub fn valid_incident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !name.contains("..")
}

/// fix-131 (`homelab incidents show <name>`): one bundle, the error, the
/// versions and the end of its transcript.
async fn incident(State(c): State<ParityCtx>, UrlPath(name): UrlPath<String>) -> Response {
    if !valid_incident(&name) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("incident {name:?}"),
                "not a bundle name",
                "pick one from the incidents list",
            ),
        );
    }
    match ask(&c, Command::IncidentShow { name: name.clone() }, 60).await {
        Ok(r) if r.ok => Json(serde_json::json!({
            "name": name,
            "text": homelab_core::executor::mask_secrets(&r.message),
        }))
        .into_response(),
        Ok(r) => refused(
            StatusCode::NOT_FOUND,
            Refusal::new(
                format!("incident {name}"),
                r.message,
                "pick one from the incidents list",
            ),
        ),
        Err(e) => bad_gateway("the incident", e),
    }
}

/// C5 (`homelab templates`): the golden templates and the OS templates.
async fn templates(State(c): State<ParityCtx>) -> Response {
    match ask(&c, Command::ListTemplates, 90).await {
        Ok(r) if r.ok => Json(serde_json::json!({
            "templates": crate::core::templates::parse(&r.message),
            "text": r.message,
            "measured_at": now_s(),
        }))
        .into_response(),
        Ok(r) => bad_gateway("the templates", r.message),
        Err(e) => bad_gateway("the templates", e),
    }
}

/// `homelab ping`: the round trip, the address and where it came from.
async fn ping(State(c): State<ParityCtx>) -> Response {
    let t = std::time::Instant::now();
    let r = ask(&c, Command::Ping, 15).await;
    let ms = t.elapsed().as_millis() as u64;
    let s = c.shared.read().await;
    let facts = serde_json::json!({
        "address": s.host_addr,
        "address_source": s.host_addr_source,
        "pin": homelab_client::repo_config::built_in_pin().map(|p| format!("SHA256:{p}")),
        "pin_source": "compiled into this build (the repository's config/client.toml pin)",
        "tls_fingerprint": s.fleet.as_ref().map(|f| f.host.tls_fingerprint.clone()),
        "host_version": s.host_version,
    });
    drop(s);
    match r {
        Ok(r) => Json(serde_json::json!({
            "ok": r.ok, "message": r.message, "ms": ms, "facts": facts, "measured_at": now_s(),
        }))
        .into_response(),
        Err(e) => Json(serde_json::json!({
            "ok": false, "message": e, "ms": ms, "facts": facts, "measured_at": now_s(),
        }))
        .into_response(),
    }
}

/// The versions and the two warnings (update available, dashboard older).
async fn versions(State(c): State<ParityCtx>) -> Json<serde_json::Value> {
    let s = c.shared.read().await;
    let mut v = crate::core::hostversion::release_view(
        s.latest_release.as_deref(),
        s.host_version.as_deref(),
    );
    v["checked_at"] = serde_json::json!(s.latest_checked_at);
    Json(v)
}

// ── drift, the plans ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct Fresh {
    #[serde(default, deserialize_with = "crate::shell::queryflag::flag")]
    fresh: bool,
}

/// [CHANGED] per stack (fix-107's words). Computing it runs latch once per
/// stack (the host's hash covers the secrets), so it runs only when asked
/// (`fresh`, the pages' "Compare with the files"); otherwise the newest
/// reading is answered, or none, and nothing is computed behind a page view.
async fn drift(State(c): State<ParityCtx>, Query(q): Query<Fresh>) -> Response {
    let now = now_s();
    let cached = c
        .drift
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let reuse = cached
        .clone()
        .filter(|(at, _)| !q.fresh || now.saturating_sub(*at) < c.drift_reuse_s);
    let (at, local) = match (reuse, q.fresh) {
        (Some(x), _) => x,
        (None, false) => {
            return Json(serde_json::json!({ "stacks": {}, "measured_at": null })).into_response();
        }
        (None, true) => {
            let files = c.files.clone();
            let read = tokio::task::spawn_blocking(move || files.local_stacks(false)).await;
            match read {
                Ok(Ok(local)) => {
                    *c.drift.lock().unwrap_or_else(PoisonError::into_inner) =
                        Some((now, local.clone()));
                    (now, local)
                }
                Ok(Err(r)) => return refused(StatusCode::CONFLICT, r),
                Err(e) => {
                    return refused(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Refusal::new("drift", e.to_string(), "report this"),
                    );
                }
            }
        }
    };
    let s = c.shared.read().await;
    let Some(fleet) = s.fleet.as_ref() else {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            Refusal::new(
                "drift",
                "the dashboard has not read the host's fleet yet",
                "wait for the fleet page",
            ),
        );
    };
    let stacks: serde_json::Map<String, serde_json::Value> = fleet
        .stacks
        .iter()
        .map(|st| {
            let local_hash = local
                .hashes
                .iter()
                .find(|(n, _)| *n == st.name)
                .map(|(_, h)| h);
            let state = drift_state(&st.applied_hash, local_hash);
            let why = local_hash.and_then(|h| h.as_ref().err()).cloned();
            (
                st.name.clone(),
                serde_json::json!({ "state": state, "label": state.label(), "why": why }),
            )
        })
        .collect();
    Json(serde_json::json!({ "stacks": stacks, "measured_at": at })).into_response()
}

/// dash-apply: the plan against the host (no programs are downloaded; the
/// secrets are read through latch, since the host's hash covers them).
async fn apply_plan(State(c): State<ParityCtx>) -> Response {
    match c.actions.apply_plan(false).await {
        Ok((view, _)) => Json(serde_json::json!({
            "plan": view,
            "pending": view.pending(),
            "measured_at": now_s(),
        }))
        .into_response(),
        Err(r) => refused(StatusCode::CONFLICT, r),
    }
}

/// fix-100's per-file plan of one stack, as the Deploy review shows it.
async fn stack_plan(State(c): State<ParityCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    if !valid_stack_name(&stack) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("the plan of {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ),
        );
    }
    match c.actions.deploy_diff(&stack).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => refused(StatusCode::CONFLICT, r),
    }
}

/// fix-219: the actual per-file change behind a repo-drift finding, read
/// only when the Health page's finding is expanded.
async fn fleet_check_file_diff(
    State(c): State<ParityCtx>,
    UrlPath(stack): UrlPath<String>,
) -> Response {
    if !valid_stack_name(&stack) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("the diff of {stack:?}"),
                "not a stack name",
                "use the stack name the Health page shows",
            ),
        );
    }
    match c.actions.repo_drift_file_diff(&stack).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => refused(StatusCode::CONFLICT, r),
    }
}

// ── downloads ───────────────────────────────────────────────────────────

fn attachment(name: &str, mime: &str, body: String) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
            (header::CACHE_CONTROL, "no-store".to_string()),
        ],
        body,
    )
        .into_response()
}

fn no_working_copy(what: &str) -> Response {
    refused(
        StatusCode::CONFLICT,
        Refusal::new(
            what,
            "the dashboard has no working copy of the homelab repository",
            "the working copy panel on the settings page says what is missing",
        ),
    )
}

/// E7 (`homelab runbook`): the disaster-recovery runbook, written from the
/// stack files, as a download.
async fn runbook(State(c): State<ParityCtx>) -> Response {
    let base = c.repo.join("stacks");
    if !base.is_dir() {
        return no_working_copy("the runbook");
    }
    let tmp = c.scratch.join(format!("runbook-{}.md", std::process::id()));
    let r = tokio::task::spawn_blocking(move || {
        let _ = std::fs::create_dir_all(tmp.parent().unwrap_or(&tmp));
        let out = homelab_client::spec::generate_runbook(&base, &tmp.display().to_string())
            .and_then(|_| std::fs::read_to_string(&tmp).map_err(|e| e.to_string()));
        let _ = std::fs::remove_file(&tmp);
        out
    })
    .await;
    match r {
        Ok(Ok(text)) => attachment("DR_RUNBOOK.md", "text/markdown; charset=utf-8", text),
        Ok(Err(why)) => refused(
            StatusCode::CONFLICT,
            Refusal::new("the runbook", why, "fix the stack files it names"),
        ),
        Err(e) => refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            Refusal::new("the runbook", e.to_string(), "report this"),
        ),
    }
}

/// D11 (`homelab export`): one stack as a single YAML bundle, never with
/// secrets, as a download.
async fn export(State(c): State<ParityCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    if !valid_stack_name(&stack) {
        return refused(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                format!("export {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ),
        );
    }
    let dir = c.repo.join("stacks").join(&stack);
    if !dir.is_dir() {
        return no_working_copy(&format!("export {stack}"));
    }
    match tokio::task::spawn_blocking(move || homelab_client::spec::bundle_text(&dir)).await {
        Ok(Ok((text, _))) => attachment(
            &format!("{stack}-bundle.yml"),
            "application/yaml; charset=utf-8",
            text,
        ),
        Ok(Err(why)) => refused(
            StatusCode::CONFLICT,
            Refusal::new(
                format!("export {stack}"),
                why,
                format!("fix stacks/{stack}"),
            ),
        ),
        Err(e) => refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            Refusal::new("export", e.to_string(), "report this"),
        ),
    }
}

/// Mounted with `dashboard_routes`: behind the login and both locks.
pub fn router(c: ParityCtx) -> Router {
    Router::new()
        .route("/data/today", get(today))
        .route("/data/fleet-check", get(fleet_check))
        .route("/data/stale-images", get(stale_images))
        .route("/data/backup-calendar", get(backup_calendar))
        .route("/data/incidents/{name}", get(incident))
        .route("/data/templates", get(templates))
        .route("/data/ping", get(ping))
        .route("/data/versions", get(versions))
        .route("/data/drift", get(drift))
        .route("/data/apply/plan", get(apply_plan))
        .route("/data/plan/{stack}", get(stack_plan))
        .route("/data/fleet-check/{stack}/diff", get(fleet_check_file_diff))
        .route("/data/download/runbook", get(runbook))
        .route("/data/download/export/{stack}", get(export))
        .with_state(c)
}

#[cfg(test)]
mod fix_219_with_repo_drift_tests {
    use super::with_repo_drift;

    /// A fleet-check finding row (`has_subject = true`): the stack is read
    /// straight from its own `subject` field.
    #[test]
    fn a_fleet_check_repo_drift_row_carries_its_stack_from_subject() {
        let rows = serde_json::json!([
            {
                "severity": "Drift",
                "subject": "syncthing",
                "what": "the files differ from what the host applied on 2026-10-02 — changed: a",
                "remedy": "x",
            },
            {
                "severity": "Drift",
                "subject": "kyu",
                "what": "net0 has firewall=0, so Proxmox applies none of the declared rules",
                "remedy": "x",
            },
        ]);
        let got = with_repo_drift(rows, true);
        let list = got.as_array().unwrap();
        assert_eq!(list[0]["drift_diffable"], serde_json::json!(true));
        assert_eq!(list[0]["drift_stack"], serde_json::json!("syncthing"));
        assert_eq!(list[1]["drift_diffable"], serde_json::json!(false));
        assert!(list[1].get("drift_stack").is_none());
    }

    /// A `Today` item (`has_subject = false`): the stack name is folded into
    /// `what` as `"<subject>: <what>"` by `ops::today::assemble`, so it is
    /// split out of there instead.
    #[test]
    fn a_today_item_repo_drift_row_carries_its_stack_split_from_what() {
        let rows = serde_json::json!([{
            "level": "Attention",
            "source": "check",
            "what": "syncthing: the files differ from what the host applied on 2026-10-02 — changed: a",
            "remedy": "x",
        }]);
        let got = with_repo_drift(rows, false);
        let list = got.as_array().unwrap();
        assert_eq!(list[0]["drift_diffable"], serde_json::json!(true));
        assert_eq!(list[0]["drift_stack"], serde_json::json!("syncthing"));
    }

    #[test]
    fn a_non_drift_row_is_never_marked_diffable() {
        let rows = serde_json::json!([{
            "level": "Broken",
            "source": "doctor",
            "what": "some other finding entirely",
            "remedy": "x",
        }]);
        let got = with_repo_drift(rows, false);
        let list = got.as_array().unwrap();
        assert_eq!(list[0]["drift_diffable"], serde_json::json!(false));
        assert!(list[0].get("drift_stack").is_none());
    }
}
