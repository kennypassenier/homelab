//! TUI parity: `homelab import <bundle.yml> <new-name> <vmid>` in the
//! browser, planned and committed as a new stack.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use super::EditCtx;
use super::check::{check_dir, others};
use super::follow::{follow_up, subject_with_id};
use super::http::{answer, blocking, body, refusal};
use super::planned::{Planned, plan_json};
use super::repo::publish_repo;
use super::taken::taken;
use crate::core::actions::Refusal;
use crate::core::editplan;
use crate::core::newstack::{self, Taken};
use crate::core::stackedit::FileChange;
use crate::shell::actions::Origin;
use crate::shell::workcopy::{Committed, WorkingCopy};

/// The feature id of the import's plan and commit.
const FEATURE: &str = "feat-stacks-3";

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

/// The name and number are free, and the bundle is of a size the page sends.
fn check_request(req: &ImportReq, taken: &Taken) -> Result<(), Refusal> {
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
    Ok(())
}

/// The bundle's files, under the new name. A bundle never carries secrets;
/// one that names a .env is refused rather than committed.
fn bundle_files(req: &ImportReq) -> Result<Vec<(String, String)>, Refusal> {
    let what = format!("the import of {}", req.name);
    let files =
        homelab_client::spec::imported_files(&req.bundle, &req.name, req.vmid).map_err(|why| {
            Refusal::new(
                what.clone(),
                why,
                "paste or upload the YAML an export wrote (Export bundle, or homelab export)",
            )
        })?;
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
    Ok(files)
}

fn prepare_import(wc: &WorkingCopy, req: &ImportReq, taken: &Taken) -> Result<Planned, Refusal> {
    let sync_error = wc.sync().err().map(|r| r.why);
    check_request(req, taken)?;
    let changes: Vec<FileChange> = bundle_files(req)?
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
        FEATURE,
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
    let done =
        blocking(move || commit_import_prepared(&wc, &req, &t, subject_in.as_deref(), &note))
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

fn commit_import_prepared(
    wc: &WorkingCopy,
    req: &ImportReq,
    t: &Taken,
    subject_in: Option<&str>,
    note: &str,
) -> Result<Committed, Refusal> {
    let planned = prepare_import(wc, req, t)?;
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
        FEATURE,
        &planned,
        serde_json::Value::Null,
    );
    let default = editplan::commit_subject(&req.name, &planned.summary, FEATURE);
    let subject = subject_with_id(subject_in, &default, FEATURE);
    let message = editplan::commit_message(&subject, note, &effects, &diffs);
    let others = others(wc, &req.name);
    let name = req.name.clone();
    wc.transact(&req.name, &planned.changes, &message, move |dir| {
        check_dir(dir, &name, &others).problems
    })
}

pub(super) async fn import_plan(
    State(c): State<EditCtx>,
    b: Result<Json<ImportReq>, JsonRejection>,
) -> Response {
    let req = match body(b, "the import") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(plan_import(&c, req).await)
}

pub(super) async fn import_commit(
    State(c): State<EditCtx>,
    b: Result<Json<ImportCommitBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "the import") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(commit_import(&c, b, Origin::Manual).await)
}
