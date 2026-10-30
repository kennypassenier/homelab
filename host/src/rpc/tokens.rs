//! RPC handlers: tokens.

use crate::*;
use homelab_proto::*;

/// fix-120 (per-machine tokens, owner decision 2026-10-01): mint,
/// list and revoke `[[tokens]]` entries. The scope-all audit line
/// above already named who issued or revoked.
pub(crate) async fn token_issue(
    state: &AppState,
    req_id: u64,
    name: String,
    scope: Scope,
) -> RpcResponse {
    let path = state.config.config_path.clone();
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    let plain = match random_token() {
        Ok(t) => t,
        Err(e) => {
            return RpcResponse {
                id: req_id,
                ok: false,
                message: e,
                deferred: None,
            };
        }
    };
    match issue_token_in(&raw, &name, scope, &plain)
        .and_then(|(text, tokens)| write_config_file(&path, &text).map(|()| tokens))
    {
        Ok(tokens) => {
            *state.tokens.write().unwrap_or_else(PoisonError::into_inner) = tokens;
            info!(name = %name, scope = ?scope, "token issued");
            let issued = homelab_proto::TokenIssued {
                name,
                scope,
                token: plain,
            };
            RpcResponse {
                id: req_id,
                ok: true,
                message: serde_json::to_string(&issued).unwrap_or_default(),
                deferred: None,
            }
        }
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e,
            deferred: None,
        },
    }
}

/// `TokenList`.
pub(crate) async fn token_list(state: &AppState, req_id: u64) -> RpcResponse {
    let mut views: Vec<homelab_proto::TokenView> = state
        .live_tokens()
        .into_iter()
        .map(|t| homelab_proto::TokenView {
            name: t.name,
            scope: t.scope,
        })
        .collect();
    if !state.config.token.is_empty() {
        views.insert(
            0,
            homelab_proto::TokenView {
                name: "legacy".into(),
                scope: homelab_proto::Scope::All,
            },
        );
    }
    RpcResponse {
        id: req_id,
        ok: true,
        message: serde_json::to_string(&views).unwrap_or_default(),
        deferred: None,
    }
}

/// `TokenRevoke`.
pub(crate) async fn token_revoke(state: &AppState, req_id: u64, name: String) -> RpcResponse {
    let path = state.config.config_path.clone();
    let raw = std::fs::read_to_string(&path).unwrap_or_default();
    match revoke_token_in(&raw, &name)
        .and_then(|(text, tokens)| write_config_file(&path, &text).map(|()| tokens))
    {
        Ok(tokens) => {
            *state.tokens.write().unwrap_or_else(PoisonError::into_inner) = tokens;
            info!(name = %name, "token revoked");
            RpcResponse {
                id: req_id,
                ok: true,
                message: format!("token {:?} revoked", name),
                deferred: None,
            }
        }
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: e,
            deferred: None,
        },
    }
}
