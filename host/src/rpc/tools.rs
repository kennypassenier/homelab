//! RPC handlers: managed tools (`[tools.*]` in host.toml).

use crate::*;
use homelab_core::ops::tools::{check, install};
use homelab_core::tools::policy::Verdict;
use homelab_core::tools::spec::ToolSpec;

/// The one place a tool name becomes a spec; an unknown name names the ones
/// that exist, so a typo is answered with its fix.
fn find<'a>(state: &'a AppState, name: &str) -> Result<&'a ToolSpec, String> {
    state
        .config
        .tools
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| {
            let known: Vec<&str> = state.config.tools.iter().map(|t| t.name.as_str()).collect();
            format!(
                "no managed tool '{name}' :: known tools are [{}]",
                known.join(", ")
            )
        })
}

fn fail(req_id: u64, message: String) -> RpcResponse {
    RpcResponse {
        id: req_id,
        ok: false,
        message,
        deferred: None,
    }
}

fn ok(req_id: u64, message: String) -> RpcResponse {
    RpcResponse {
        id: req_id,
        ok: true,
        message,
        deferred: None,
    }
}

fn verdict_json(v: &Verdict) -> serde_json::Value {
    match v {
        Verdict::Current => serde_json::json!({ "state": "current" }),
        Verdict::Install(ver) => {
            serde_json::json!({ "state": "install", "version": ver.to_string() })
        }
        Verdict::Hold { version, major } => {
            serde_json::json!({ "state": "hold", "version": version.to_string(), "major": major })
        }
    }
}

/// One JSON object per managed tool: installed, newest, and what the follow
/// rule says. A tool whose release cannot be read still appears, with the
/// reason, so the dashboard shows the failure instead of hiding the row.
pub(crate) async fn tool_status(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    let mut rows = Vec::new();
    for spec in &state.config.tools {
        let row = match check::status(&exec, spec).await {
            Ok(s) => serde_json::json!({
                "name": s.name,
                "installed": s.installed.map(|v| v.to_string()),
                "newest": s.newest.to_string(),
                "follow": spec.follow,
                "verdict": verdict_json(&s.verdict),
            }),
            Err(e) => serde_json::json!({ "name": spec.name, "error": e.to_string() }),
        };
        rows.push(row);
    }
    ok(req_id, serde_json::json!({ "tools": rows }).to_string())
}

/// Install the newest signed release now. This is also how a held major is
/// released: a person pressed the button, so the follow rule is satisfied.
pub(crate) async fn tool_update(state: &AppState, req_id: u64, name: String) -> RpcResponse {
    let spec = match find(state, &name) {
        Ok(s) => s,
        Err(e) => return fail(req_id, e),
    };
    let exec = RealExecutor;
    let (version, refs) = match check::newest_release(&exec, spec).await {
        Ok(r) => r,
        Err(e) => return fail(req_id, e.to_string()),
    };
    match install::install(&exec, spec, &refs).await {
        Ok(_) => ok(
            req_id,
            format!(
                "{name} {version} installed; the previous one is kept as {}",
                spec.prev_path()
            ),
        ),
        Err(e) => fail(req_id, e.to_string()),
    }
}

pub(crate) async fn tool_rollback(state: &AppState, req_id: u64, name: String) -> RpcResponse {
    let spec = match find(state, &name) {
        Ok(s) => s,
        Err(e) => return fail(req_id, e),
    };
    match install::rollback(&RealExecutor, spec).await {
        Ok(()) => ok(req_id, format!("{name} is back on its previous program")),
        Err(e) => fail(req_id, e.to_string()),
    }
}

/// What the doctor shows about each managed tool; `None` when the host
/// manages none, so an older report stays as it was.
pub(crate) async fn tool_probes(
    state: &AppState,
) -> Option<Vec<homelab_core::doctor_tools::ToolProbe>> {
    use homelab_core::doctor_tools::ToolProbe;
    if state.config.tools.is_empty() {
        return None;
    }
    let exec = RealExecutor;
    let mut out = Vec::new();
    for spec in &state.config.tools {
        let probe = match check::status(&exec, spec).await {
            Ok(s) => ToolProbe {
                name: s.name,
                installed: s.installed.map(|v| v.to_string()),
                newest: Some(s.newest.to_string()),
                held: match s.verdict {
                    Verdict::Hold { version, major } => Some((version.to_string(), major)),
                    _ => None,
                },
                error: None,
            },
            Err(e) => ToolProbe {
                name: spec.name.clone(),
                installed: None,
                newest: None,
                held: None,
                error: Some(e.to_string()),
            },
        };
        out.push(probe);
    }
    Some(out)
}
