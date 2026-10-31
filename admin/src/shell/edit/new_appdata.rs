//! feat-stacks-3: the `/appdata` folders a new stack's preset binds, for the
//! wizard's data-folder step.

use axum::Json;
use axum::extract::State;
use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use super::EditCtx;
use super::http::{blocking, body, refusal};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppdataBody {
    preset: String,
    name: String,
    vmid: u16,
}

pub(super) async fn new_appdata(
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
