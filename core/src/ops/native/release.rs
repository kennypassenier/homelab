//! B1: reading GitHub's release answers — the latest release, the newest
//! signed one, the list a dropdown shows — and the checksum list. Pure
//! parsing, no I/O.

use serde_json::Value;

/// B1: what the latest release of a repository offers — the tag, and the
/// download URLs of the asset and its checksum list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseRefs {
    pub tag: String,
    pub asset_url: String,
    pub sums_url: String,
    /// fix-29: `SHA256SUMS.minisig`, or None while the release is unsigned.
    pub sig_url: Option<String>,
}

/// The `browser_download_url` of the asset called `name`, if the release
/// carries one.
fn asset_url_of(release: &Value, name: &str) -> Option<String> {
    release
        .get("assets")?
        .as_array()?
        .iter()
        .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))?
        .get("browser_download_url")?
        .as_str()
        .map(str::to_string)
}

/// GitHub's `releases/latest` answer, reduced to what an update needs. A
/// release without the asset, or without SHA256SUMS, is refused here — an
/// unverifiable binary is exactly the hand-built step this replaces.
pub fn parse_latest_release(json: &str, asset: &str) -> Result<ReleaseRefs, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("release listing is not JSON: {}", e))?;
    if let Some(msg) = v.get("message").and_then(|m| m.as_str())
        && v.get("tag_name").is_none()
    {
        return Err(format!("GitHub answered: {}", msg));
    }
    let tag = v
        .get("tag_name")
        .and_then(|t| t.as_str())
        .ok_or("release listing carries no tag_name")?
        .to_string();
    let asset_url = asset_url_of(&v, asset)
        .ok_or_else(|| format!("release {} carries no asset '{}'", tag, asset))?;
    let sums_url = asset_url_of(&v, "SHA256SUMS").ok_or_else(|| {
        format!(
            "release {} has no SHA256SUMS — refusing to install an unverified binary",
            tag
        )
    })?;
    let sig_url = asset_url_of(&v, crate::release_sig::SIG_ASSET);
    Ok(ReleaseRefs {
        tag,
        asset_url,
        sums_url,
        sig_url,
    })
}

/// The releases of GitHub's list answer, or why it is not one.
fn release_array(json: &str) -> Result<Vec<Value>, String> {
    let v: Value =
        serde_json::from_str(json).map_err(|e| format!("release list is not JSON: {}", e))?;
    match v.as_array() {
        Some(list) => Ok(list.clone()),
        None => {
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("not a list of releases");
            Err(format!("GitHub answered: {}", msg))
        }
    }
}

fn flag(release: &Value, key: &str) -> bool {
    release.get(key).and_then(|b| b.as_bool()) == Some(true)
}

/// dashboard-latch: the newest published release in GitHub's release LIST
/// (`/releases`, newest first) that carries `asset`, `SHA256SUMS` and the
/// signature over it. Drafts and pre-releases are passed over, and so is a
/// release that is not signed yet: between the CI upload and the author's
/// signature the newest release is unsigned, and a container missing a tool
/// should get the newest one that can be verified rather than fail its
/// deploy for an hour. Each entry is read by [`parse_latest_release`], so
/// the refusals are the same as for a single release.
pub fn newest_signed_release(json: &str, asset: &str) -> Result<ReleaseRefs, String> {
    release_array(json)?
        .iter()
        .filter(|r| !flag(r, "draft") && !flag(r, "prerelease"))
        .filter_map(|r| parse_latest_release(&r.to_string(), asset).ok())
        .find(|refs| refs.sig_url.is_some())
        .ok_or_else(|| {
            format!(
                "no signed release carries '{}' (asset, SHA256SUMS and {}) — refusing to \
                 install an unverified binary",
                asset,
                crate::release_sig::SIG_ASSET
            )
        })
}

/// dashboard-latest: one release of GitHub's release LIST, reduced to what
/// a "release tag" dropdown needs — the tag, and whether it carries
/// `SHA256SUMS.minisig` (fix-29: a release without it is listed but not
/// selectable, never silently skipped).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ReleaseListItem {
    pub tag: String,
    pub signed: bool,
}

/// GitHub's `releases` (list) answer, reduced to the tag and signed state
/// of every published release, in the order GitHub gives them (newest
/// first); drafts and pre-releases are left out, the same as
/// [`newest_signed_release`].
pub fn list_releases(json: &str) -> Result<Vec<ReleaseListItem>, String> {
    Ok(release_array(json)?
        .iter()
        .filter(|r| !flag(r, "draft") && !flag(r, "prerelease"))
        .filter_map(|r| {
            let tag = r.get("tag_name")?.as_str()?.to_string();
            let signed = asset_url_present(r, crate::release_sig::SIG_ASSET);
            Some(ReleaseListItem { tag, signed })
        })
        .collect())
}

/// Whether the release lists an asset called `name` at all (a URL is not
/// needed to call a release signed).
fn asset_url_present(release: &Value, name: &str) -> bool {
    release
        .get("assets")
        .and_then(|a| a.as_array())
        .is_some_and(|assets| {
            assets
                .iter()
                .any(|a| a.get("name").and_then(|n| n.as_str()) == Some(name))
        })
}

/// The checksum a SHA256SUMS file lists for `filename`, if any.
pub fn listed_sha(sums: &str, filename: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let mut parts = l.split_whitespace();
        match (parts.next(), parts.next()) {
            (Some(h), Some(f)) if f.trim_start_matches('*') == filename => {
                Some(h.to_ascii_lowercase())
            }
            _ => None,
        }
    })
}

/// A release tag as a URL path segment may carry it.
pub fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'+'))
}
