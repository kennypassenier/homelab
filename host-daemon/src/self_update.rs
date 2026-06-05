use std::fs;
use std::path::PathBuf;
use std::process::Command;

use reqwest::blocking::Client;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct ReleaseInfo {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

pub fn check_and_apply_update() -> Result<String, String> {
    let repo =
        std::env::var("HOST_UPDATE_REPO").unwrap_or_else(|_| "kennypassenier/homelab".to_string());
    let expected_asset = std::env::var("HOST_UPDATE_ASSET")
        .unwrap_or_else(|_| "HOST-linux-x86_64-unknown-linux-gnu".to_string());

    let release = fetch_latest_release(&repo)?;
    let latest = normalize_version(&release.tag_name);
    let current = normalize_version(env!("CARGO_PKG_VERSION"));

    if version_cmp(&latest, &current) <= 0 {
        return Ok(format!(
            "No HOST update available (current={} latest={})",
            current, latest
        ));
    }

    let asset = release
        .assets
        .iter()
        .find(|a| a.name == expected_asset)
        .ok_or_else(|| {
            format!(
                "Release {} missing asset {}",
                release.tag_name, expected_asset
            )
        })?;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let tmp = tmp_path_for(&exe);

    download_asset(&asset.browser_download_url, &tmp)?;
    make_executable(&tmp)?;

    fs::rename(&tmp, &exe).map_err(|e| format!("Atomic replace failed: {}", e))?;

    let restart_msg = match try_restart_service() {
        Ok(()) => "service restart requested".to_string(),
        Err(e) => format!("binary replaced; manual restart may be required ({})", e),
    };

    Ok(format!(
        "HOST updated {} -> {} ({})",
        current, latest, restart_msg
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

    releases
        .into_iter()
        .find(|r| r.tag_name.starts_with("host-daemon-v"))
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

fn try_restart_service() -> Result<(), String> {
    let status = Command::new("systemctl")
        .args(["restart", "host-daemon.service"])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("systemctl restart host-daemon.service failed".to_string())
    }
}

fn tmp_path_for(exe: &std::path::Path) -> PathBuf {
    let mut p = exe.to_path_buf();
    p.set_extension("new");
    p
}

fn normalize_version(v: &str) -> String {
    v.trim_start_matches('v')
        .trim_start_matches("host-daemon-")
        .to_string()
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
