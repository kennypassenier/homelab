//! What every commit shares after the repository took it: the subject with
//! its feature id, and the follow-up job (a deploy of exactly that commit,
//! or a resize) queued through the action machinery.

use super::EditCtx;
use crate::core::actions::{self, ActionArgs, ActionKind, Refusal};
use crate::shell::actions::Origin;

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
pub(super) fn follow_up(
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
            return serde_json::json!({ "refused": Refusal::new("the follow-up", format!("{other:?} is not deploy or resize"), "pick one the plan offers") });
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
    queue(c, req, origin)
}

/// Precheck and submit a validated request; the job's JSON or the refusal.
pub(super) fn queue(c: &EditCtx, req: actions::ActionRequest, origin: Origin) -> serde_json::Value {
    if let Err(r) = c.actions.precheck(&req) {
        return serde_json::json!({ "refused": r });
    }
    let job = c.actions.submit(req, origin);
    serde_json::json!({ "job": job.job, "action": job.action, "restarts_dashboard": job.restarts_dashboard })
}
