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
    evaluate_pin_existence, parse_challenge, pinned_digests, PinAnswer, PinnedDigest,
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
