//! The preview route: the CLI line and what the press would change.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::Actions;
use super::routes::{parse_args, refusal};
use crate::core::actions::{self, ActionArgs, ActionKind, Refusal};

/// The route: the CLI line and what the press would do, read from the
/// stack's manifest only (no secrets, no downloads).
pub(super) async fn preview(
    State(a): State<Actions>,
    UrlPath((stack, action)): UrlPath<(String, String)>,
    b: Result<Json<ActionArgs>, JsonRejection>,
) -> Response {
    let mut args = match parse_args(b, &action, &stack) {
        Ok(args) => args,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    // cli-yes: the preview may come before the name is typed; the line
    // carries --yes only once it is.
    let typed = args.confirm.as_deref() == Some(stack.as_str());
    if ActionKind::from_slug(&action).is_some_and(|k| k.confirm()) && !typed {
        args.confirm = Some(stack.clone());
    }
    let req = match actions::validate(&stack, &action, args) {
        Ok(r) => r,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    let (stack, action) = (req.stack.clone(), req.action);
    let (line, guard, restarts) = a.preview_of(req, typed).await;
    let mut out = serde_json::json!({
        "stack": stack,
        "action": action,
        "entry": action.catalog_entry(),
        "cli": line.as_ref().ok(),
        "cli_unavailable": line.err(),
        "guard": guard,
        "restarts_dashboard": restarts,
    });
    add_plan(&a, action, &stack, &mut out).await;
    Json(out).into_response()
}

/// TUI parity: what the press would change, before it is pressed.
async fn add_plan(a: &Actions, action: ActionKind, stack: &str, out: &mut serde_json::Value) {
    let unavailable = |out: &mut serde_json::Value, r: Refusal| {
        out["plan_unavailable"] = serde_json::json!(r.why);
    };
    match action {
        ActionKind::Deploy => match a.deploy_diff(stack).await {
            Ok(d) => out["plan"] = d,
            Err(r) => unavailable(out, r),
        },
        ActionKind::Apply => match a.apply_plan(false).await {
            Ok((view, _)) => out["apply"] = serde_json::json!(view),
            Err(r) => unavailable(out, r),
        },
        ActionKind::Wipe => match a.wipe_plan(stack).await {
            Ok(text) => out["wipe_plan"] = serde_json::json!(text),
            Err(r) => unavailable(out, r),
        },
        _ => {}
    }
}
