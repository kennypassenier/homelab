//! pinned-digest-pruned-1: a digest a stack pins stays pullable after the
//! upstream deletes it. Story: `docs/deployment/REGISTER.md`.

use std::collections::BTreeSet;

use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::keep::{
    KEEP_COPIES_PER_ROUND, KEEP_MAX_COPIES, Pin, copy_script, parse_catalog, parse_tags,
    pins_in_compose, pins_in_deployed_compose, plan, rewrite_to_keep,
};
use homelab_core::ops::pinexists::{
    PinAnswer, PinnedDigest, evaluate_pin_existence, pinned_digests,
};
use homelab_core::ops::registry_cache::{CacheCfg, CacheUpstream};

const D1: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";
const D2: &str = "sha256:2222222222222222222222222222222222222222222222222222222222222222";

fn cfg() -> CacheCfg {
    CacheCfg {
        host: "10.10.10.17".into(),
        upstreams: vec![
            CacheUpstream {
                registry: "docker.io".into(),
                port: 5000,
            },
            CacheUpstream {
                registry: "ghcr.io".into(),
                port: 5001,
            },
        ],
        pull_timeout_secs: 180,
        keep_port: Some(5004),
        keep_max_mb: None,
    }
}

fn pin(registry: &str, repository: &str, digest: &str) -> Pin {
    Pin {
        registry: registry.into(),
        repository: repository.into(),
        digest: digest.into(),
    }
}

#[test]
fn a_pin_is_kept_under_a_tag_made_from_its_digest() {
    let p = pin("ghcr.io", "super-productivity/super-productivity", D1);
    assert_eq!(p.tag(), format!("sha256-{}", &D1[7..]));
    assert_eq!(
        p.kept_ref("10.10.10.17", 5004),
        format!(
            "10.10.10.17:5004/ghcr.io/super-productivity/super-productivity@{}",
            D1
        )
    );
}

#[test]
fn the_same_pin_is_seen_in_the_repository_and_in_a_deployed_container() {
    let origin = format!("    image: ghcr.io/o/n:master-abc@{D1}\n");
    let cached = format!("    image: 10.10.10.17:5001/o/n:master-abc@{D1}\n");
    let kept = format!("    image: 10.10.10.17:5004/ghcr.io/o/n@{D1}\n");
    let hub = format!("    image: 10.10.10.17:5000/library/redis:7@{D2}\n");
    let want = pin("ghcr.io", "o/n", D1);
    assert_eq!(pins_in_compose(&origin), vec![want.clone()]);
    assert_eq!(
        pins_in_deployed_compose(&origin, &cfg()),
        vec![want.clone()]
    );
    assert_eq!(
        pins_in_deployed_compose(&cached, &cfg()),
        vec![want.clone()]
    );
    assert_eq!(pins_in_deployed_compose(&kept, &cfg()), vec![want]);
    assert_eq!(
        pins_in_deployed_compose(&hub, &cfg()),
        vec![pin("docker.io", "library/redis", D2)]
    );
    // Docker Hub has two spellings in the stack-file reader; one here.
    let hub_origin = format!("    image: redis:7@{D2}\n");
    assert_eq!(
        pins_in_compose(&hub_origin),
        vec![pin("docker.io", "library/redis", D2)]
    );
}

#[test]
fn a_tag_only_image_and_a_foreign_cache_port_are_not_pins() {
    let c = "    image: ghcr.io/o/n:latest\n    image: 10.10.10.17:5999/o/n@sha256:abc\n";
    assert!(pins_in_deployed_compose(c, &cfg()).is_empty());
}

#[test]
fn the_keep_rewrite_touches_only_digest_pinned_lines() {
    let compose = format!(
        "services:\n  a:\n    image: ghcr.io/o/n:master-abc@{D1}\n  b:\n    image: nginx:alpine\n  c:\n    image: ghcr.io/private/x@{D2}\n"
    );
    let out = rewrite_to_keep(&compose, "10.10.10.17", 5004, Some("ghcr.io"));
    // ghcr.io is the stack's own login: private, never kept, never rewritten.
    assert_eq!(out, compose);
    let out = rewrite_to_keep(&compose, "10.10.10.17", 5004, None);
    assert!(out.contains(&format!("    image: 10.10.10.17:5004/ghcr.io/o/n@{D1}\n")));
    assert!(out.contains("    image: nginx:alpine\n"));
    assert!(out.contains(&format!("10.10.10.17:5004/ghcr.io/private/x@{D2}")));
    assert!(out.ends_with('\n'));
    // What it wrote reads back as the same pins.
    assert_eq!(
        pins_in_deployed_compose(&out, &cfg()),
        vec![pin("ghcr.io", "o/n", D1), pin("ghcr.io", "private/x", D2)]
    );
}

#[test]
fn the_plan_copies_what_is_missing_and_drops_what_nothing_pins() {
    let a = pin("ghcr.io", "o/a", D1);
    let b = pin("ghcr.io", "o/b", D2);
    let wanted: BTreeSet<Pin> = [a.clone(), b.clone()].into();
    let held: BTreeSet<(String, String)> = [
        (a.kept_repo(), a.tag()),
        ("ghcr.io/o/old".to_string(), "sha256-dead".to_string()),
    ]
    .into();
    let p = plan(&wanted, &held, true);
    assert_eq!(p.copy, vec![b]);
    assert_eq!(p.drop, vec![("ghcr.io/o/old".into(), "sha256-dead".into())]);
}

/// A stack whose compose could not be read is missing from `wanted`; dropping
/// on that would delete the only copy of its pins. An empty wanted set (a
/// state that failed to load) never drops either.
#[test]
fn nothing_is_dropped_when_the_pins_are_not_all_known() {
    let a = pin("ghcr.io", "o/a", D1);
    let held: BTreeSet<(String, String)> = [(a.kept_repo(), a.tag())].into();
    let none: BTreeSet<Pin> = BTreeSet::new();
    assert!(plan(&none, &held, true).drop.is_empty());
    let other: BTreeSet<Pin> = [pin("ghcr.io", "o/b", D2)].into();
    assert!(plan(&other, &held, false).drop.is_empty());
    assert_eq!(plan(&other, &held, true).drop.len(), 1);
}

#[test]
fn the_store_is_bounded_by_copies_per_round_and_in_all() {
    let wanted: BTreeSet<Pin> = (0..100)
        .map(|i| pin("ghcr.io", &format!("o/n{i:03}"), D1))
        .collect();
    let held = BTreeSet::new();
    let p = plan(&wanted, &held, true);
    assert_eq!(p.copy.len(), KEEP_COPIES_PER_ROUND);
    assert!(!p.notes.is_empty());
    // Nearly full: only the room that is left.
    let held: BTreeSet<(String, String)> = (0..KEEP_MAX_COPIES - 3)
        .map(|i| (format!("ghcr.io/x/h{i}"), "sha256-aa".to_string()))
        .collect();
    let p = plan(&wanted, &held, true);
    assert_eq!(p.copy.len(), 3);
}

#[test]
fn the_registry_answers_are_read() {
    assert_eq!(
        parse_catalog(r#"{"repositories":["ghcr.io/o/n","docker.io/library/redis"]}"#),
        vec!["ghcr.io/o/n", "docker.io/library/redis"]
    );
    assert_eq!(
        parse_tags(r#"{"name":"x","tags":["sha256-ab","latest"]}"#),
        vec!["sha256-ab"]
    );
    assert!(parse_catalog("not json").is_empty());
}

#[test]
fn the_copy_keeps_the_manifest_digest_and_tries_the_plain_cache_without_tls() {
    let p = pin("ghcr.io", "o/n", D1);
    let s = copy_script(&p, &p.origin_ref(), false, 5004);
    assert!(s.contains("copy --all --preserve-digests"));
    assert!(s.contains("--src-tls-verify=true"));
    assert!(s.contains(&format!("'docker://ghcr.io/o/n@{D1}'")));
    assert!(s.contains(&format!(
        "'docker://127.0.0.1:5004/ghcr.io/o/n:{}'",
        p.tag()
    )));
    let c = copy_script(&p, "10.10.10.17:5001/o/n@x", true, 5004);
    assert!(c.contains("--src-tls-verify=false"));
}

#[test]
fn an_upstream_deleted_pin_with_a_kept_copy_is_attention_not_broken() {
    let pd = PinnedDigest {
        stack: "productivity".into(),
        registry: "ghcr.io".into(),
        repository: "o/n".into(),
        digest: D1.into(),
        reference: format!("ghcr.io/o/n@{D1}"),
    };
    let kept = pin_ref();
    let f = evaluate_pin_existence(&[(pd.clone(), PinAnswer::MissingKept(kept.clone()))]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Drift);
    assert!(f[0].what.contains("deleted upstream"));
    assert!(f[0].what.contains(&kept));
    // Neither exists: still Broken.
    let f = evaluate_pin_existence(&[(pd, PinAnswer::Missing)]);
    assert_eq!(f[0].severity, Severity::Broken);
}

fn pin_ref() -> String {
    pin("ghcr.io", "o/n", D1).kept_ref("10.10.10.17", 5004)
}

#[test]
fn the_stack_file_reader_still_finds_the_pin_the_keep_is_made_from() {
    let c = format!("services:\n  a:\n    image: ghcr.io/o/n:v1@{D1}\n");
    let pins = pinned_digests(&c);
    assert_eq!(pins.len(), 1);
    assert_eq!(
        homelab_core::ops::keep::pin_of(&pins[0]),
        Some(pin("ghcr.io", "o/n", D1))
    );
}

#[test]
fn the_keep_registry_is_an_insecure_registry_of_every_docker_guest() {
    // guards.rs builds daemon.json from the upstream ports plus keep_port.
    let src = include_str!("../src/ops/guards.rs");
    assert!(src.contains(".chain(c.keep_port)"));
}
