use std::fs;
use std::path::PathBuf;
use std::process::Command;

use reqwest::blocking::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
struct ReleaseInfo {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize, Clone)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub fn check_and_apply_update() -> Result<String, String> {
    let repo = env_nonempty("HOST_UPDATE_REPO", "kennypassenier/homelab");
    let expected_asset = env_nonempty("HOST_UPDATE_ASSET", "HOST");
    let service = env_nonempty("HOST_UPDATE_SERVICE", "host-daemon.service");

    let release = fetch_latest_release(&repo)?;
    let latest = normalize_version(&release.tag_name);
    let current = normalize_version(env!("CARGO_PKG_VERSION"));

    if version_cmp(&latest, &current) <= 0 {
        return Ok(format!(
            "No HOST update available (current={} latest={})",
            current, latest
        ));
    }

    let (asset, fallback_used) = select_host_asset(&release, &expected_asset)?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tmp = tmp_path_for(&exe);
    let backup = backup_path_for(&exe);

    download_asset(&asset.browser_download_url, &tmp)?;
    make_executable(&tmp)?;
    validate_candidate_binary(&tmp)?;

    fs::copy(&exe, &backup)
        .map_err(|e| format!("Failed to create update backup at {}: {}", backup.display(), e))?;
    make_executable(&backup)?;

    fs::rename(&tmp, &exe).map_err(|e| {
        format!(
            "Atomic replace failed: {} (backup preserved at {})",
            e,
            backup.display()
        )
    })?;

    let watchdog_note = match schedule_update_watchdog(&service, &exe, &backup) {
        Ok(()) => "rollback watchdog armed".to_string(),
        Err(e) => format!("rollback watchdog unavailable ({})", e),
    };

    let restart_msg = match try_restart_service(&service) {
        Ok(()) => "service restart requested".to_string(),
        Err(e) => {
            // Immediate rollback path: if restart request itself fails, restore the old binary now.
            let _ = fs::copy(&backup, &exe);
            let _ = make_executable(&exe);
            let _ = try_restart_service(&service);
            return Err(format!(
                "update rollback applied after restart failure: {} (backup={})",
                e,
                backup.display()
            ));
        }
    };

    let selection = if fallback_used {
        format!("fallback asset {}", asset.name)
    } else {
        format!("asset {}", asset.name)
    };

    Ok(format!(
        "HOST updated {} -> {} ({}, {}, {}, backup={})",
        current,
        latest,
        restart_msg,
        selection,
        watchdog_note,
        backup.display()
    ))
}

pub fn check_and_apply_update_with_latch_pull() -> Result<String, String> {
    let latch_note = run_latch_pull_before_remote_update();
    match check_and_apply_update() {
        Ok(msg) => Ok(format!("{} [{}]", msg, latch_note)),
        Err(err) => Err(format!("{} [{}]", err, latch_note)),
    }
}

fn run_latch_pull_before_remote_update() -> String {
    if !env_bool("HOST_LATCH_PULL_ON_UPDATE", true) {
        return "latch pull disabled".to_string();
    }

    let repo = env_nonempty("GITOPS_REPO", "/root/homelab");

    let output = Command::new("latch")
        .args(["pull", "--sparse"])
        .current_dir(&repo)
        .output();

    match output {
        Ok(o) if o.status.success() => "latch pull --sparse ok".to_string(),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            let short = stderr
                .lines()
                .next()
                .unwrap_or("unknown latch error")
                .trim();
            format!("latch pull failed: {}", short)
        }
        Err(e) => format!("latch pull unavailable: {}", e),
    }
}

fn select_host_asset<'a>(
    release: &'a ReleaseInfo,
    expected_asset: &str,
) -> Result<(&'a ReleaseAsset, bool), String> {
    if let Some(asset) = release.assets.iter().find(|a| a.name == expected_asset) {
        return Ok((asset, false));
    }

    let fallback_candidates = ["HOST", "HOST-linux-x86_64-unknown-linux-gnu"];
    for name in fallback_candidates {
        if let Some(asset) = release.assets.iter().find(|a| a.name == name) {
            return Ok((asset, true));
        }
    }

    if let Some(asset) = release.assets.iter().find(|a| a.name.starts_with("HOST")) {
        return Ok((asset, true));
    }

    let available = if release.assets.is_empty() {
        "none".to_string()
    } else {
        release
            .assets
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };

    Err(format!(
        "Release {} missing asset {} (available: {})",
        release.tag_name, expected_asset, available
    ))
}

fn fetch_latest_release(repo: &str) -> Result<ReleaseInfo, String> {
    // Fetch recent releases and find the most recent one tagged host-daemon-v*.
    // /releases/latest returns whichever release GitHub marks as "Latest", which may
    // be an LXC or CLIENT release — not a HOST release — causing a completely wrong
    // version comparison and silently skipping every legitimate HOST update.
    let url = format!("https://api.github.com/repos/{}/releases?per_page=20", repo);
    let releases: Vec<ReleaseInfo> = github_get(&url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<Vec<ReleaseInfo>>()
        .map_err(|e| e.to_string())?;

    let mut host_releases: Vec<ReleaseInfo> = releases
        .into_iter()
        .filter(|r| r.tag_name.starts_with("host-daemon-v"))
        .collect();

    if host_releases.is_empty() {
        return Err("No host-daemon-v* release found on GitHub".to_string());
    }

    host_releases.sort_by(|a, b| {
        parse_triplet(&normalize_version(&b.tag_name))
            .cmp(&parse_triplet(&normalize_version(&a.tag_name)))
    });

    host_releases
        .into_iter()
        .next()
        .ok_or_else(|| "No host-daemon-v* release found on GitHub".to_string())
}

fn download_asset(url: &str, path: &PathBuf) -> Result<(), String> {
    let bytes = github_get(url)
        .send()
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .bytes()
        .map_err(|e| e.to_string())?;

    fs::write(path, &bytes).map_err(|e| e.to_string())
}

fn make_executable(path: &PathBuf) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn github_get(url: &str) -> reqwest::blocking::RequestBuilder {
    let client = Client::new();
    let req = client.get(url).header("User-Agent", "homelab-host-daemon");
    let token = std::env::var("HOST_UPDATE_TOKEN").unwrap_or_default();
    if token.is_empty() {
        req
    } else {
        req.bearer_auth(token)
    }
}

fn try_restart_service(service: &str) -> Result<(), String> {
    let status = Command::new("systemctl")
        .args(["restart", service])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("systemctl restart {} failed", service))
    }
}

fn tmp_path_for(exe: &std::path::Path) -> PathBuf {
    let mut p = exe.to_path_buf();
    p.set_extension("new");
    p
}

fn backup_path_for(exe: &std::path::Path) -> PathBuf {
    let mut p = exe.to_path_buf();
    p.set_extension("bak");
    p
}

fn validate_candidate_binary(path: &PathBuf) -> Result<(), String> {
    let version_output = Command::new(path)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to execute downloaded binary: {}", e))?;

    if !version_output.status.success() {
        let stderr = String::from_utf8_lossy(&version_output.stderr);
        let detail = stderr.lines().next().unwrap_or("unknown error").trim();
        return Err(format!(
            "Downloaded binary failed --version preflight: {}",
            detail
        ));
    }

    // Best-effort dynamic dependency check: reject obvious unresolved libs.
    if let Ok(ldd_output) = Command::new("ldd").arg(path).output() {
        let text = format!(
            "{}\n{}",
            String::from_utf8_lossy(&ldd_output.stdout),
            String::from_utf8_lossy(&ldd_output.stderr)
        );
        if text.contains("not found") {
            return Err(format!(
                "Downloaded binary failed dynamic-link preflight: {}",
                text.lines().find(|line| line.contains("not found")).unwrap_or("unresolved dependency")
            ));
        }
    }

    Ok(())
}

fn schedule_update_watchdog(service: &str, exe: &std::path::Path, backup: &std::path::Path) -> Result<(), String> {
    let delay_secs = std::env::var("HOST_UPDATE_VERIFY_DELAY_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(35)
        .clamp(10, 300);

    let unit = format!("host-daemon-update-verify-{}", std::process::id());
    let script = format!(
        "set -euo pipefail\nSERVICE={service}\nEXE={exe}\nBAK={backup}\nrollback=0\nif ! systemctl is-active --quiet \"$SERVICE\"; then rollback=1; fi\nif [[ $rollback -eq 0 ]] && command -v ss >/dev/null 2>&1; then\n  if ! ss -tln | grep -q ':8080 '; then rollback=1; fi\nfi\nif [[ $rollback -eq 1 ]]; then\n  cp \"$BAK\" \"$EXE\"\n  chmod +x \"$EXE\"\n  systemctl restart \"$SERVICE\" || true\nfi\n",
        service = shell_quote(service),
        exe = shell_quote(&exe.display().to_string()),
        backup = shell_quote(&backup.display().to_string())
    );

    let output = Command::new("systemd-run")
        .args([
            "--unit",
            &unit,
            "--on-active",
            &format!("{}s", delay_secs),
            "/bin/bash",
            "-lc",
            &script,
        ])
        .output()
        .map_err(|e| format!("systemd-run unavailable: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn normalize_version(v: &str) -> String {
    let stripped = v
        .trim_start_matches("host-daemon-")
        .trim_start_matches("host-daemon-v")
        .trim_start_matches('v');
    stripped.to_string()
}

fn version_cmp(a: &str, b: &str) -> i32 {
    let pa = parse_triplet(a);
    let pb = parse_triplet(b);
    pa.cmp(&pb) as i32
}

fn parse_triplet(v: &str) -> (u64, u64, u64) {
    let mut it = v.split('.').map(|x| x.parse::<u64>().unwrap_or(0));
    (
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
        it.next().unwrap_or(0),
    )
}

fn env_nonempty(key: &str, default: &str) -> String {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn env_bool(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(v) => {
            let norm = v.trim().to_ascii_lowercase();
            !matches!(norm.as_str(), "0" | "false" | "no" | "off")
        }
        Err(_) => default,
    }
}
