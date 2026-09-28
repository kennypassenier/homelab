//! TUI parity round: GitHub releases as the dashboard reads them.
//!
//! * "Update host" (dash-host-update): the host binary of a homelab release,
//!   downloaded here and verified like every release the homelab installs
//!   (fix-29): the minisign signature over `SHA256SUMS`, then the binary
//!   against it; an unsigned release is refused. CT 120 has no `gh`, and
//!   needs none: the repository is public and its firewall lets HTTPS out.
//! * The "host update available" badge: the latest homelab tag, read now and
//!   then and compared with the version the host said Hello with.
//! * install-native without a tag: the latest tag of the service's
//!   repository (the host then downloads that release itself).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chassis::shell::live::Live;

use super::host_link::{now_s, Shared};

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The homelab's own repository (the host, the client, the dashboard).
pub const HOMELAB_REPO: &str = homelab_client::release::REPO;
/// The host binary's asset name.
pub const HOST_ASSET: &str = "homelab-host";
/// How often the badge looks for a newer release.
pub const WATCH_EVERY: Duration = Duration::from_secs(3600);

/// Where releases come from; a test gives a fake.
pub trait Releases: Send + Sync + 'static {
    /// The newest release's tag of `repo` (`owner/name`).
    fn latest_tag<'a>(&'a self, repo: &'a str) -> BoxFut<'a, Result<String, String>>;
    /// The host binary of `tag`, verified (signature, then checksum), as
    /// base64 ready for `SelfUpdateHost`.
    fn host_binary<'a>(&'a self, tag: &'a str) -> BoxFut<'a, Result<String, String>>;
}

/// GitHub over HTTPS.
pub struct GitHub {
    client: reqwest::Client,
}

impl Default for GitHub {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHub {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("homelab-admin/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(300))
            .build()
            .unwrap_or_default();
        GitHub { client }
    }

    async fn get(&self, url: &str) -> Result<Option<Vec<u8>>, String> {
        let r = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("GitHub did not answer {url}: {e}"))?;
        if r.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !r.status().is_success() {
            return Err(format!("GitHub answered {} for {url}", r.status()));
        }
        r.bytes()
            .await
            .map(|b| Some(b.to_vec()))
            .map_err(|e| format!("the download of {url} broke off: {e}"))
    }
}

impl Releases for GitHub {
    fn latest_tag<'a>(&'a self, repo: &'a str) -> BoxFut<'a, Result<String, String>> {
        Box::pin(async move {
            let url = format!("https://api.github.com/repos/{repo}/releases/latest");
            let body = self
                .get(&url)
                .await?
                .ok_or_else(|| format!("{repo} has no release"))?;
            let v: serde_json::Value = serde_json::from_slice(&body)
                .map_err(|e| format!("GitHub's release listing does not read: {e}"))?;
            v.get("tag_name")
                .and_then(|t| t.as_str())
                .map(str::to_string)
                .ok_or_else(|| format!("GitHub's latest release of {repo} names no tag"))
        })
    }

    fn host_binary<'a>(&'a self, tag: &'a str) -> BoxFut<'a, Result<String, String>> {
        Box::pin(async move {
            if !crate::core::actions::valid_tag(tag) {
                return Err(format!("{tag:?} is not a release tag"));
            }
            let base = format!("https://github.com/{HOMELAB_REPO}/releases/download/{tag}");
            let binary = self
                .get(&format!("{base}/{HOST_ASSET}"))
                .await?
                .ok_or_else(|| format!("release {tag} carries no {HOST_ASSET}"))?;
            let sums = self
                .get(&format!("{base}/SHA256SUMS"))
                .await?
                .ok_or_else(|| format!("release {tag} has no SHA256SUMS — refusing it"))?;
            let sums = String::from_utf8(sums).map_err(|_| "SHA256SUMS is not text".to_string())?;
            let sig = self
                .get(&format!("{base}/{}", homelab_core::release_sig::SIG_ASSET))
                .await?
                .map(|b| String::from_utf8_lossy(&b).into_owned());
            verified_b64(tag, &binary, &sums, sig.as_deref())
        })
    }
}

/// The verification every host binary passes before it is sent: the
/// signature over the checksum list, the binary against the list, and the
/// line's frame limit.
pub fn verified_b64(
    tag: &str,
    binary: &[u8],
    sums: &str,
    sig: Option<&str>,
) -> Result<String, String> {
    homelab_core::release_sig::verify_release(HOST_ASSET, binary, sums, sig)
        .map_err(|e| format!("{HOST_ASSET} {tag}: {e}"))?;
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(binary);
    if let Some(why) = homelab_client::version::too_large(b64.len()) {
        return Err(why);
    }
    Ok(b64)
}

/// The badge: the latest homelab release, read every `every`, kept in the
/// snapshot and told to every page (`release`).
pub fn spawn_watch(releases: Arc<dyn Releases>, shared: Shared, live: Live, every: Duration) {
    tokio::spawn(async move {
        loop {
            match releases.latest_tag(HOMELAB_REPO).await {
                Ok(tag) => {
                    let mut s = shared.write().await;
                    s.latest_release = Some(tag.clone());
                    s.latest_checked_at = Some(now_s());
                    let host = s.host_version.clone();
                    drop(s);
                    let _ = live.publish(
                        "release",
                        &crate::core::hostversion::release_view(Some(&tag), host.as_deref()),
                    );
                }
                Err(e) => tracing::info!(why = %e, "the latest homelab release was not read"),
            }
            tokio::time::sleep(every).await;
        }
    });
}
