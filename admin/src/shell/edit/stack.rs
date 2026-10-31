//! feat-stacks-2: one stack's edit, planned and committed. The routes and
//! the driven `next` / `confirm` (feat-platform-10) share the functions.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;

use super::EditCtx;
use super::applied::applied_changes;
use super::check::{check_dir, others};
use super::follow::{follow_up, subject_with_id};
use super::http::{answer, blocking, body, check_stack_name, refusal};
use super::planned::{plan_json, prepare};
use super::repo::publish_repo;
use crate::core::actions::Refusal;
use crate::core::editplan;
use crate::core::stackedit::StackEdit;
use crate::shell::actions::Origin;
use crate::shell::workcopy::{Committed, WorkingCopy};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlanBody {
    edit: StackEdit,
}

pub(super) async fn stack_plan(
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

/// feat-stacks-2: the plan of one stack's edit. The route and a driven
/// `next` (feat-platform-10) both come here.
pub async fn plan_stack(
    c: &EditCtx,
    stack: &str,
    edit: StackEdit,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    check_stack_name(stack, "the plan")?;
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

/// redesign-flows-6: the Update flow's commit of new image lines, through
/// the same transaction a person's edit takes (`commit_stack`), without a
/// follow-up: the flow deploys the commit itself.
pub struct EditCommitter(pub EditCtx);

impl crate::shell::actions::ImageCommitter for EditCommitter {
    fn commit_images(
        &self,
        stack: String,
        images: std::collections::BTreeMap<String, String>,
        subject: String,
        origin: Origin,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, Refusal>> + Send + '_>>
    {
        Box::pin(async move {
            let body: CommitBody = serde_json::from_value(serde_json::json!({
                "edit": { "kind": "settings", "images": images },
                "subject": subject,
            }))
            .map_err(|e| Refusal::new("the commit", e.to_string(), "report this"))?;
            let v = commit_stack(&self.0, &stack, body, origin)
                .await
                .map_err(|(_, r)| r)?;
            Ok(v["committed"]["commit"]
                .as_str()
                .unwrap_or_default()
                .to_string())
        })
    }
}

pub(super) async fn stack_commit(
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
    check_stack_name(stack, "the commit")?;
    let stack = stack.to_string();
    let wc = c.wc.clone();
    let (s2, edit) = (stack.clone(), b.edit.clone());
    let (subject_in, note) = (b.subject.clone(), b.note.clone().unwrap_or_default());
    let done = blocking(move || commit_edit(&wc, &s2, &edit, subject_in.as_deref(), &note))
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

/// Plan again under the lock, refuse on problems, and commit the changes.
fn commit_edit(
    wc: &WorkingCopy,
    stack: &str,
    edit: &StackEdit,
    subject_in: Option<&str>,
    note: &str,
) -> Result<Committed, Refusal> {
    let planned = prepare(wc, stack, edit)?;
    if !planned.checked.problems.is_empty() {
        return Err(Refusal::new(
            format!("the commit to stacks/{stack}"),
            planned.checked.problems.join("; "),
            "correct the edit; nothing was written",
        ));
    }
    let (plan, effects, diffs) = plan_json(
        stack,
        edit.kind(),
        edit.feature(),
        &planned,
        serde_json::Value::Null,
    );
    let default = plan["subject"].as_str().unwrap_or_default().to_string();
    let subject = subject_with_id(subject_in, &default, edit.feature());
    let message = editplan::commit_message(&subject, note, &effects, &diffs);
    let others = others(wc, stack);
    let name = stack.to_string();
    wc.transact(stack, &planned.changes, &message, move |dir| {
        check_dir(dir, &name, &others).problems
    })
}
