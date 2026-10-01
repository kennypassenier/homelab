//! gap-37 (2026-09-28, found by fix-153; Kenny: "Dus in het vervolg eerst
//! checken of de image bestaat ipv er maar van uit te gaan"): a pinned image
//! digest that no longer exists upstream fails only at the next real pull,
//! which is a rebuild or a disaster recovery. supersync's digest, pinned at
//! M8, had been removed from ghcr.io; the Debian 13 rebuild of productivity
//! pulled it and supersync was down until it was repinned.
//!
//! `homelab check` now asks each registry whether every pinned digest in the
//! stack files still resolves. The pure halves are tested here: reading the
//! pins out of a compose file, reading the registry's auth challenge, and
//! turning the answers into findings.
//!
//! Each test here was written before the code and failed on it first.

use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::pinexists::{
    evaluate_pin_existence, parse_challenge, parse_digest_header, pinned_digests,
    rewrite_tag_lines, tagged_images, PinAnswer, PinnedDigest,
};

/// The three shapes the stack files carry: a tag plus digest on ghcr.io, a
/// digest alone, and a Docker Hub official image with its implicit
/// `library/` and registry.
#[test]
fn gap_37_the_pins_are_read_out_of_a_compose_file() {
    let compose = "services:\n  kp-soft:\n    image: ghcr.io/kennypassenier/kp-soft:v0.4.0@sha256:b9fd7be0bd3e6cb62a06a6eb31350ecd6499d66edd6eddd04f71aa57f552c29b\n  supersync:\n    image: ghcr.io/super-productivity/supersync@sha256:d12077be6c00545e71b50ba01831ec48af1e60458111888af699a2b76234ccde\n  db:\n    image: postgres:16.15-alpine@sha256:cf78e76683b9ca8c5733cbbdce6c9262b45b6767934dd0a95e671f9a0fc20685\n  loose:\n    image: nginx:latest\n";
    let pins = pinned_digests(compose);
    assert_eq!(pins.len(), 3, "{:?}", pins);
    assert_eq!(pins[0].registry, "ghcr.io");
    assert_eq!(pins[0].repository, "kennypassenier/kp-soft");
    assert!(pins[0].digest.starts_with("sha256:b9fd7be0"));
    assert_eq!(pins[1].repository, "super-productivity/supersync");
    assert_eq!(pins[2].registry, "registry-1.docker.io");
    assert_eq!(pins[2].repository, "library/postgres");
    // An unpinned image is not this check's business (fix-82 pins them).
    assert!(pins.iter().all(|p| !p.reference.contains("nginx")));
}

/// A registry answers `GET /v2/` with 401 and says where to get a token.
#[test]
fn gap_37_the_auth_challenge_names_the_realm_and_service() {
    let (realm, service) = parse_challenge(
        r#"Bearer realm="https://ghcr.io/token",service="ghcr.io",scope="repository:user/image:pull""#,
    )
    .expect("a bearer challenge");
    assert_eq!(realm, "https://ghcr.io/token");
    assert_eq!(service, "ghcr.io");
    assert!(parse_challenge("Basic realm=\"x\"").is_none());
}

fn pin(stack: &str, digest: &str) -> PinnedDigest {
    PinnedDigest {
        stack: stack.into(),
        registry: "ghcr.io".into(),
        repository: "super-productivity/supersync".into(),
        digest: digest.into(),
        reference: format!("ghcr.io/super-productivity/supersync@{}", digest),
    }
}

/// A digest the registry says is gone is Broken — the next rebuild of that
/// stack fails at its pull — and the remedy says how to repin. One the
/// check could not ask about (a private repository without credentials, no
/// network) is Noted, never counted as a fault.
#[test]
fn gap_37_a_missing_digest_is_broken_and_an_unanswered_one_is_only_noted() {
    let findings = evaluate_pin_existence(&[
        (pin("productivity", "sha256:fcffea4b"), PinAnswer::Present),
        (pin("productivity", "sha256:dead"), PinAnswer::Missing),
        (
            pin("kp-soft", "sha256:b9fd"),
            PinAnswer::NotAsked("401".into()),
        ),
    ]);
    assert_eq!(findings.len(), 2, "{:?}", findings);
    let broken = findings
        .iter()
        .find(|f| f.severity == Severity::Broken)
        .expect("the missing one is broken");
    assert!(broken.subject.contains("productivity"), "{:?}", broken);
    assert!(broken.what.contains("sha256:dead"), "{:?}", broken);
    assert!(broken.remedy.contains("repin"), "{:?}", broken);
    assert!(findings
        .iter()
        .any(|f| f.severity == Severity::Noted && f.subject.contains("kp-soft")));
}

/// registry-cache-plaintext (deep-dive answer, 2026-10-01): the header
/// block `curl -D -` returns is read case-insensitively, by line, CRLF or
/// LF, and a digest-shaped value is required — `parse_digest_header` is the
/// one piece standing between a registry's `HEAD` response and trusting a
/// string as a content digest.
#[test]
fn registry_cache_plaintext_parse_digest_header_reads_the_header_case_insensitively() {
    let headers = "HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.distribution.manifest.v2+json\r\nDocker-Content-Digest: sha256:deadbeef\r\n\r\n";
    assert_eq!(
        parse_digest_header(headers),
        Some("sha256:deadbeef".to_string())
    );
    // lower-case header name, LF only.
    let headers = "HTTP/1.1 200 OK\ndocker-content-digest: sha256:cafe\n";
    assert_eq!(
        parse_digest_header(headers),
        Some("sha256:cafe".to_string())
    );
    // No such header at all.
    assert_eq!(parse_digest_header("HTTP/1.1 404 Not Found\r\n"), None);
    // A header present but not digest-shaped (a registry misbehaving) is
    // rejected rather than trusted.
    assert_eq!(
        parse_digest_header("Docker-Content-Digest: not-a-digest\r\n"),
        None
    );
}

/// registry-cache-plaintext: `tagged_images` finds every tag-only
/// `image:` line (including an implicit `:latest`), skips anything already
/// pinned by digest, and splits registry/repository/tag the same way the
/// pull-through cache rewrite does.
#[test]
fn registry_cache_plaintext_tagged_images_finds_only_unpinned_references() {
    let compose = "services:\n  a:\n    image: ghcr.io/kp/app:1.2.3\n  \
                   b:\n    image: redis\n  \
                   c:\n    image: ghcr.io/kp/pinned@sha256:aaaa\n  \
                   d:\n    image: 10.10.10.17/library/traefik:v3\n";
    let images = tagged_images(compose);
    assert_eq!(images.len(), 3, "{:?}", images);
    let app = images.iter().find(|i| i.repository == "kp/app").unwrap();
    assert_eq!(app.registry, "ghcr.io");
    assert_eq!(app.tag, "1.2.3");
    assert_eq!(app.reference, "ghcr.io/kp/app:1.2.3");
    let redis = images
        .iter()
        .find(|i| i.repository == "library/redis" || i.repository == "redis")
        .unwrap();
    assert_eq!(redis.tag, "latest", "a bare image name means :latest");
    assert!(
        images.iter().all(|i| i.repository != "kp/pinned"),
        "an already-digest-pinned line is not re-resolved: {:?}",
        images
    );
}

/// registry-cache-plaintext: `rewrite_tag_lines` appends `@<digest>` only to
/// lines whose exact reference was resolved, leaves every other `image:`
/// line untouched (including one that could not be resolved), and preserves
/// indentation and the file's trailing newline.
#[test]
fn registry_cache_plaintext_rewrite_tag_lines_only_touches_resolved_references() {
    let compose = "services:\n  a:\n    image: ghcr.io/kp/app:1\n  \
                   b:\n    image: redis:7\n";
    let mut resolved = std::collections::BTreeMap::new();
    resolved.insert("ghcr.io/kp/app:1".to_string(), "sha256:aaaa".to_string());
    let out = rewrite_tag_lines(compose, &resolved);
    assert_eq!(
        out,
        "services:\n  a:\n    image: ghcr.io/kp/app:1@sha256:aaaa\n  \
         b:\n    image: redis:7\n"
    );

    // No match: byte-for-byte unchanged, trailing newline preserved.
    let unresolved = rewrite_tag_lines(compose, &std::collections::BTreeMap::new());
    assert_eq!(unresolved, compose);
}
