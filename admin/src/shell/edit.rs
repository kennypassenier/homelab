//! milestone edit: changing the fleet from the browser.
//!
//! * feat-stacks-2: a stack's settings (and any of its files, raw) with a
//!   plan before the commit: the diff, what homelab would change on the
//!   machines, and what the host last applied; then optionally a deploy of
//!   exactly that commit through the action queue.
//! * feat-firewall-1 / feat-firewall-2: the firewall editor per stack, and
//!   every stack's rules and the fleet's matrix on one page.
//! * feat-stacks-3: a new stack, or a preset's app in an existing stack,
//!   from the repository's presets with the client's own scaffold.
//! * feat-settings-1: every host.toml key, read and written with commands
//!   answered to this session only (the TUI's settings screen keeps its
//!   unsaved edits); arch-self keys are read-only.
//!
//! Every change of the repository goes through the working copy's one
//! transaction (`workcopy`).

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use homelab_core::ops::deployguard;
use homelab_proto::{Command, FileBlob, StackManifest};
use serde::Deserialize;

use super::actions::{Actions, HostPort, Origin, Publish};
use super::host_link::Shared;
use super::workcopy::{Unpushed, WorkingCopy};
use crate::core::actions::{self, ActionArgs, ActionKind, Refusal, SELF_STACK};
use crate::core::editplan::{self, Effect, FileDiff};
use crate::core::fwmatrix::{self, FleetFirewall};
use crate::core::hostsettings;
use crate::core::newstack::{self, NewStack, Taken};
use crate::core::presetedit;
use crate::core::stackedit::{self, AddAppFiles, FileChange, StackEdit, StackTexts, MANIFEST};
use crate::core::stackedit_checks;
use crate::core::stackedit_files;
use crate::core::stackedit_latch;
use crate::core::stackedit_native;
use crate::core::stackedit_tiles;
use crate::core::yamledit;

/// The host release that answers `GetHostConfig` (feat-settings-1). An
/// older host drops a request it cannot parse without a word, so the page
/// asks only a host at least this new: the next release, 3.63.0.
pub const HOST_SETTINGS_SINCE: (u64, u64, u64) = crate::core::hostversion::NEXT_RELEASE;

#[derive(Clone)]
pub struct EditCtx {
    pub wc: Arc<WorkingCopy>,
    pub actions: Actions,
    pub host: Arc<dyn HostPort>,
    pub shared: Shared,
    pub publish: Arc<dyn Publish>,
}

fn refusal(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

fn body<T>(b: Result<Json<T>, JsonRejection>, what: &str) -> Result<T, Refusal> {
    b.map(|Json(t)| t).map_err(|e| {
        Refusal::new(
            what,
            format!("the request body does not read: {}", e.body_text()),
            "send the JSON the page sends",
        )
    })
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, Refusal> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        Refusal::new(
            "the working copy",
            e.to_string(),
            "report this with the dashboard's log",
        )
    })
}

// ── the working copy ────────────────────────────────────────────────────

async fn repo_status_json(c: &EditCtx) -> serde_json::Value {
    let wc = c.wc.clone();
    let status = blocking(move || wc.status()).await.unwrap_or_default();
    // arch-edit-txn: a stack deployed from a commit the remote lacks.
    let fleet: Vec<(String, Option<String>)> = c
        .shared
        .read()
        .await
        .fleet
        .as_ref()
        .map(|f| {
            f.stacks
                .iter()
                .map(|s| (s.name.clone(), s.applied_source.clone()))
                .collect()
        })
        .unwrap_or_default();
    let deployed_unpushed: Vec<serde_json::Value> = fleet
        .iter()
        .filter_map(|(name, src)| {
            let applied = src.as_deref().and_then(deployguard::applied_commit)?;
            let hit = status
                .unpushed
                .iter()
                .find(|u| u.commit.starts_with(applied))?;
            Some(serde_json::json!({ "stack": name, "commit": hit.commit }))
        })
        .collect();
    serde_json::json!({ "repo": status, "deployed_unpushed": deployed_unpushed })
}

async fn publish_repo(c: &EditCtx) {
    let v = repo_status_json(c).await;
    c.publish.publish("repo", v);
}

async fn repo_status(State(c): State<EditCtx>) -> Json<serde_json::Value> {
    Json(repo_status_json(&c).await)
}

async fn repo_sync(State(c): State<EditCtx>) -> Response {
    let wc = c.wc.clone();
    let r = blocking(move || wc.sync()).await.and_then(|r| r);
    publish_repo(&c).await;
    match r {
        Ok(()) => Json(repo_status_json(&c).await).into_response(),
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnpushedBody {
    choice: Unpushed,
}

async fn repo_unpushed(
    State(c): State<EditCtx>,
    b: Result<Json<UnpushedBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the unpushed commits") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    let wc = c.wc.clone();
    let r = blocking(move || wc.resolve(b.choice)).await.and_then(|r| r);
    publish_repo(&c).await;
    match r {
        Ok(()) => Json(repo_status_json(&c).await).into_response(),
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

// ── validation, shared by every plan and commit ─────────────────────────

/// Another stack's number and address, for the collision check.
#[derive(Debug, Clone)]
struct Other {
    name: String,
    vmid: u16,
    ip: String,
}

fn others(wc: &WorkingCopy, except: &str) -> Vec<Other> {
    wc.stack_names()
        .into_iter()
        .filter(|n| n != except)
        .filter_map(|n| {
            let m = homelab_client::spec::build_manifest(&wc.repo.join("stacks").join(&n)).ok()?;
            Some(Other {
                name: n,
                vmid: m.vmid,
                ip: bare_ip(&m.network.ip),
            })
        })
        .collect()
}

fn bare_ip(cidr: &str) -> String {
    cidr.split('/').next().unwrap_or(cidr).to_string()
}

/// What the staged stack directory holds, checked with the same functions
/// `homelab plan` and the deploy use.
struct Checked {
    problems: Vec<String>,
    manifest: Option<StackManifest>,
    /// Every file a deploy would send → sha256 (`homelab check`'s digest).
    digest: Option<BTreeMap<String, String>>,
}

fn check_dir(dir: &Path, stack: &str, others: &[Other]) -> Checked {
    let mut problems = Vec::new();
    let mut manifest = None;
    if dir.join(MANIFEST).exists() {
        match homelab_client::spec::build_manifest(dir) {
            Err(e) => problems.push(e),
            Ok(m) => {
                if m.stack_name != stack {
                    problems.push(format!(
                        "stack_name is {:?}, the directory is {stack:?}; they must be the same",
                        m.stack_name
                    ));
                }
                if let Err(e) = homelab_core::manifest::validate_manifest(&m) {
                    problems.push(e.to_string());
                }
                let gateway = homelab_core::safety::SafetyConfig::default().gateway_vmid;
                if m.vmid == gateway {
                    if let Some(fw) = m.firewall.as_ref().filter(|f| f.enabled) {
                        problems.extend(homelab_core::firewall::gateway_problems(fw));
                    }
                }
                for o in others {
                    if o.vmid == m.vmid {
                        problems.push(format!("vmid {} is {}'s already", m.vmid, o.name));
                    }
                    if o.ip == bare_ip(&m.network.ip) {
                        problems.push(format!("address {} is {}'s already", o.ip, o.name));
                    }
                }
                manifest = Some(m);
            }
        }
        if let Err(e) = homelab_client::spec::route_files(dir) {
            problems.push(e);
        }
    }
    for (rel, text) in super::workcopy::read_texts(dir) {
        if rel.ends_with("docker-compose.yml") || rel.ends_with(".yml") || rel.ends_with(".yaml") {
            if let Err(e) = serde_yaml::from_str::<serde_yaml::Value>(&text) {
                problems.push(format!("stacks/{stack}/{rel} does not read as YAML: {e}"));
            }
        }
        if rel.ends_with("service.yml") {
            match serde_yaml::from_str::<homelab_proto::NativeServiceManifest>(&text) {
                Ok(n) => {
                    if let Err(p) = homelab_core::native::validate_native(&n) {
                        problems.push(format!("stacks/{stack}/{rel}: {}", p.join("; ")));
                    }
                }
                Err(e) => problems.push(format!("stacks/{stack}/{rel}: {e}")),
            }
        }
        if rel.ends_with("checks.yml") {
            // serde_yaml's Display carries "at line N column M" when the
            // file has one, which is all the file/line the parser can give.
            // Schema only, same as service.yml above: a shortcoming (no
            // check reaching the application layer, a missing blind_spot)
            // is judged elsewhere (checks::shortcomings, the checks page)
            // and does not block a save here.
            if let Err(e) = serde_yaml::from_str::<homelab_core::checks::ServiceChecks>(&text) {
                problems.push(format!("stacks/{stack}/{rel}: {e}"));
            }
        }
    }
    let digest = homelab_client::spec::stack_digest(dir)
        .ok()
        .map(|d| d.files);
    Checked {
        problems,
        manifest,
        digest,
    }
}

// ── plans ───────────────────────────────────────────────────────────────

/// What the host last applied for a stack, against what a deploy of the
/// new files would send: `+ path`, `~ path`, `- path` (like `homelab
/// apply`). Read-only on the host.
async fn applied_changes(
    c: &EditCtx,
    stack: &str,
    digest: Option<&BTreeMap<String, String>>,
) -> serde_json::Value {
    let Some(digest) = digest else {
        return serde_json::json!({ "unavailable": "the stack's files could not be listed" });
    };
    let r = c
        .host
        .ask_traced(
            Command::GetApplied {
                stack: stack.to_string(),
            },
            Duration::from_secs(30),
            None,
        )
        .await;
    let applied: Vec<FileBlob> = match r {
        Ok(r) if r.ok => match serde_json::from_str(&r.message) {
            Ok(f) => f,
            Err(_) => return serde_json::json!({ "unavailable": "the host's answer did not read" }),
        },
        Ok(r) => {
            return serde_json::json!({ "unavailable": format!("the host answered: {}", r.message.chars().take(200).collect::<String>()) })
        }
        Err(e) => return serde_json::json!({ "unavailable": e }),
    };
    if applied.is_empty() {
        return serde_json::json!({ "never": true, "changes": [] });
    }
    let mut out = Vec::new();
    for (path, sha) in digest {
        match applied.iter().find(|a| &a.path == path) {
            None => out.push(format!("+ {path}")),
            Some(a) if &homelab_core::manifest::sha256_hex(a.content.as_bytes()) != sha => {
                out.push(format!("~ {path}"))
            }
            Some(_) => {}
        }
    }
    for a in &applied {
        if !digest.contains_key(&a.path) {
            out.push(format!("- {}", a.path));
        }
    }
    out.sort_by(|a, b| a[2..].cmp(&b[2..]));
    serde_json::json!({ "changes": out })
}

struct Planned {
    changes: Vec<FileChange>,
    checked: Checked,
    old: Option<StackManifest>,
    head: Option<String>,
    sync_error: Option<String>,
    /// The change in words, for the commit subject (`stackedit::describe`).
    summary: String,
}

fn plan_json(
    stack: &str,
    kind: &str,
    feature: &str,
    p: &Planned,
    applied: serde_json::Value,
) -> (serde_json::Value, Vec<Effect>, Vec<FileDiff>) {
    let diffs = editplan::file_diffs(&p.changes);
    let manifest_path = format!("stacks/{stack}/{MANIFEST}");
    let others: Vec<String> = p
        .changes
        .iter()
        .filter(|c| c.path != manifest_path)
        .map(|c| c.path.clone())
        .collect();
    let effects = match &p.checked.manifest {
        // tile-watch: `plan_json` is pure and has no host.toml fetch of its
        // own, so the plan does not yet show the derived tile-watch rule —
        // only the hand-declared ones, same as before that decision. See
        // the report for wiring the fleet-wide `tile_watch_source` in here.
        Some(new) => editplan::effects(p.old.as_ref(), new, &others, None),
        None => Vec::new(),
    };
    let summary = if p.summary.is_empty() {
        editplan::summary(kind, &diffs, &effects)
    } else {
        p.summary.clone()
    };
    let subject = editplan::commit_subject(stack, &summary, feature);
    let json = serde_json::json!({
        "stack": stack,
        "kind": kind,
        "head": p.head,
        "sync_error": p.sync_error,
        "files": diffs,
        "effects": effects,
        "follow_ups": editplan::follow_ups(&effects),
        "problems": p.checked.problems,
        "valid": p.checked.problems.is_empty() && !p.changes.is_empty(),
        "unchanged": p.changes.is_empty(),
        "applied": applied,
        "subject": subject,
        "restarts_dashboard": stack == SELF_STACK,
    });
    (json, effects, diffs)
}

/// The preset's apps as files of `stack` (feat-stacks-3, add an app).
fn add_app_files(
    wc: &WorkingCopy,
    stack: &str,
    preset: &str,
    vmid: u16,
) -> Result<AddAppFiles, Refusal> {
    let presets = homelab_client::scaffold::scan_presets(&wc.repo.join("presets"));
    let p = presets.iter().find(|p| p.name == preset).ok_or_else(|| {
        Refusal::new(
            "the app",
            format!("there is no preset {preset:?}"),
            "pick one the wizard lists",
        )
    })?;
    let Some(dir) = &p.dir else {
        return Err(Refusal::new(
            "the app",
            format!("the preset {preset} has no files"),
            "pick another",
        ));
    };
    if p.apps.is_empty() {
        return Err(Refusal::new(
            "the app",
            format!("the preset {preset} holds no app"),
            "pick another",
        ));
    }
    let ip =
        newstack::ip_for(vmid).unwrap_or_else(|| format!("10.10.10.{}", vmid.saturating_sub(100)));
    let mut out = AddAppFiles {
        apps: p.apps.clone(),
        owner_uid: homelab_client::scaffold::StackDefaults::default().appdata_owner_uid,
        ..Default::default()
    };
    let mut appdata = std::collections::BTreeSet::new();
    for app in &p.apps {
        for (rel, raw) in super::workcopy::read_texts(&dir.join(app)) {
            let content = homelab_client::scaffold::substitute(&raw, stack, vmid, &ip);
            if rel.ends_with("docker-compose.yml") {
                appdata.extend(homelab_client::scaffold::appdata_paths_in(&content));
            }
            out.files.insert(format!("{app}/{rel}"), content);
        }
    }
    out.appdata = appdata.into_iter().collect();
    Ok(out)
}

/// Everything a stack edit's plan and commit share, read under the working
/// copy's lock.
fn prepare(wc: &WorkingCopy, stack: &str, edit: &StackEdit) -> Result<Planned, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    let texts: StackTexts = wc.stack_texts(stack)?;
    let old = texts
        .get(MANIFEST)
        .and_then(|t| stackedit::parse_manifest(t).ok());
    let add = match edit {
        StackEdit::AddApp { preset, .. } => {
            let vmid = old.as_ref().map(|m| m.vmid).unwrap_or(0);
            Some(add_app_files(wc, stack, preset, vmid)?)
        }
        _ => None,
    };
    let changes = stackedit::changes(stack, &texts, edit, add.as_ref())?;
    let others = others(wc, stack);
    let checked = wc.with_staged(stack, &changes, |dir| check_dir(dir, stack, &others))?;
    let head = wc.status().head.map(|h| h.commit);
    let summary = stackedit::describe(edit, old.as_ref());
    Ok(Planned {
        changes,
        checked,
        old,
        head,
        sync_error,
        summary,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanBody {
    edit: StackEdit,
}

async fn stack_plan(
    State(c): State<EditCtx>,
    UrlPath(stack): UrlPath<String>,
    b: Result<Json<PlanBody>, JsonRejection>,
) -> Response {
    let b = match body(b, &format!("the plan for {stack}")) {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_stack(&c, &stack, b.edit).await)
}

/// A route's answer from a shared function's.
fn answer(r: Result<serde_json::Value, (StatusCode, Refusal)>) -> Response {
    match r {
        Ok(v) => Json(v).into_response(),
        Err((status, r)) => refusal(status, r),
    }
}

/// feat-stacks-2: the plan of one stack's edit. The route and a driven
/// `next` (feat-platform-10) both come here.
pub async fn plan_stack(
    c: &EditCtx,
    stack: &str,
    edit: StackEdit,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    if !actions::valid_stack_name(stack) {
        return Err((
            StatusCode::BAD_REQUEST,
            Refusal::new(
                "the plan",
                "not a stack name",
                "use the name the fleet page shows",
            ),
        ));
    }
    let wc = c.wc.clone();
    let (s2, e2) = (stack.to_string(), edit.clone());
    let planned = blocking(move || prepare(&wc, &s2, &e2))
        .await
        .and_then(|r| r)
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let applied = applied_changes(c, stack, planned.checked.digest.as_ref()).await;
    let (json, _, _) = plan_json(stack, edit.kind(), edit.feature(), &planned, applied);
    Ok(json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitBody {
    edit: StackEdit,
    /// The first line; the default is the plan's. A missing feature id is
    /// added (standing rule 4).
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    note: Option<String>,
    /// After the commit: `deploy` (exactly that commit) or `resize`.
    #[serde(default)]
    follow: Option<String>,
}

/// The subject as typed, with the feature's id added when it names none.
pub fn subject_with_id(typed: Option<&str>, default: &str, feature: &str) -> String {
    match typed.map(str::trim).filter(|s| !s.is_empty()) {
        None => default.to_string(),
        Some(s) => {
            let one_line: String = s.lines().next().unwrap_or("").chars().take(150).collect();
            if one_line.contains('[') && one_line.contains(']') {
                one_line
            } else {
                format!("{one_line} [{feature}]")
            }
        }
    }
}

/// Queue the follow-up of a commit (the act job machinery).
fn follow_up(
    c: &EditCtx,
    stack: &str,
    follow: Option<&str>,
    commit: &str,
    origin: Origin,
) -> serde_json::Value {
    let kind = match follow {
        None | Some("") | Some("none") => return serde_json::Value::Null,
        Some("deploy") => ActionKind::DeployCommit,
        Some("resize") => ActionKind::Resize,
        Some(other) => {
            return serde_json::json!({ "refused": Refusal::new("the follow-up", format!("{other:?} is not deploy or resize"), "pick one the plan offers") })
        }
    };
    let args = ActionArgs {
        commit: (kind == ActionKind::DeployCommit).then(|| commit.to_string()),
        ..Default::default()
    };
    let req = match actions::validate(stack, kind.slug(), args) {
        Ok(r) => r,
        Err(r) => return serde_json::json!({ "refused": r }),
    };
    if let Err(r) = c.actions.precheck(&req) {
        return serde_json::json!({ "refused": r });
    }
    let job = c.actions.submit(req, origin);
    serde_json::json!({ "job": job.job, "action": job.action, "restarts_dashboard": job.restarts_dashboard })
}

async fn stack_commit(
    State(c): State<EditCtx>,
    UrlPath(stack): UrlPath<String>,
    b: Result<Json<CommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, &format!("the commit to {stack}")) {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_stack(&c, &stack, b, Origin::Manual).await)
}

/// feat-stacks-2: the commit and push of one stack's edit, then its
/// follow-up. The route and a driven `confirm` (feat-platform-10) both
/// come here: one transaction, whoever pressed.
pub async fn commit_stack(
    c: &EditCtx,
    stack: &str,
    b: CommitBody,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    if !actions::valid_stack_name(stack) {
        return Err((
            StatusCode::BAD_REQUEST,
            Refusal::new(
                "the commit",
                "not a stack name",
                "use the name the fleet page shows",
            ),
        ));
    }
    let stack = stack.to_string();
    let wc = c.wc.clone();
    let (s2, edit) = (stack.clone(), b.edit.clone());
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || {
        let planned = prepare(&wc, &s2, &edit)?;
        if !planned.checked.problems.is_empty() {
            return Err(Refusal::new(
                format!("the commit to stacks/{s2}"),
                planned.checked.problems.join("; "),
                "correct the edit; nothing was written",
            ));
        }
        let (plan, effects, diffs) = plan_json(
            &s2,
            edit.kind(),
            edit.feature(),
            &planned,
            serde_json::Value::Null,
        );
        let default = plan["subject"].as_str().unwrap_or_default().to_string();
        let subject = subject_with_id(subject_in.as_deref(), &default, edit.feature());
        let message = editplan::commit_message(&subject, &note, &effects, &diffs);
        let others = others(&wc, &s2);
        let stack3 = s2.clone();
        wc.transact(&s2, &planned.changes, &message, move |dir| {
            check_dir(dir, &stack3, &others).problems
        })
    })
    .await
    .and_then(|r| r);
    publish_repo(c).await;
    match done {
        Ok(committed) => {
            let follow = follow_up(c, &stack, b.follow.as_deref(), &committed.commit, origin);
            Ok(serde_json::json!({ "committed": committed, "follow": follow }))
        }
        Err(r) => Err((StatusCode::CONFLICT, r)),
    }
}

/// The editor's first read: the stack's files, its parsed settings and
/// firewall, its images, and the presets an app can come from.
async fn stack_edit(State(c): State<EditCtx>, UrlPath(stack): UrlPath<String>) -> Response {
    answer(read_stack_edit(&c, &stack).await)
}

/// The editor's first read, for the route and a driven `open`.
pub async fn read_stack_edit(
    c: &EditCtx,
    stack: &str,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    if !actions::valid_stack_name(stack) {
        return Err((
            StatusCode::BAD_REQUEST,
            Refusal::new(
                "the editor",
                "not a stack name",
                "use the name the fleet page shows",
            ),
        ));
    }
    let wc = c.wc.clone();
    let s2 = stack.to_string();
    let read = blocking(move || {
        let sync_error = wc.sync().err().map(|r| r.why);
        let texts = wc.stack_texts(&s2)?;
        let presets = homelab_client::scaffold::scan_presets(&wc.repo.join("presets"));
        let head = wc.status().head;
        Ok::<_, Refusal>((texts, presets, head, sync_error))
    })
    .await
    .and_then(|r| r);
    let (texts, presets, head, sync_error) = read.map_err(|r| (StatusCode::CONFLICT, r))?;
    let manifest = texts.get(MANIFEST).map(|t| stackedit::parse_manifest(t));
    let (manifest_json, manifest_error, native_details) = match manifest {
        Some(Ok(m)) => (
            serde_json::json!({
                "vmid": m.vmid,
                "hostname": m.hostname,
                "ip": m.network.ip,
                "resources": m.resources,
                "boot": m.boot,
                "protection": m.lxc.protection,
                "apps": m.apps,
                "natives": m.natives,
                "native_only": m.native_only,
                "firewall": m.firewall,
                "tiles": m.tiles,
                // feat-stacks-9/10/11 (Area C): the settings-extension,
                // apps/storage and latch forms' own read side.
                "network": m.network,
                "lxc": m.lxc,
                "on_demand": m.on_demand,
                "storage": m.storage,
                "data_mounts": m.data_mounts,
                "log_files": m.log_files,
                "retention": m.retention,
                "latch": texts.get(MANIFEST).map(|t| stackedit_latch::current(t)).unwrap_or_default(),
                // feat-checks-1/feat-tiles-1 (Area B): the checks and
                // tiles forms' own read side. Tiles already reads from
                // `m.tiles` above; each app's checks.yml is per-app, not
                // part of the manifest, so it needs its own read.
                "checks": stackedit_checks::checks_by_app(&texts, &m.apps),
            }),
            None,
            // feat-native-1: each declared native unit's own service.yml,
            // parsed, so the form can prefill without a YAML parser in the
            // browser.
            stackedit_native::native_views(&texts, &m.natives),
        ),
        Some(Err(e)) => (serde_json::Value::Null, Some(e), Vec::new()),
        None => (
            serde_json::Value::Null,
            Some(format!("stacks/{stack} has no {MANIFEST}")),
            Vec::new(),
        ),
    };
    Ok(serde_json::json!({
        "stack": stack,
        "head": head,
        "sync_error": sync_error,
        "texts": texts,
        "manifest": manifest_json,
        "manifest_error": manifest_error,
        "images": stackedit::images(&texts),
        "file_templates": stackedit_files::templates(),
        "self_stack": SELF_STACK,
        "natives": native_details,
        "presets": presets.iter().filter(|p| p.dir.is_some() && !p.apps.is_empty()).map(|p| serde_json::json!({
            "name": p.name, "description": p.meta.description, "apps": p.apps,
        })).collect::<Vec<_>>(),
    }))
}

// ── feat-stacks-3: a new stack ──────────────────────────────────────────

async fn taken(c: &EditCtx) -> Taken {
    let mut t = Taken::default();
    if let Some(f) = c.shared.read().await.fleet.as_ref() {
        for s in &f.stacks {
            t.names.insert(s.name.clone());
            t.vmids.insert(s.vmid);
        }
    }
    let wc = c.wc.clone();
    let repo = blocking(move || others(&wc, "")).await.unwrap_or_default();
    let names = {
        let wc = c.wc.clone();
        blocking(move || wc.stack_names()).await.unwrap_or_default()
    };
    t.names.extend(names);
    for o in repo {
        t.vmids.insert(o.vmid);
        t.ips.insert(o.ip);
    }
    // Single addresses the firewalls name (Kenny's desktop, the router):
    // in use on the LAN even though no stack has them.
    let wc = c.wc.clone();
    let peers = blocking(move || {
        let mut out = Vec::new();
        for n in wc.stack_names() {
            let Ok(m) = homelab_client::spec::build_manifest(&wc.repo.join("stacks").join(&n))
            else {
                continue;
            };
            for r in m.firewall.iter().flat_map(|f| f.rules.iter()) {
                for a in [&r.source, &r.dest].into_iter().flatten() {
                    if !a.contains('/') {
                        out.push(a.clone());
                    }
                }
            }
        }
        out
    })
    .await
    .unwrap_or_default();
    t.ips.extend(peers);
    t
}

async fn presets(State(c): State<EditCtx>) -> Response {
    Json(read_presets(&c).await).into_response()
}

/// The new-stack wizard's first read, for the route and a driven `open`.
pub async fn read_presets(c: &EditCtx) -> serde_json::Value {
    let wc = c.wc.clone();
    // Only the repository's presets: without a working copy the scaffold's
    // built-in fallbacks would be offered as if they were Kenny's.
    let (list, sync_error) = blocking(move || {
        let sync_error = wc.sync().err().map(|r| r.why);
        let list = if wc.present() {
            homelab_client::scaffold::scan_presets(&wc.repo.join("presets"))
                .into_iter()
                .filter(|p| p.dir.is_some())
                .collect()
        } else {
            Vec::new()
        };
        (list, sync_error)
    })
    .await
    .unwrap_or_default();
    let t = taken(c).await;
    let d = homelab_client::scaffold::StackDefaults::default();
    serde_json::json!({
        "presets": list.iter().map(|p| serde_json::json!({
            "name": p.name,
            "description": p.meta.description,
            "ram_mb": p.meta.ram_mb,
            "cores": p.meta.cores.unwrap_or(d.default_cores),
            "disk_gb": p.meta.disk_gb.unwrap_or(d.default_disk_gb),
            "apps": p.apps,
            "gpu": p.meta.gpu,
            "vpn": p.meta.vpn,
        })).collect::<Vec<_>>(),
        "suggest_vmid": newstack::suggest_vmid(&t),
        "taken": { "names": t.names, "vmids": t.vmids },
        "swap": { "divisor": d.swap_divisor, "min_mb": d.swap_min_mb, "max_mb": d.swap_max_mb },
        "working_copy": c.wc.present(),
        "sync_error": sync_error,
    })
}

// ── feat-preset-1 (D): editing presets/ itself ──────────────────────────

async fn preset_edit(State(c): State<EditCtx>, UrlPath(name): UrlPath<String>) -> Response {
    answer(read_preset_edit(&c, &name).await)
}

/// The presets editor's first read for one preset: its `preset.yml`
/// (parsed) and every other file it holds. `exists: false` with empty
/// files is a name the working copy does not have yet — the editor's "new
/// preset" case, not an error.
pub async fn read_preset_edit(
    c: &EditCtx,
    name: &str,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let name = presetedit::valid_name(name).map_err(|r| (StatusCode::BAD_REQUEST, r))?;
    let wc = c.wc.clone();
    let (texts, head, sync_error) = blocking(move || {
        let sync_error = wc.sync().err().map(|r| r.why);
        (wc.preset_texts(), wc.status().head, sync_error)
    })
    .await
    .unwrap_or_default();
    let prefix = format!("{name}/");
    let files: BTreeMap<String, String> = texts
        .iter()
        .filter(|(k, _)| k.starts_with(&prefix))
        .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone()))
        .collect();
    let meta = files
        .get(presetedit::PRESET_META)
        .and_then(|t| serde_yaml::from_str::<homelab_client::scaffold::PresetMeta>(t).ok());
    Ok(serde_json::json!({
        "name": name,
        "head": head,
        "sync_error": sync_error,
        "exists": !files.is_empty(),
        "meta": meta,
        "files": files,
        // Area A's file templates fit here too: a preset's app files are
        // the same shapes a stack's app files are.
        "file_templates": stackedit_files::templates(),
    }))
}

struct PlannedPreset {
    changes: Vec<FileChange>,
    problems: Vec<String>,
    head: Option<String>,
    sync_error: Option<String>,
}

fn prepare_preset(
    wc: &WorkingCopy,
    edit: &presetedit::PresetEdit,
) -> Result<PlannedPreset, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    let texts = wc.preset_texts();
    let changes = presetedit::changes(&texts, edit)?;
    let problems = wc.with_staged_presets(&changes, check_presets_dir)?;
    let head = wc.status().head.map(|h| h.commit);
    Ok(PlannedPreset {
        changes,
        problems,
        head,
        sync_error,
    })
}

/// Every `preset.yml` under the staged `presets/` reads back as a preset —
/// the same net `check_dir` casts over a stack's `service.yml` files.
fn check_presets_dir(dir: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return problems;
    };
    for e in entries.flatten() {
        if !e.path().is_dir() {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let meta_path = e.path().join(presetedit::PRESET_META);
        if !meta_path.exists() {
            continue;
        }
        let ok = std::fs::read_to_string(&meta_path)
            .ok()
            .and_then(|t| serde_yaml::from_str::<homelab_client::scaffold::PresetMeta>(&t).ok())
            .is_some();
        if !ok {
            problems.push(format!(
                "presets/{name}/preset.yml does not read as a preset"
            ));
        }
    }
    problems
}

/// The default commit subject for a preset edit (`subject_with_id`'s
/// input): `commit_subject` always prefixes `stacks/`, which is wrong here.
fn preset_default_subject(edit: &presetedit::PresetEdit) -> String {
    format!(
        "presets/{}: {}",
        edit.preset_name(),
        presetedit::describe(edit)
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetPlanBody {
    edit: presetedit::PresetEdit,
}

async fn presets_plan(
    State(c): State<EditCtx>,
    b: Result<Json<PresetPlanBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the plan for a preset") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_preset(&c, b.edit).await)
}

pub async fn plan_preset(
    c: &EditCtx,
    edit: presetedit::PresetEdit,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let wc = c.wc.clone();
    let e2 = edit.clone();
    let planned = blocking(move || prepare_preset(&wc, &e2))
        .await
        .and_then(|r| r)
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let diffs = editplan::file_diffs(&planned.changes);
    let subject = subject_with_id(None, &preset_default_subject(&edit), "feat-preset-1");
    Ok(serde_json::json!({
        "stack": format!("presets/{}", edit.preset_name()),
        "kind": edit.kind(),
        "head": planned.head,
        "sync_error": planned.sync_error,
        "files": diffs,
        // A preset commit deploys nothing by itself (feat-stacks-3 reads
        // it later, when a stack adds its app) — the plan dialog's `Plan`
        // shape still wants these, empty rather than absent.
        "effects": Vec::<serde_json::Value>::new(),
        "follow_ups": Vec::<String>::new(),
        "applied": serde_json::Value::Null,
        "restarts_dashboard": false,
        "problems": planned.problems,
        "valid": planned.problems.is_empty() && !planned.changes.is_empty(),
        "unchanged": planned.changes.is_empty(),
        "subject": subject,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PresetCommitBody {
    pub edit: presetedit::PresetEdit,
    #[serde(default)]
    pub subject: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

async fn presets_commit(
    State(c): State<EditCtx>,
    b: Result<Json<PresetCommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the commit to a preset") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_preset(&c, b).await)
}

pub async fn commit_preset(
    c: &EditCtx,
    b: PresetCommitBody,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let wc = c.wc.clone();
    let edit = b.edit.clone();
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || {
        let planned = prepare_preset(&wc, &edit)?;
        if !planned.problems.is_empty() {
            return Err(Refusal::new(
                "the commit to presets/",
                planned.problems.join("; "),
                "correct the edit; nothing was written",
            ));
        }
        if planned.changes.is_empty() {
            return Err(Refusal::new(
                "the commit to presets/",
                "nothing changes",
                "change something first",
            ));
        }
        let diffs = editplan::file_diffs(&planned.changes);
        let default = preset_default_subject(&edit);
        let subject = subject_with_id(subject_in.as_deref(), &default, "feat-preset-1");
        let message = editplan::commit_message(&subject, &note, &[], &diffs);
        wc.transact_presets(&planned.changes, &message, check_presets_dir)
    })
    .await
    .and_then(|r| r);
    publish_repo(c).await;
    match done {
        Ok(committed) => Ok(serde_json::json!({ "committed": committed })),
        Err(r) => Err((StatusCode::CONFLICT, r)),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppdataBody {
    preset: String,
    name: String,
    vmid: u16,
}

async fn new_appdata(
    State(c): State<EditCtx>,
    b: Result<Json<AppdataBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the new stack's data folders") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    Json(appdata_paths(&c, b).await).into_response()
}

/// The `/appdata` folders a new stack's preset binds.
pub async fn appdata_paths(c: &EditCtx, b: AppdataBody) -> serde_json::Value {
    let wc = c.wc.clone();
    let paths = blocking(move || {
        let base = wc.repo.join("presets");
        let list = homelab_client::scaffold::scan_presets(&base);
        let p = list.iter().find(|p| p.name == b.preset).cloned();
        homelab_client::scaffold::preview_appdata_paths(&base, p.as_ref(), &b.name, b.vmid)
    })
    .await
    .unwrap_or_default();
    serde_json::json!({ "appdata": paths })
}

fn scaffold_new(wc: &WorkingCopy, req: &NewStack) -> Result<Vec<FileChange>, Refusal> {
    let presets_base = wc.repo.join("presets");
    let list = homelab_client::scaffold::scan_presets(&presets_base);
    let preset = list.iter().find(|p| p.name == req.preset).cloned();
    let req = req.clone();
    let name = req.name.clone();
    wc.scaffolded(&name, move |base| {
        homelab_client::scaffold::scaffold_stack(
            base,
            &presets_base,
            &homelab_client::scaffold::StackParams {
                name: &req.name,
                vmid: req.vmid,
                ram_mb: req.ram_mb,
                cores: req.cores,
                disk_gb: req.disk_gb,
                swap_mb: req.swap_mb,
                preset: preset.as_ref(),
                no_data_paths: &req.no_data,
            },
        )
        .map(|_| ())
    })
}

/// feat-tiles-3: the wizard's own optional Tile step, applied to the
/// freshly scaffolded manifest in the very same commit — the same
/// `Op::Set` at `tiles.<hostname>` `StackEdit::PublishApp`'s and
/// `StackEdit::AddApp`'s own tile steps use, rather than a second,
/// best-effort `StackEdit::Tiles` commit once the stack already exists.
fn apply_new_stack_tile(
    req: &NewStack,
    mut changes: Vec<FileChange>,
) -> Result<Vec<FileChange>, Refusal> {
    let Some(tile) = &req.tile else {
        return Ok(changes);
    };
    let rel = format!("stacks/{}/{MANIFEST}", req.name);
    let Some(entry) = changes.iter_mut().find(|c| c.path == rel) else {
        return Err(Refusal::new(
            "the new stack's tile",
            format!("{rel} was not scaffolded"),
            "report this with the dashboard's log",
        ));
    };
    let text = entry.new.clone().unwrap_or_default();
    let ops = stackedit_tiles::set_tile_ops(&text, &tile.hostname, &tile.fields());
    let new = yamledit::edit(&text, &ops).map_err(|e| {
        Refusal::new(
            "the new stack's tile",
            format!("{rel}: {e}"),
            "correct the tile step's values",
        )
    })?;
    entry.new = Some(new);
    Ok(changes)
}

fn prepare_new(wc: &WorkingCopy, req: &NewStack, taken: &Taken) -> Result<Planned, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    let names: Vec<String> = homelab_client::scaffold::scan_presets(&wc.repo.join("presets"))
        .into_iter()
        .map(|p| p.name)
        .collect();
    let problems = newstack::problems(req, taken, &names);
    if !problems.is_empty() {
        return Err(Refusal::new(
            "the new stack",
            problems
                .iter()
                .map(|(_, w)| w.as_str())
                .collect::<Vec<_>>()
                .join("; "),
            "correct the marked fields in the wizard",
        ));
    }
    let changes = scaffold_new(wc, req)?;
    let changes = apply_new_stack_tile(req, changes)?;
    let others = others(wc, &req.name);
    let name = req.name.clone();
    let checked = wc.with_staged(&req.name, &changes, |dir| check_dir(dir, &name, &others))?;
    Ok(Planned {
        changes,
        checked,
        old: None,
        head: wc.status().head.map(|h| h.commit),
        sync_error,
        summary: format!("new stack from the {} preset (CT {})", req.preset, req.vmid),
    })
}

async fn new_plan(State(c): State<EditCtx>, b: Result<Json<NewStack>, JsonRejection>) -> Response {
    let req = match body(b, "the new stack") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_new(&c, req).await)
}

/// feat-stacks-3: the new stack's plan, for the route and a driven wizard.
pub async fn plan_new(
    c: &EditCtx,
    req: NewStack,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let t = taken(c).await;
    let wc = c.wc.clone();
    let r2 = req.clone();
    let planned = blocking(move || prepare_new(&wc, &r2, &t))
        .await
        .and_then(|r| r)
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let (json, _, _) = plan_json(
        &req.name,
        "new",
        "feat-stacks-3",
        &planned,
        serde_json::json!({ "never": true, "changes": [] }),
    );
    Ok(json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewCommitBody {
    stack: NewStack,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    follow: Option<String>,
}

async fn new_commit(
    State(c): State<EditCtx>,
    b: Result<Json<NewCommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the new stack") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_new(&c, b, Origin::Manual).await)
}

/// feat-stacks-3: the new stack's commit and push, then its first deploy
/// when asked; for the route and a driven wizard's final press.
pub async fn commit_new(
    c: &EditCtx,
    b: NewCommitBody,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let t = taken(c).await;
    let wc = c.wc.clone();
    let req = b.stack.clone();
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || {
        let planned = prepare_new(&wc, &req, &t)?;
        if !planned.checked.problems.is_empty() {
            return Err(Refusal::new(
                format!("the new stack {}", req.name),
                planned.checked.problems.join("; "),
                "correct the wizard's values; nothing was written",
            ));
        }
        let (_, effects, diffs) = plan_json(
            &req.name,
            "new",
            "feat-stacks-3",
            &planned,
            serde_json::Value::Null,
        );
        let default = editplan::commit_subject(&req.name, &planned.summary, "feat-stacks-3");
        let subject = subject_with_id(subject_in.as_deref(), &default, "feat-stacks-3");
        let message = editplan::commit_message(&subject, &note, &effects, &diffs);
        let others = others(&wc, &req.name);
        let name = req.name.clone();
        wc.transact(&req.name, &planned.changes, &message, move |dir| {
            check_dir(dir, &name, &others).problems
        })
    })
    .await
    .and_then(|r| r);
    publish_repo(c).await;
    match done {
        Ok(committed) => {
            let follow = follow_up(
                c,
                &b.stack.name,
                b.follow.as_deref(),
                &committed.commit,
                origin,
            );
            Ok(serde_json::json!({ "committed": committed, "follow": follow }))
        }
        Err(r) => Err((StatusCode::CONFLICT, r)),
    }
}

// ── TUI parity: import a bundle as a new stack ─────────────────────────

/// `homelab import <bundle.yml> <new-name> <vmid>` in the browser: the
/// bundle's text (an export, never with secrets), the new stack's name and
/// container number.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportReq {
    pub bundle: String,
    pub name: String,
    pub vmid: u16,
}

/// The largest bundle the page sends (the whole repository's stacks are a
/// few hundred KiB).
pub const IMPORT_MAX: usize = 2 * 1024 * 1024;

fn prepare_import(wc: &WorkingCopy, req: &ImportReq, taken: &Taken) -> Result<Planned, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    let what = format!("the import of {}", req.name);
    let problems = newstack::identity_problems(&req.name, req.vmid, taken);
    if !problems.is_empty() {
        return Err(Refusal::new(
            what,
            problems
                .iter()
                .map(|(_, w)| w.as_str())
                .collect::<Vec<_>>()
                .join("; "),
            "correct the name or the container number",
        ));
    }
    if req.bundle.len() > IMPORT_MAX {
        return Err(Refusal::new(
            what,
            format!("the bundle is larger than {} MiB", IMPORT_MAX / 1024 / 1024),
            "export one stack at a time",
        ));
    }
    let files =
        homelab_client::spec::imported_files(&req.bundle, &req.name, req.vmid).map_err(|why| {
            Refusal::new(
                what.clone(),
                why,
                "paste or upload the YAML an export wrote (Export bundle, or homelab export)",
            )
        })?;
    // A bundle never carries secrets; one that names a .env is refused
    // rather than committed.
    if let Some((p, _)) = files.iter().find(|(p, _)| {
        p.rsplit('/')
            .next()
            .is_some_and(|n| n == ".env" || n.ends_with(".env"))
    }) {
        return Err(Refusal::new(
            what,
            format!("the bundle carries {p}, a secrets file"),
            "secrets go through latch, never into the repository",
        ));
    }
    let changes: Vec<FileChange> = files
        .into_iter()
        .map(|(rel, text)| FileChange {
            path: format!("stacks/{}/{rel}", req.name),
            old: None,
            new: Some(text),
        })
        .collect();
    let others = others(wc, &req.name);
    let name = req.name.clone();
    let checked = wc.with_staged(&req.name, &changes, |dir| check_dir(dir, &name, &others))?;
    Ok(Planned {
        changes,
        checked,
        old: None,
        head: wc.status().head.map(|h| h.commit),
        sync_error,
        summary: format!(
            "import a bundle as the new stack {} (CT {})",
            req.name, req.vmid
        ),
    })
}

/// The import's plan, for the route and a driven import form.
pub async fn plan_import(
    c: &EditCtx,
    req: ImportReq,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let t = taken(c).await;
    let wc = c.wc.clone();
    let r2 = req.clone();
    let planned = blocking(move || prepare_import(&wc, &r2, &t))
        .await
        .and_then(|r| r)
        .map_err(|r| (StatusCode::CONFLICT, r))?;
    let (json, _, _) = plan_json(
        &req.name,
        "import",
        "feat-stacks-3",
        &planned,
        serde_json::json!({ "never": true, "changes": [] }),
    );
    Ok(json)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportCommitBody {
    edit: ImportReq,
    #[serde(default)]
    subject: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    follow: Option<String>,
}

/// The import's commit and push, then its first deploy when asked; for the
/// route and a driven import's final press.
pub async fn commit_import(
    c: &EditCtx,
    b: ImportCommitBody,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let t = taken(c).await;
    let wc = c.wc.clone();
    let req = b.edit.clone();
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || {
        let planned = prepare_import(&wc, &req, &t)?;
        if !planned.checked.problems.is_empty() {
            return Err(Refusal::new(
                format!("the import of {}", req.name),
                planned.checked.problems.join("; "),
                "correct the bundle, the name or the number; nothing was written",
            ));
        }
        let (_, effects, diffs) = plan_json(
            &req.name,
            "import",
            "feat-stacks-3",
            &planned,
            serde_json::Value::Null,
        );
        let default = editplan::commit_subject(&req.name, &planned.summary, "feat-stacks-3");
        let subject = subject_with_id(subject_in.as_deref(), &default, "feat-stacks-3");
        let message = editplan::commit_message(&subject, &note, &effects, &diffs);
        let others = others(&wc, &req.name);
        let name = req.name.clone();
        wc.transact(&req.name, &planned.changes, &message, move |dir| {
            check_dir(dir, &name, &others).problems
        })
    })
    .await
    .and_then(|r| r);
    publish_repo(c).await;
    match done {
        Ok(committed) => {
            let follow = follow_up(
                c,
                &b.edit.name,
                b.follow.as_deref(),
                &committed.commit,
                origin,
            );
            Ok(serde_json::json!({ "committed": committed, "follow": follow }))
        }
        Err(r) => Err((StatusCode::CONFLICT, r)),
    }
}

async fn import_plan(
    State(c): State<EditCtx>,
    b: Result<Json<ImportReq>, JsonRejection>,
) -> Response {
    let req = match body(b, "the import") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_import(&c, req).await)
}

async fn import_commit(
    State(c): State<EditCtx>,
    b: Result<Json<ImportCommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the import") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_import(&c, b, Origin::Manual).await)
}

// ── feat-firewall-2 ─────────────────────────────────────────────────────

async fn firewall(State(c): State<EditCtx>) -> Response {
    let wc = c.wc.clone();
    let read = blocking(move || {
        if !wc.present() {
            return Err(Refusal::new(
                "the firewall page",
                "the dashboard has no working copy of the homelab repository yet",
                "the working copy panel says why; it clones at start once the deploy key is there",
            ));
        }
        let mut fleet = Vec::new();
        let mut summaries = Vec::new();
        for name in wc.stack_names() {
            let Ok(texts) = wc.stack_texts(&name) else {
                continue;
            };
            let Some(Ok(m)) = texts.get(MANIFEST).map(|t| stackedit::parse_manifest(t)) else {
                continue;
            };
            let Ok(ip) = bare_ip(&m.network.ip).parse() else {
                continue;
            };
            summaries.push(serde_json::json!({
                "stack": name, "vmid": m.vmid, "ip": bare_ip(&m.network.ip),
                "declared": m.firewall.is_some(),
                "enabled": m.firewall.as_ref().is_some_and(|f| f.enabled),
                "policy_in": m.firewall.as_ref().map(|f| stackedit::action_word(f.policy_in)),
                "policy_out": m.firewall.as_ref().map(|f| stackedit::action_word(f.policy_out)),
                "rules": m.firewall.as_ref().map(|f| f.rules.len()).unwrap_or(0),
                "management_open": m.firewall.as_ref().and_then(|f| f.management_open.clone()),
            }));
            fleet.push(FleetFirewall {
                stack: name,
                vmid: m.vmid,
                ip,
                firewall: m.firewall,
            });
        }
        let head = wc.status().head;
        Ok((fwmatrix::matrix(&fleet), summaries, head))
    })
    .await
    .and_then(|r| r);
    match read {
        Ok((m, summaries, head)) => {
            Json(serde_json::json!({ "matrix": m, "stacks": summaries, "head": head }))
                .into_response()
        }
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

// ── feat-settings-1 ─────────────────────────────────────────────────────

/// `3.62.2` or `v3.62.2-4-gabc` as numbers.
pub use crate::core::hostversion::version_triple;

async fn host_new_enough(c: &EditCtx) -> Result<(), Refusal> {
    let v = c.shared.read().await.host_version.clone();
    match v.as_deref().and_then(version_triple) {
        Some(t) if t >= HOST_SETTINGS_SINCE => Ok(()),
        Some(_) => Err(Refusal::new(
            "the host settings",
            format!(
                "the host runs {} and answers host.toml only from {}.{}.{} on",
                v.unwrap_or_default(),
                HOST_SETTINGS_SINCE.0,
                HOST_SETTINGS_SINCE.1,
                HOST_SETTINGS_SINCE.2
            ),
            "update the host to the release that carries feat-settings-1; until then the TUI's settings screen edits the three live keys",
        )),
        None => Err(Refusal::new(
            "the host settings",
            "the dashboard has not heard the host yet",
            "wait for the link to the host; the top bar shows it",
        )),
    }
}

async fn host_settings(State(c): State<EditCtx>) -> Response {
    answer(read_host_settings(&c).await)
}

/// feat-settings-1: host.toml as the page shows it, for the route and a
/// driven `open host-settings`.
pub async fn read_host_settings(c: &EditCtx) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    host_new_enough(c)
        .await
        .map_err(|r| (StatusCode::SERVICE_UNAVAILABLE, r))?;
    let r = c
        .host
        .ask_traced(Command::GetHostConfig, Duration::from_secs(20), None)
        .await;
    match r {
        Ok(r) if r.ok => match serde_json::from_str::<homelab_proto::HostConfigFile>(&r.message) {
            Ok(file) => Ok(
                serde_json::json!({ "page": hostsettings::page(&file), "measured_at": super::host_link::now_s() }),
            ),
            Err(e) => Err((
                StatusCode::BAD_GATEWAY,
                Refusal::new(
                    "the host settings",
                    format!("the host's answer did not read: {e}"),
                    "update the host and the dashboard to the same release",
                ),
            )),
        },
        Ok(r) => Err((
            StatusCode::BAD_GATEWAY,
            Refusal::new("the host settings", r.message, "look at host.toml on pve"),
        )),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Refusal::new(
                "the host settings",
                e,
                "check that the host answers (homelab ping)",
            ),
        )),
    }
}

async fn host_settings_save(
    State(c): State<EditCtx>,
    b: Result<Json<hostsettings::Change>, JsonRejection>,
) -> Response {
    let change = match body(b, "the host settings") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(save_host_settings(&c, change, Origin::Manual).await)
}

/// feat-settings-1: write host.toml, for the route and a driven final
/// press: the same check, the same session-only command, once.
///
/// Owner decision 2026-09-30 (item 2): when any key just written takes
/// effect only at the host's next start (`Apply::Restart`), the answer
/// also carries a `follow` job that restarts `homelab-host.service` — the
/// same shape `stack_commit`'s `follow_up` gives a deploy, so the generic
/// driven-form job wiring in `shell::drive` picks it up unchanged.
pub async fn save_host_settings(
    c: &EditCtx,
    change: hostsettings::Change,
    origin: Origin,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    host_new_enough(c)
        .await
        .map_err(|r| (StatusCode::SERVICE_UNAVAILABLE, r))?;
    let expect = change.expect_sha256.clone();
    let changes = hostsettings::check(change).map_err(|r| (StatusCode::BAD_REQUEST, r))?;
    let keys: Vec<String> = changes.keys().cloned().collect();
    let r = c
        .host
        .ask_traced(
            Command::SetHostConfig {
                changes,
                expect_sha256: expect,
            },
            Duration::from_secs(30),
            None,
        )
        .await;
    match r {
        Ok(r) if r.ok => {
            let saved: serde_json::Value = serde_json::from_str(&r.message).unwrap_or_default();
            c.publish.publish(
                "host_settings",
                serde_json::json!({ "saved": saved, "keys": keys }),
            );
            let follow = restart_follow_up(c, &keys, origin);
            Ok(serde_json::json!({ "saved": saved, "follow": follow }))
        }
        Ok(r) => Err((
            StatusCode::CONFLICT,
            Refusal::new(
                "the host settings",
                r.message,
                "nothing was written; correct the change or reload the page",
            ),
        )),
        Err(e) => Err((
            StatusCode::BAD_GATEWAY,
            Refusal::new(
                "the host settings",
                format!("{e}; whether the host wrote it is unknown"),
                "reload the page: it shows host.toml as it is now",
            ),
        )),
    }
}

/// Owner decision 2026-09-30 (item 2): a key just written whose
/// `Apply::Restart` means the host only reads it at its next start;
/// queues `restart-host` so "Save and restart the host" does both in one
/// press. `null` when nothing written needs it.
fn restart_follow_up(c: &EditCtx, keys: &[String], origin: Origin) -> serde_json::Value {
    use homelab_core::hostconfig::{key_info, Apply};
    let needs_restart = keys
        .iter()
        .any(|k| key_info(k).is_some_and(|i| i.apply == Apply::Restart));
    if !needs_restart {
        return serde_json::Value::Null;
    }
    let req = match actions::validate(
        actions::HOST_TARGET,
        ActionKind::RestartHost.slug(),
        ActionArgs::default(),
    ) {
        Ok(r) => r,
        Err(r) => return serde_json::json!({ "refused": r }),
    };
    if let Err(r) = c.actions.precheck(&req) {
        return serde_json::json!({ "refused": r });
    }
    let job = c.actions.submit(req, origin);
    serde_json::json!({ "job": job.job, "action": job.action, "restarts_dashboard": job.restarts_dashboard })
}

/// Mounted with `dashboard_routes`: the login and both locks stand before
/// every one of them.
pub fn router(ctx: EditCtx) -> Router {
    Router::new()
        .route("/data/repo", get(repo_status))
        .route("/data/repo/sync", post(repo_sync))
        .route("/data/repo/unpushed", post(repo_unpushed))
        .route("/data/stacks/{stack}/edit", get(stack_edit))
        .route("/data/stacks/{stack}/plan", post(stack_plan))
        .route("/data/stacks/{stack}/commit", post(stack_commit))
        .route("/data/presets", get(presets))
        .route("/data/presets/{name}/edit", get(preset_edit))
        .route("/data/presets/plan", post(presets_plan))
        .route("/data/presets/commit", post(presets_commit))
        .route("/data/stacks-new/appdata", post(new_appdata))
        .route("/data/stacks-new/plan", post(new_plan))
        .route("/data/stacks-new/commit", post(new_commit))
        .route("/data/stacks-import/plan", post(import_plan))
        .route("/data/stacks-import/commit", post(import_commit))
        .route("/data/firewall", get(firewall))
        .route(
            "/data/host-settings",
            get(host_settings).put(host_settings_save),
        )
        .with_state(ctx)
}
