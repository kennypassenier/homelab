//! The fleet's read-only pages built from the working copy's declarations:
//! feat-firewall-2 (every stack's rules and the matrix), feat-overview-7
//! (topology) and feat-stacks-9 (dependencies).

use std::collections::BTreeMap;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use super::EditCtx;
use super::fleet_read::{fetch_firewall_live, fleet_firewall};
use super::http::{blocking, refusal};
use crate::core::fwmatrix;

type LiveStatuses = BTreeMap<String, homelab_core::ops::fleetcheck::FirewallLiveStatus>;

/// fix-207: patch each stack's row with what pve actually enforces, so the
/// "Firewall" column (and anything that reuses this same read, e.g. the
/// firewall page's table) shows the deployed truth, not only the working
/// copy's declaration.
fn patch_live(summaries: &mut [serde_json::Value], live: &LiveStatuses) {
    for row in summaries {
        let Some(stack) = row.get("stack").and_then(|v| v.as_str()) else {
            continue;
        };
        if let Some(s) = live.get(stack)
            && let Some(obj) = row.as_object_mut()
        {
            obj.insert("live_enforced".into(), serde_json::json!(s.enforced));
            obj.insert(
                "live_matches_repo".into(),
                serde_json::json!(s.matches_repo),
            );
        }
    }
}

pub(super) async fn firewall(State(c): State<EditCtx>) -> Response {
    let wc = c.wc.clone();
    let read = blocking(move || {
        let (fleet, summaries, head, ..) = fleet_firewall(&wc)?;
        Ok((fwmatrix::matrix(&fleet), summaries, head, fleet))
    })
    .await
    .and_then(|r| r);
    match read {
        Ok((m, mut summaries, head, fleet)) => {
            if let Some(live) = fetch_firewall_live(&c, &fleet).await {
                patch_live(&mut summaries, &live);
            }
            Json(serde_json::json!({ "matrix": m, "stacks": summaries, "head": head }))
                .into_response()
        }
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

/// feat-overview-7 (topology: which container talks to which): the same
/// firewall declarations feat-firewall-2 reads, turned into nodes and
/// edges (`crate::core::topology`). fix-203: `from_fleet_declared`, not the
/// firewall page's stricter `from_fleet` — the fleet view wants every
/// written-down relationship (even one not enforced yet) and every address
/// a stack's own files name, not only what is currently enforced. The
/// firewall page overlays measured traffic on ITS OWN plain
/// `from_fleet`/`from_matrix` shape (feat-firewall-3), unchanged.
pub(super) async fn topology(State(c): State<EditCtx>) -> Response {
    let wc = c.wc.clone();
    let read = blocking(move || {
        let (fleet, _summaries, head, natives, gateway_vmids, named) = fleet_firewall(&wc)?;
        Ok((
            crate::core::topology::from_fleet_declared(&fleet, &natives, &gateway_vmids, &named),
            head,
            fleet,
        ))
    })
    .await
    .and_then(|r| r);
    match read {
        Ok((mut t, head, fleet)) => {
            // fix-207: the repository-only shape stays the fallback; a live
            // answer from the host only ever adds the two optional fields
            // per node, never changes a node or edge the repository found.
            if let Some(live) = fetch_firewall_live(&c, &fleet).await {
                t = crate::core::topology::with_live(t, &live);
            }
            Json(serde_json::json!({ "topology": t, "head": head })).into_response()
        }
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

/// feat-stacks-9 (dependencies between stacks): the topology's edges, read
/// per stack as "depends on" / "depended on by". Same `from_fleet_declared`
/// as the topology route (fix-203), so the two pages can never disagree
/// about what counts as a dependency.
pub(super) async fn dependencies(State(c): State<EditCtx>) -> Response {
    let wc = c.wc.clone();
    let read = blocking(move || {
        let (fleet, _summaries, head, natives, gateway_vmids, named) = fleet_firewall(&wc)?;
        let topo =
            crate::core::topology::from_fleet_declared(&fleet, &natives, &gateway_vmids, &named);
        Ok((crate::core::topology::dependencies(&topo), head))
    })
    .await
    .and_then(|r| r);
    match read {
        Ok((rows, head)) => {
            Json(serde_json::json!({ "dependencies": rows, "head": head })).into_response()
        }
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}
