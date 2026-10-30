//! The edge check: reading the live Cloudflare edge and the nightly comparison with the stack files.

use crate::*;

/// fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01): one
/// GET against the Cloudflare API, through the Executor rather than
/// `std::process::Command` directly (unlike `client/src/edge.rs`, which runs
/// on the workstation and is free to shell out itself) and with the token in
/// a 0600 header file rather than on curl's argv (fix-35's own pattern,
/// reused rather than the client's `-K -` on stdin, which `Executor` has no
/// way to feed).
pub(crate) async fn edge_get(
    exec: &RealExecutor,
    header_file: &str,
    path: &str,
) -> Result<serde_json::Value, String> {
    let url = format!("{}{}", homelab_core::ops::edge::API, path);
    let args = [
        "-sS",
        "--fail",
        "-m",
        "20",
        "-H",
        &format!("@{}", header_file),
        &url,
    ];
    let out = exec
        .run(&Cmd::new("curl", &args, 25))
        .await
        .map_err(|e| format!("GET {}: {}", path, e))?;
    if !out.success() {
        return Err(format!("GET {}: curl exited {}", path, out.code));
    }
    let v: serde_json::Value =
        serde_json::from_str(&out.stdout).map_err(|e| format!("GET {}: {}", path, e))?;
    if v.get("success") != Some(&serde_json::Value::Bool(true)) {
        return Err(format!("GET {}: the API did not answer success", path));
    }
    Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
}

/// fix-143: the live edge, projected like the capture — the host's own copy
/// of `client/src/edge.rs::fetch_live`.
pub(crate) async fn fetch_live_edge(
    exec: &RealExecutor,
    header_file: &str,
    ids: &homelab_core::ops::edge::EdgeIds,
) -> Result<homelab_core::ops::edge::EdgeState, String> {
    use homelab_core::ops::edge::{EdgeState, project_apps, project_dns, project_tunnel};
    let tunnel = edge_get(
        exec,
        header_file,
        &format!("/accounts/{}/cfd_tunnel/{}", ids.account_id, ids.tunnel_id),
    )
    .await?;
    let config = edge_get(
        exec,
        header_file,
        &format!(
            "/accounts/{}/cfd_tunnel/{}/configurations",
            ids.account_id, ids.tunnel_id
        ),
    )
    .await?;
    let apps = edge_get(
        exec,
        header_file,
        &format!("/accounts/{}/access/apps", ids.account_id),
    )
    .await?;
    let dns = edge_get(
        exec,
        header_file,
        &format!("/zones/{}/dns_records", ids.zone_id),
    )
    .await?;
    Ok(EdgeState {
        tunnels: serde_json::Value::Array(vec![project_tunnel(&tunnel, &config)]),
        apps: project_apps(&apps),
        dns: project_dns(&dns),
    })
}

/// fix-143: the nightly half of the Cloudflare edge comparison — the same
/// `core::ops::edge` logic `homelab check` already runs from the
/// workstation, run here too so a flipped Access app is noticed even on a
/// night nobody ran the CLI. `cloudflare_token` absent is reported as "not
/// configured", never as a failure: an unasked question must never become a
/// finding (the same rule `prometheus_url`/`loki_url` follow).
pub(crate) async fn run_nightly_edge_check(state: &AppState, exec: &RealExecutor, now: u64) {
    let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
    let Some(token) = state.config.cloudflare_token.clone() else {
        record_state(&store, "edge check", |s| {
            s.last_edge_check = now;
            s.last_edge_findings = 0;
            s.last_edge_error = Some("not configured (no cloudflare_token in host.toml)".into());
        })
        .await;
        return;
    };
    let captured_dir = std::path::Path::new(&state.config.state_dir).join("repo/captured/gateway");
    let (findings, error) = match homelab_core::ops::edge::load_capture(&captured_dir) {
        Ok((ids, captured)) => {
            let header_file = format!("{}/secrets/cloudflare.header", state.config.state_dir);
            let header = format!("authorization: Bearer {}\n", token.trim());
            match exec.write_file(&header_file, &header, 0o600).await {
                Ok(()) => match fetch_live_edge(exec, &header_file, &ids).await {
                    Ok(live) => (
                        homelab_core::ops::edge::compare_edge(&captured, &live),
                        None,
                    ),
                    Err(e) => (
                        Vec::new(),
                        Some(format!("the Cloudflare API did not answer: {e}")),
                    ),
                },
                Err(e) => (
                    Vec::new(),
                    Some(format!("cannot write the header file: {e}")),
                ),
            }
        }
        Err(e) => (Vec::new(), Some(format!("the capture does not read: {e}"))),
    };
    if let Some(e) = &error {
        tracing::warn!("edge check: not compared — {}", e);
    } else if !findings.is_empty() {
        tracing::warn!(
            "edge check: {} finding(s) against captured/gateway/ — {}",
            findings.len(),
            homelab_core::ops::fleetcheck::render(&findings)
        );
    } else {
        info!("edge check: Cloudflare agrees with captured/gateway/");
    }
    let count = findings.len();
    record_state(&store, "edge check", |s| {
        s.last_edge_check = now;
        s.last_edge_findings = count;
        s.last_edge_error = error;
    })
    .await;
}
