//! `homelab ui open batch <action> <stack>,<stack>`.

use super::*;

/// Whether one action can be given several stacks at once.
fn batchable(k: &ActionKind) -> bool {
    !k.host_wide()
        && !k
            .args()
            .iter()
            .any(|a| matches!(a, Arg::Unit | Arg::Commit))
}

fn batch_action(req: &Req, action: &str) -> Result<ActionKind, Refusal> {
    if let Some(ak) = ActionKind::from_slug(action).filter(batchable) {
        return Ok(ak);
    }
    let all: Vec<&str> = ActionKind::ALL
        .iter()
        .filter(|k| batchable(k))
        .map(|k| k.slug())
        .collect();
    Err(req.refuse(
        format!("{action:?} is not an action several stacks can be given at once"),
        format!(
            "homelab ui open batch <action> <stack>,<stack> with one of: {}",
            all.join(", ")
        ),
    ))
}

/// Owner decision 2026-09-30: no stacks named opens the batch dialog from
/// the Overview table's own ticked selection (`homelab ui select` first),
/// exactly as the page's "Run on the selected…" button does.
fn batch_stacks(req: &Req, action: &str) -> Result<Vec<String>, Refusal> {
    let list: Vec<String> = match req.target {
        Some(t) => t
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect(),
        None => req.selected.to_vec(),
    };
    if list.is_empty() {
        return Err(req.refuse(
            "a batch needs its stacks: none were named and the fleet table's selection is empty",
            format!(
                "homelab ui select <stack>,<stack> then homelab ui open batch {action}, \
                 or homelab ui open batch {action} <stack>,<stack>"
            ),
        ));
    }
    for s in &list {
        req.stack_named(Some(s))?;
    }
    Ok(list)
}

fn check_batch_allowed(
    req: &Req,
    ak: ActionKind,
    action: &str,
    list: &[String],
) -> Result<(), Refusal> {
    if ak.refused_for_self() && list.iter().any(|s| s == SELF_STACK) {
        return Err(req.refuse(
            format!(
                "The dashboard never does this to its own stack ({SELF_STACK}); leave it out of the selection."
            ),
            "name the other stacks only",
        ));
    }
    if ak.scope() > req.cx.scope {
        return Err(req.refuse(
            format!(
                "{action} needs scope {:?}; the token \"{}\" has {:?}",
                ak.scope(),
                req.cx.by,
                req.cx.scope
            ),
            "drive it with a token of that scope, or leave it to Kenny",
        ));
    }
    Ok(())
}

pub(super) fn open_batch(req: &Req, es_: &mut EditState) -> Result<Opening, Refusal> {
    let action = req.form.strip_prefix("batch:").unwrap_or("");
    let ak = batch_action(req, action)?;
    let list = batch_stacks(req, action)?;
    check_batch_allowed(req, ak, action, &list)?;
    es_.batch_action = Some(action.to_string());
    es_.stacks = list.clone();
    es_.guarded = req.data["guarded"].as_u64().unwrap_or(0) as usize;
    let n = list.len();
    Ok(Opening {
        id: format!("batch:{action}"),
        title: format!(
            "{} · {n} {}",
            ak.label(),
            if n == 1 { "stack" } else { "stacks" }
        ),
        steps: vec![step_of("review", "Review and run", batch_fields(ak, &list))],
        // A batch action is opened from the Overview page's own selection
        // (drive.rs's Select step requires it).
        page: "/overview".to_string(),
        stack: list.join(","),
    })
}
