//! gap-37: does every pinned image digest in the stack files still resolve
//! in its registry? fix-83 asks whether a NEWER release exists; this asks
//! whether the pinned one still does. Story: `docs/deployment/REGISTER.md`.
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

/// registry-cache-plaintext (deep-dive answer, 2026-10-01): the digest from
/// a registry's `Docker-Content-Digest` response header, read off the full
/// raw header block `curl -D -` returns (one `Key: Value` per line, any
/// case, either line ending). Used to resolve a TAG to a digest at the
/// source registry before a deploy ever reaches the pull-through cache on
/// 10.10.10.17 — so the cache can serve the bytes, but only the bytes whose
/// hash matches what the source said, because docker verifies a digest
/// client-side however it was fetched.
pub fn parse_digest_header(headers: &str) -> Option<String> {
    headers.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("docker-content-digest")
            .then(|| v.trim().to_string())
            .filter(|d| d.starts_with("sha256:"))
    })
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

/// registry-cache-plaintext: one `image:` line of a compose file that names
/// a TAG and no digest — the shape that needs resolving before a deploy, so
/// whatever the pull-through cache hands back is checked against a hash the
/// source registry gave for that tag a moment ago. `latest` and any other
/// tag are treated the same; refusing to resolve `latest` would just leave
/// the one tag most likely to drift unpinned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedImage {
    pub registry: String,
    pub repository: String,
    pub tag: String,
    /// The exact text after `image:` on this line, unquoted/untrimmed only
    /// of surrounding whitespace — what `rewrite_tag_line` matches on.
    pub reference: String,
}

/// Every tag-only `image:` line in a compose file, in file order. A line
/// already pinned by digest (`@sha256:…`) or naming no tag at all (bare
/// `image: redis`, which docker reads as `:latest`) IS included — `:latest`
/// is implicit precisely where resolving matters most.
pub fn tagged_images(compose: &str) -> Vec<TaggedImage> {
    let mut out = Vec::new();
    for line in compose.lines() {
        let Some(rest) = line.trim().strip_prefix("image:") else {
            continue;
        };
        let reference = rest.trim().trim_matches('"').trim_matches('\'').to_string();
        if reference.is_empty() || reference.contains('@') {
            continue;
        }
        let (registry, path) = crate::ops::registry_cache::split_registry(&reference);
        let (repository, tag) = match path.rsplit_once(':') {
            Some((repo, t)) if !t.contains('/') => (repo.to_string(), t.to_string()),
            _ => (path, "latest".to_string()),
        };
        out.push(TaggedImage {
            registry,
            repository,
            tag,
            reference,
        });
    }
    out
}

/// Append `@<digest>` to every `image:` line in `compose` whose reference is
/// in `resolved` (reference -> digest). Lines not resolved (registry did not
/// answer, private repository, offline) are left exactly as written — a
/// best-effort pin, never a reason to fail the deploy over a registry
/// having a bad evening (the same stance D60 takes toward the cache itself).
pub fn rewrite_tag_lines(
    compose: &str,
    resolved: &std::collections::BTreeMap<String, String>,
) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in compose.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("image:") {
            let reference = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if let Some(digest) = resolved.get(&reference) {
                let indent = &line[..line.len() - trimmed.len()];
                out.push(format!("{}image: {}@{}", indent, reference, digest));
                continue;
            }
        }
        out.push(line.to_string());
    }
    out.join("\n") + if compose.ends_with('\n') { "\n" } else { "" }
}
