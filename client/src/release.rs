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
    let dir = std::env::temp_dir().join(format!("homelab-release-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let out = Command::new("gh")
        .args([
            "release",
            "download",
            tag,
            "--repo",
            REPO,
            "-p",
            "homelab-host",
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
            "release download failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let binary = std::fs::read(dir.join("homelab-host")).map_err(|e| e.to_string())?;
    let sums = std::fs::read_to_string(dir.join("SHA256SUMS")).map_err(|e| e.to_string())?;
    let actual = homelab_core::manifest::sha256_hex(&binary);
    if !sha_listed(&sums, "homelab-host", &actual) {
        let _ = std::fs::remove_dir_all(&dir);
        return Err(format!(
            "CHECKSUM MISMATCH for {} — download corrupted or tampered; not shipping it",
            tag
        ));
    }
    let _ = std::fs::remove_dir_all(&dir);
    use base64::Engine as _;
    Ok(base64::engine::general_purpose::STANDARD.encode(&binary))
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
