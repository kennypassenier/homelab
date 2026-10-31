//! The small helpers every route of the edit surface shares: the refusal
//! as an HTTP answer, the request body read, the blocking hop to the working
//! copy, and the stack-name guard.

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::core::actions::{self, Refusal};

pub(super) fn refusal(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

pub(super) fn body<T>(b: Result<Json<T>, JsonRejection>, what: &str) -> Result<T, Refusal> {
    b.map(|Json(t)| t).map_err(|e| {
        Refusal::new(
            what,
            format!("the request body does not read: {}", e.body_text()),
            "send the JSON the page sends",
        )
    })
}

pub(super) async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, Refusal> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        Refusal::new(
            "the working copy",
            e.to_string(),
            "report this with the dashboard's log",
        )
    })
}

/// A route's answer from a shared function's.
pub(super) fn answer(r: Result<serde_json::Value, (StatusCode, Refusal)>) -> Response {
    match r {
        Ok(v) => Json(v).into_response(),
        Err((status, r)) => refusal(status, r),
    }
}

/// The 400 every stack route gives for a name the fleet could not have.
pub(super) fn check_stack_name(stack: &str, what: &str) -> Result<(), (StatusCode, Refusal)> {
    if actions::valid_stack_name(stack) {
        return Ok(());
    }
    Err((
        StatusCode::BAD_REQUEST,
        Refusal::new(
            what,
            "not a stack name",
            "use the name the fleet page shows",
        ),
    ))
}

/// A 502: the host's answer was a refusal, was missing, or did not read.
pub(super) fn bad_gateway(what: &str, why: impl Into<String>, fix: &str) -> (StatusCode, Refusal) {
    (StatusCode::BAD_GATEWAY, Refusal::new(what, why.into(), fix))
}
