//! fix-143 (see REGISTER.md): the client half of the Cloudflare edge
//! comparison run by `homelab check`.
//!
//! It runs here, on the workstation, because this is where the read-only
//! token Kenny issued lives (`~/.config/cloudflare/kp-soft.token`) and where
//! the capture is committed.
//!
//! Only GETs, only with that token, and the token goes to curl on stdin.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use homelab_core::ops::edge::{
    API, EdgeIds, EdgeState, compare_edge, load_capture, project_apps, project_dns, project_tunnel,
};
use homelab_core::ops::fleetcheck::Finding;
use serde_json::Value;

/// Where the read-only token lives: `edge_token_file` in config/client.toml,
/// with `~` for the home directory. None: not configured.
pub fn token_path() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    let (_, cfg) = crate::repo_config::load(&cwd).ok()??;
    let raw = cfg.edge_token_file?;
    Some(match raw.strip_prefix("~/") {
        Some(rest) => {
            PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".into())).join(rest)
        }
        None => PathBuf::from(raw),
    })
}

/// curl's arguments: everything, the URL and the token included, comes from
/// the config on stdin (`-K -`), so neither `ps` nor a transcript sees it.
pub fn curl_argv() -> Vec<String> {
    ["-sS", "--fail", "-m", "20", "-K", "-"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// The config curl reads on stdin. Quotes, backslashes and line breaks in
/// either value are removed, so a stray character in the token file cannot
/// add an option of its own.
pub fn curl_config(token: &str, url: &str) -> String {
    let clean = |s: &str| {
        s.chars()
            .filter(|c| !matches!(c, '"' | '\\' | '\n' | '\r'))
            .collect::<String>()
    };
    format!(
        "header = \"Authorization: Bearer {}\"\nurl = \"{}\"\n",
        clean(token),
        clean(url)
    )
}

fn get(token: &str, path: &str) -> Result<Value, String> {
    let url = format!("{}{}", API, path);
    let mut child = Command::new("curl")
        .args(curl_argv())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {}", e))?;
    child
        .stdin
        .take()
        .ok_or("curl: no stdin")?
        .write_all(curl_config(token, &url).as_bytes())
        .map_err(|e| format!("curl: {}", e))?;
    let out = child
        .wait_with_output()
        .map_err(|e| format!("curl: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "GET {}: {}",
            path,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let v: Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("GET {}: {}", path, e))?;
    if v.get("success") != Some(&Value::Bool(true)) {
        return Err(format!("GET {}: the API did not answer success", path));
    }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

/// The live edge, projected like the capture.
pub fn fetch_live(ids: &EdgeIds, token: &str) -> Result<EdgeState, String> {
    let tunnel = get(
        token,
        &format!("/accounts/{}/cfd_tunnel/{}", ids.account_id, ids.tunnel_id),
    )?;
    let config = get(
        token,
        &format!(
            "/accounts/{}/cfd_tunnel/{}/configurations",
            ids.account_id, ids.tunnel_id
        ),
    )?;
    let apps = get(token, &format!("/accounts/{}/access/apps", ids.account_id))?;
    let dns = get(token, &format!("/zones/{}/dns_records", ids.zone_id))?;
    Ok(EdgeState {
        tunnels: Value::Array(vec![project_tunnel(&tunnel, &config)]),
        apps: project_apps(&apps),
        dns: project_dns(&dns),
    })
}

/// What `homelab check` does with the edge: findings, or why it was not
/// compared. Not compared is said, never counted as a finding.
pub enum EdgeOutcome {
    Compared(Vec<Finding>),
    NotCompared(String),
}

pub fn check_edge(captured_dir: &Path) -> EdgeOutcome {
    let Some(path) = token_path() else {
        return EdgeOutcome::NotCompared("no edge_token_file in config/client.toml".to_string());
    };
    let Ok(token) = std::fs::read_to_string(&path) else {
        return EdgeOutcome::NotCompared(format!(
            "no read-only Cloudflare token at {}",
            path.display()
        ));
    };
    let (ids, captured) = match load_capture(captured_dir) {
        Ok(c) => c,
        Err(e) => return EdgeOutcome::NotCompared(format!("the capture does not read: {}", e)),
    };
    match fetch_live(&ids, token.trim()) {
        Ok(live) => EdgeOutcome::Compared(compare_edge(&captured, &live)),
        Err(e) => EdgeOutcome::NotCompared(format!("the Cloudflare API did not answer: {}", e)),
    }
}
