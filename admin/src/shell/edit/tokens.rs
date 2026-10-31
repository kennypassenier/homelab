//! fix-120 (per-machine tokens, owner decision 2026-10-01).
//!
//! Issuing and revoking are their own commands rather than going through
//! `SetHostConfig` — `tokens` is `Access::HostHeld` there on purpose
//! (arch-self: the dashboard's own token must not be able to loosen what it
//! may do; fix-170: it is also generated and kept by the host itself, never
//! declared in `config/host.toml`). Both still need scope `All`, so a read-
//! or operate-scope dashboard session is refused by the host itself, the
//! same as any other scope-all command.

use std::time::Duration;

use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::Response;
use homelab_proto::Command;
use serde::Deserialize;

use super::EditCtx;
use super::http::{answer, bad_gateway, body, refusal};
use crate::core::actions::Refusal;

const HOST_SILENT_FIX: &str = "check that the host answers (homelab ping)";

pub(super) async fn tokens_list(State(c): State<EditCtx>) -> Response {
    answer(read_tokens(&c).await)
}

/// The tokens the host currently trusts, name and scope only.
pub async fn read_tokens(c: &EditCtx) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let r = c
        .host
        .ask_traced(Command::TokenList, Duration::from_secs(20), None)
        .await;
    match r {
        Ok(r) if r.ok => match serde_json::from_str::<Vec<homelab_proto::TokenView>>(&r.message) {
            Ok(tokens) => Ok(serde_json::json!({ "tokens": tokens })),
            Err(e) => Err(bad_gateway(
                "the tokens",
                format!("the host's answer did not read: {e}"),
                "update the host and the dashboard to the same release",
            )),
        },
        Ok(r) => Err(bad_gateway(
            "the tokens",
            r.message,
            "look at host.toml on pve",
        )),
        Err(e) => Err(bad_gateway("the tokens", e, HOST_SILENT_FIX)),
    }
}

#[derive(Deserialize)]
pub struct IssueTokenBody {
    pub name: String,
    pub scope: homelab_proto::Scope,
}

pub(super) async fn tokens_issue(
    State(c): State<EditCtx>,
    b: Result<Json<IssueTokenBody>, JsonRejection>,
) -> Response {
    let b = match body(b, "a new token") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    answer(issue_token(&c, b.name, b.scope).await)
}

/// Mint a new `[[tokens]]` entry. `token` is in this one answer and nowhere
/// else — the host keeps only its SHA-256, and this function never logs it.
pub async fn issue_token(
    c: &EditCtx,
    name: String,
    scope: homelab_proto::Scope,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let r = c
        .host
        .ask_traced(
            Command::TokenIssue { name, scope },
            Duration::from_secs(20),
            None,
        )
        .await;
    match r {
        Ok(r) if r.ok => issued_answer(c, &r.message),
        Ok(r) => Err((
            StatusCode::BAD_REQUEST,
            Refusal::new("a new token", r.message, "pick another name"),
        )),
        Err(e) => Err(bad_gateway("a new token", e, HOST_SILENT_FIX)),
    }
}

fn issued_answer(c: &EditCtx, message: &str) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    match serde_json::from_str::<homelab_proto::TokenIssued>(message) {
        Ok(issued) => {
            c.publish
                .publish("tokens", serde_json::json!({ "issued": issued.name }));
            Ok(serde_json::json!({ "issued": issued }))
        }
        Err(e) => Err(bad_gateway(
            "a new token",
            format!("the host's answer did not read: {e}"),
            "the token was written to host.toml; read it again over ssh if this page \
             could not show it",
        )),
    }
}

#[derive(Deserialize)]
pub(super) struct RevokeQuery {
    #[serde(default)]
    confirm: Option<String>,
}

/// fix-wipe-1: the revoke carries the typed name (`?confirm=<name>`),
/// checked here and not only in the browser.
pub(super) async fn tokens_revoke(
    State(c): State<EditCtx>,
    UrlPath(name): UrlPath<String>,
    axum::extract::Query(q): axum::extract::Query<RevokeQuery>,
) -> Response {
    if q.confirm.as_deref() != Some(name.as_str()) {
        return refusal(
            StatusCode::BAD_REQUEST,
            Refusal::new(
                "revoke a token",
                "the typed name does not match the token",
                format!("type {name:?} exactly to confirm"),
            ),
        );
    }
    answer(revoke_token(&c, name).await)
}

/// Remove one `[[tokens]]` entry by name. Refused for `"legacy"` (the
/// single `token` key; see OPERATIONS_RUNBOOK's migration note) and for a
/// name that is not there.
pub async fn revoke_token(
    c: &EditCtx,
    name: String,
) -> Result<serde_json::Value, (StatusCode, Refusal)> {
    let r = c
        .host
        .ask_traced(
            Command::TokenRevoke { name: name.clone() },
            Duration::from_secs(20),
            None,
        )
        .await;
    match r {
        Ok(r) if r.ok => {
            c.publish
                .publish("tokens", serde_json::json!({ "revoked": name }));
            Ok(serde_json::json!({ "revoked": name }))
        }
        Ok(r) => Err((
            StatusCode::BAD_REQUEST,
            Refusal::new(
                "revoke a token",
                r.message,
                "`homelab token list` (or this page) shows the current names",
            ),
        )),
        Err(e) => Err(bad_gateway("revoke a token", e, HOST_SILENT_FIX)),
    }
}
