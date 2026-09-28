//! gap-37 (2026-09-28, found by fix-153; Kenny: "Dus in het vervolg eerst
//! checken of de image bestaat ipv er maar van uit te gaan"): does every
//! pinned image digest in the stack files still resolve in its registry?
//!
//! supersync's digest, pinned when the stack was written (M8), had been
//! removed from ghcr.io. The old container had the image locally, so no
//! deploy ever pulled it again, and the first real pull was the Debian 13
//! rebuild: supersync was down until it was repinned. fix-83 asks whether a
//! NEWER release exists; nothing asked whether the pinned one still does.
//!
//! The pure halves live here (reading the pins, reading the registry's auth
//! challenge, turning answers into findings); the client asks the
//! registries, from the workstation, as part of `homelab check`.

use crate::ops::fleetcheck::{Finding, Severity};

/// One `image: …@sha256:…` line of a compose file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedDigest {
    pub stack: String,
    /// `ghcr.io`, `registry-1.docker.io`, …
    pub registry: String,
    /// `owner/name`; Docker Hub official images get their `library/`.
    pub repository: String,
    /// `sha256:<hex>`.
    pub digest: String,
    /// The image line as written.
    pub reference: String,
}

/// What the registry said about one digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinAnswer {
    Present,
    /// 404 on the manifest: the next pull of this pin fails.
    Missing,
    /// No answer that means anything (a private repository without
    /// credentials here, no network, a registry error); the text says why.
    NotAsked(String),
}

/// Every digest-pinned image in one compose file, in file order. `stack` is
/// left empty; the caller fills it in.
pub fn pinned_digests(compose: &str) -> Vec<PinnedDigest> {
    let mut out = Vec::new();
    for line in compose.lines() {
        let Some(rest) = line.trim().strip_prefix("image:") else {
            continue;
        };
        let reference = rest.trim().trim_matches('"').trim_matches('\'').to_string();
        let Some((name, digest)) = reference.split_once('@') else {
            continue;
        };
        if !digest.starts_with("sha256:") {
            continue;
        }
        // Drop a tag: the part after the last ':' that is not in the host.
        let (host_part, path) = match name.split_once('/') {
            Some((first, rest)) if first.contains('.') || first.contains(':') => {
                (Some(first), rest)
            }
            _ => (None, name),
        };
        let path = match path.rsplit_once(':') {
            Some((p, _tag)) => p,
            None => path,
        };
        let (registry, repository) = match host_part {
            Some(h) => (h.to_string(), path.to_string()),
            None if path.contains('/') => ("registry-1.docker.io".to_string(), path.to_string()),
            None => (
                "registry-1.docker.io".to_string(),
                format!("library/{}", path),
            ),
        };
        out.push(PinnedDigest {
            stack: String::new(),
            registry,
            repository,
            digest: digest.to_string(),
            reference,
        });
    }
    out
}

/// `(realm, service)` from a registry's `WWW-Authenticate: Bearer …` header.
pub fn parse_challenge(header: &str) -> Option<(String, String)> {
    let rest = header.trim().strip_prefix("Bearer ")?;
    let mut realm = None;
    let mut service = String::new();
    for part in rest.split(',') {
        let (k, v) = part.split_once('=')?;
        let v = v.trim().trim_matches('"').to_string();
        match k.trim() {
            "realm" => realm = Some(v),
            "service" => service = v,
            _ => {}
        }
    }
    Some((realm?, service))
}

/// A missing digest is Broken: the next rebuild of that stack fails at its
/// pull. One that could not be asked about is Noted, never a fault.
pub fn evaluate_pin_existence(answers: &[(PinnedDigest, PinAnswer)]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (p, a) in answers {
        match a {
            PinAnswer::Present => {}
            PinAnswer::Missing => out.push(Finding {
                severity: Severity::Broken,
                subject: format!("{} image {}/{}", p.stack, p.registry, p.repository),
                what: format!(
                    "the pinned digest {} no longer exists in the registry — the next pull \
                     (a rebuild, a disaster recovery) fails",
                    p.digest
                ),
                remedy: "repin: read the digest the tag carries today from the registry, put \
                         it on the image: line, and deploy the stack before anything rebuilds it"
                    .into(),
            }),
            PinAnswer::NotAsked(why) => out.push(Finding {
                severity: Severity::Noted,
                subject: format!("{} image {}/{}", p.stack, p.registry, p.repository),
                what: format!(
                    "whether the pinned digest still exists was not asked: {}",
                    why
                ),
                remedy: "nothing; a private repository is checked from its container before a \
                         rebuild"
                    .into(),
            }),
        }
    }
    out
}
