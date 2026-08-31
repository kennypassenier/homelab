//! D60: point an image at the cache in the house, when the cache answers.
//!
//! The M7 drill lost 573 of its 653 seconds of outage to one `docker compose
//! pull` that stalled for 8 m 40 s (F108). Nothing here can fix a registry on
//! the internet having a bad evening — but a copy in the house means the
//! second container that needs an image does not have to ask.
//!
//! Kenny chose (form C1, 2026-08-31) the variant that rewrites at DEPLOY time
//! rather than in the files. The reason is what happens when the cache itself
//! is down: with the cache address written into every stack file, a dead
//! cache means nothing in the house can pull at all — a new single point of
//! failure bought to avoid a slow evening. Rewriting here means the files
//! keep naming the real origin, and a cache that does not answer costs speed
//! instead of the deploy.
//!
//! Docker's own `registry-mirrors` would have been simpler and is not enough:
//! it redirects Docker Hub only. Measured across the fleet on 2026-08-31 —
//! docker.io 17 images, gcr.io 7, ghcr.io 6, lscr.io 1 — and of the media
//! stack's nine, exactly two come from Hub.

use serde::{Deserialize, Serialize};

/// One upstream registry and the port its cache listens on.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheUpstream {
    /// The registry as it appears in an image reference, e.g. `ghcr.io`.
    /// Docker Hub is the empty-prefix case and is written as `docker.io`.
    pub registry: String,
    pub port: u16,
}

/// Where the cache lives and what it mirrors. None = no cache configured,
/// and every image keeps naming its own origin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheCfg {
    /// Host or address the containers reach it at, e.g. `10.10.10.17`.
    pub host: String,
    pub upstreams: Vec<CacheUpstream>,
}

/// Rewrite the `image:` lines of a compose file to point at the cache.
///
/// `available` is the set of upstreams that answered a moment ago; anything
/// not in it is left alone, so a half-broken cache degrades per registry
/// rather than all at once.
///
/// `skip_registry` is the registry this stack signs into, if any. The cache
/// is deliberately anonymous, so a private image must keep going to its own
/// registry with its own token — a cache holding it would hand it to anyone
/// on the LAN who asked.
pub fn rewrite_compose(
    content: &str,
    cfg: &CacheCfg,
    available: &[String],
    skip_registry: Option<&str>,
) -> String {
    content
        .lines()
        .map(|line| rewrite_line(line, cfg, available, skip_registry))
        .collect::<Vec<_>>()
        .join("\n")
        + if content.ends_with('\n') { "\n" } else { "" }
}

fn rewrite_line(
    line: &str,
    cfg: &CacheCfg,
    available: &[String],
    skip_registry: Option<&str>,
) -> String {
    let trimmed = line.trim_start();
    let Some(rest) = trimmed.strip_prefix("image:") else {
        return line.to_string();
    };
    let indent = &line[..line.len() - trimmed.len()];
    let image = rest.trim();
    if image.is_empty() {
        return line.to_string();
    }
    let (registry, path) = split_registry(image);
    if Some(registry.as_str()) == skip_registry {
        return line.to_string();
    }
    if !available.iter().any(|a| a == &registry) {
        return line.to_string();
    }
    let Some(up) = cfg.upstreams.iter().find(|u| u.registry == registry) else {
        return line.to_string();
    };
    format!("{}image: {}:{}/{}", indent, cfg.host, up.port, path)
}

/// Split an image reference into (registry, path-with-tag-or-digest).
///
/// Docker's own rules, which are not obvious: a first segment counts as a
/// registry only if it contains a dot or a colon, or is `localhost`.
/// Everything else is Docker Hub, and a Hub image with no namespace lives
/// under `library/` — `postgres:16` is really `docker.io/library/postgres:16`,
/// and a cache asked for `postgres` without that prefix answers 404.
pub fn split_registry(image: &str) -> (String, String) {
    let first = image.split('/').next().unwrap_or("");
    let is_registry = first.contains('.')
        || first.contains(':') && !first.starts_with("localhost")
        || first == "localhost";
    if is_registry && image.contains('/') {
        let path = image[first.len() + 1..].to_string();
        (first.to_string(), path)
    } else if image.contains('/') {
        ("docker.io".to_string(), image.to_string())
    } else {
        ("docker.io".to_string(), format!("library/{}", image))
    }
}
