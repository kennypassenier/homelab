//! H7 (Kenny's design, evaluation deep-dive): release-driven host updates. The
//! CLIENT — with Kenny's authenticated `gh` — detects, downloads and
//! verifies a GitHub release, then ships the binary over the proven TLS
//! line (SelfUpdateHost). The host keeps its full selfcheck/backup/armed-
//! rollback pipeline; the repo can stay private because only the desktop
//! talks to GitHub.

use std::process::Command;

pub const REPO: &str = "kennypassenier/homelab";

/// Latest release tag (e.g. "v2.7.0"), via `gh` (authenticated, private-repo
/// capable). None when gh is missing, unauthenticated, or no release exists.
pub fn latest_release_tag() -> Option<String> {
    let out = Command::new("gh")
        .args([
            "release", "view", "--repo", REPO, "--json", "tagName", "--jq", ".tagName",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!tag.is_empty()).then_some(tag)
}

/// "v2.7.0" newer than "2.6.0"? Plain semver-triple compare; malformed
/// versions are never "newer" (fail-safe: no phantom update prompts).
pub fn version_newer(latest_tag: &str, current: &str) -> bool {
    fn triple(s: &str) -> Option<(u32, u32, u32)> {
        let s = s.trim().trim_start_matches('v');
        let mut it = s.split('.');
        let maj = it.next()?.parse().ok()?;
        let min = it.next()?.parse().ok()?;
        let pat = it.next()?.split(['-', '+']).next()?.parse().ok()?;
        Some((maj, min, pat))
    }
    match (triple(latest_tag), triple(current)) {
        (Some(l), Some(c)) => l > c,
        _ => false,
    }
}

/// Does `sums` (the SHA256SUMS file) list `filename` with `actual_hex`?
pub fn sha_listed(sums: &str, filename: &str, actual_hex: &str) -> bool {
    sums.lines().any(|l| {
        let mut parts = l.split_whitespace();
        matches!((parts.next(), parts.next()),
            (Some(h), Some(f)) if h.eq_ignore_ascii_case(actual_hex)
                && f.trim_start_matches('*') == filename)
    })
}

/// Download `homelab-host` + SHA256SUMS for `tag`, verify, return the binary
/// base64-encoded ready for SelfUpdateHost. Every failure is a clear string.
pub fn stage_release(tag: &str) -> Result<String, String> {
    stage_asset(REPO, tag, "homelab-host")
}

/// T11: the same staging for ANY release asset in any repository Kenny's
/// `gh` can read — the four native services each ship their own binary.
///
/// The download and the checksum check happen HERE, on the desktop, for the
/// same reason H7 does it: this machine has the authenticated `gh`, so a
/// private repository never needs a credential on the Proxmox host, and a
/// corrupted download is refused before it ever reaches a container.
///
/// A release with no SHA256SUMS is refused rather than trusted. Installing
/// an unverified binary into a container is precisely the hand-built step
/// this verb exists to replace.
pub fn stage_asset(repo: &str, tag: &str, asset: &str) -> Result<String, String> {
    let dir =
        std::env::temp_dir().join(format!("homelab-release-{}-{}", std::process::id(), asset));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = Command::new("gh")
        .args([
            "release",
            "download",
            tag,
            "--repo",
            repo,
            "-p",
            asset,
            "-p",
            "SHA256SUMS",
            "-D",
            dir.to_str().unwrap(),
            "--clobber",
        ])
        .output()
        .map_err(|e| format!("gh not runnable: {}", e))?;
    if !out.status.success() {
        return Err(format!(
            "release download failed ({} {} from {}): {}",
            asset,
            tag,
            repo,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let binary = std::fs::read(dir.join(asset))
        .map_err(|e| format!("{} is not in release {} of {}: {}", asset, tag, repo, e))?;
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).map_err(|_| {
        format!(
            "release {} of {} has no SHA256SUMS — refusing to install an unverified binary \
             into a container, which is exactly the hand-built step this replaces",
            tag, repo
        )
    })?;
    let actual = homelab_core::manifest::sha256_hex(&binary);
    if !sha_listed(&sums, asset, &actual) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!(
            "CHECKSUM MISMATCH for {} in {} — download corrupted or tampered; not shipping it",
            asset, tag
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::STANDARD.encode(&binary))
}

/// Latest release tag of any repository (H7's helper, generalised for T11).
pub fn latest_tag_of(repo: &str) -> Option<String> {
    let out = Command::new("gh")
        .args([
            "release", "view", "--repo", repo, "--json", "tagName", "--jq", ".tagName",
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let tag = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!tag.is_empty()).then_some(tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare_is_strict_and_failsafe() {
        assert!(version_newer("v2.7.0", "2.6.0"));
        assert!(version_newer("v2.6.1", "2.6.0"));
        assert!(!version_newer("v2.6.0", "2.6.0"));
        assert!(!version_newer("v2.5.9", "2.6.0"));
        assert!(version_newer("v3.0.0", "2.99.99"));
        assert!(
            !version_newer("garbage", "2.6.0"),
            "malformed is never newer"
        );
        assert!(
            !version_newer("v2.7.0", "dev"),
            "unknown current: no prompt"
        );
    }

    #[test]
    fn b7_install_source_reads_a_tag_or_a_file() {
        use super::{install_source, InstallSource};
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            install_source(&a(&[])).unwrap(),
            InstallSource::Release(None)
        );
        assert_eq!(
            install_source(&a(&["v3.3.0"])).unwrap(),
            InstallSource::Release(Some("v3.3.0".into()))
        );
        assert_eq!(
            install_source(&a(&["--file", "/tmp/good.sh"])).unwrap(),
            InstallSource::File("/tmp/good.sh".into())
        );
        assert!(install_source(&a(&["--file"])).is_err());
        assert!(install_source(&a(&["--bogus"])).is_err());
    }

    #[test]
    fn sha_listing_verification() {
        let sums = "abc123  homelab-host\ndef456  homelab\n";
        assert!(sha_listed(sums, "homelab-host", "abc123"));
        assert!(
            sha_listed(sums, "homelab-host", "ABC123"),
            "case-insensitive"
        );
        assert!(
            !sha_listed(sums, "homelab-host", "def456"),
            "wrong file's hash"
        );
        assert!(!sha_listed(sums, "homelab-host", "beef"), "unlisted hash");
        assert!(!sha_listed("", "homelab-host", "abc123"));
    }
}

/// B7: where `install-native` takes its binary from — a release tag (the
/// default, resolved to the latest when absent) or a local file for a drill
/// that hands a fake service over by hand. Pure so the argument shapes have
/// one reader and a test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallSource {
    Release(Option<String>),
    File(String),
}

pub fn install_source(rest: &[String]) -> Result<InstallSource, String> {
    match rest {
        [] => Ok(InstallSource::Release(None)),
        [flag, path] if flag == "--file" => Ok(InstallSource::File(path.clone())),
        [flag] if flag == "--file" => Err("--file needs a path".into()),
        [tag] if !tag.starts_with("--") => Ok(InstallSource::Release(Some(tag.clone()))),
        other => Err(format!(
            "usage: homelab install-native stacks/<name>[/<unit>] [<tag> | --file <path>] (got {:?})",
            other
        )),
    }
}

/// A local file as the binary: read, hashed for the transcript, base64 for
/// the line. Nothing is verified against a checksum list here — the caller
/// chose the bytes and sees their hash.
pub fn stage_file(path: &str) -> Result<(String, String), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {}: {}", path, e))?;
    if bytes.is_empty() {
        return Err(format!(
            "{} is empty — an empty program is not a service",
            path
        ));
    }
    let sha = homelab_core::manifest::sha256_hex(&bytes);
    use base64::Engine as _;
    Ok((
        base64::engine::general_purpose::STANDARD.encode(&bytes),
        sha,
    ))
}
