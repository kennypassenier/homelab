//! fix-83 · what a pinned `manual` app runs, and whether upstream moved on.
//!
//! Expert panel finding manual-images-latest-unpinned, Kenny's answer
//! "vastzetten-plus-melding" (2026-09-27). Pinning (fix-82) stops a rebuild
//! from jumping ahead; on its own it also stops anything from ever moving, and
//! nobody would know. Measured the same day: traefik ran 3.7.12 with 3.7.13
//! out since 2026-09-04, cloudflared 2026.8.3 with 2026.9.3 out, grafana
//! 13.2.1 with 13.2.2 out, and nothing had said so. The nightly round now
//! records the digest each manual container runs and says, as a noted
//! finding, when its declared upstream has released something newer.

use std::collections::BTreeMap;

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::fleetcheck::{check_passes, evaluate, GrowthLimits, LiveFacts, Severity};
use homelab_core::ops::pins::{
    apply, evaluate_pins, gather, is_behind, RunningImage, UpstreamRelease, UPSTREAM_MAX_AGE_S,
};
use homelab_core::state::{HostState, StackState};

const NOW: u64 = 1_790_000_000;
const TRAEFIK_DIGEST: &str =
    "sha256:9c2a54d87f76f5c2f5f2682c68394af92fb12c0a2686798d6462a3f84bd78eaf";

fn stack(vmid: u16, name: &str) -> StackState {
    StackState {
        vmid,
        hostname: format!("{}-app-{}", vmid, name),
        apps: vec!["app".into()],
        applied_at: NOW,
        last_backup: NOW,
        applied_hash: String::new(),
        manifest: None,
        natives: Vec::new(),
        incomplete_step: None,
        enabled: true,
        route_file: None,
        extra_route_files: Vec::new(),
    }
}

fn fleet() -> HostState {
    let mut s = HostState::default();
    s.stacks.insert("gateway".into(), stack(104, "gateway"));
    s.stacks.insert("metrics".into(), stack(113, "metrics"));
    s
}

/// What the inspect loop prints on CT 104: one line per manual container.
fn gateway_inspect() -> CmdOutput {
    CmdOutput::ok(&format!(
        "/traefik|10.10.10.17:5000/library/traefik:v3.7.12@{d}|{d}|github.com/traefik/traefik|\
         10.10.10.17:5000/library/traefik@{d},traefik@{d}\n\
         /goaccess|10.10.10.17:5000/allinurl/goaccess:1.11@sha256:95fd|sha256:95fd||\
         10.10.10.17:5000/allinurl/goaccess@sha256:95fd\n",
        d = TRAEFIK_DIGEST
    ))
}

const INSPECT: &str =
    "sh -c for c in $(docker ps -q --filter label=com.homelab.update.policy=manual)";

fn release_json(tag: &str) -> CmdOutput {
    CmdOutput::ok(&format!(
        r#"{{"tag_name": "{}", "published_at": "2026-09-04T12:40:17Z", "name": "x"}}"#,
        tag
    ))
}

/// covers: fix-83
#[tokio::test]
async fn fix_83_the_nightly_records_the_digest_each_manual_container_runs() {
    let exec = MockExecutor::new();
    exec.respond_always(
        &format!("pct exec 104 -- timeout -k 10 60 {}", INSPECT),
        gateway_inspect(),
    );
    exec.respond_always(
        &format!("pct exec 113 -- timeout -k 10 60 {}", INSPECT),
        CmdOutput::failed(1, "docker: not running"),
    );
    exec.respond_always(
        "api.github.com/repos/traefik/traefik",
        release_json("v3.7.13"),
    );

    let mut state = fleet();
    // A record from an earlier night for the stack that cannot be read now:
    // an unreadable container keeps what was last known rather than losing it.
    state.running_images.insert(
        "metrics".into(),
        BTreeMap::from([(
            "prometheus".to_string(),
            RunningImage {
                image: "prom/prometheus:v3.14.0@sha256:5ce7".into(),
                digest: "sha256:5ce7".into(),
                upstream: None,
                seen_at: NOW - 86_400,
            },
        )]),
    );
    let (facts, notes) = gather(&exec, &state, &[], NOW).await;
    apply(&mut state, facts);

    let gw = &state.running_images["gateway"];
    assert_eq!(gw["traefik"].digest, TRAEFIK_DIGEST);
    assert_eq!(
        gw["traefik"].upstream.as_deref(),
        Some("github.com/traefik/traefik")
    );
    assert_eq!(gw["traefik"].seen_at, NOW);
    assert_eq!(
        gw["goaccess"].upstream, None,
        "an empty label is no upstream"
    );
    assert_eq!(
        state.running_images["metrics"]["prometheus"].seen_at,
        NOW - 86_400,
        "a container that could not be read keeps its last record"
    );
    assert!(
        notes.iter().any(|n| n.contains("113")),
        "the unreadable container is named in the notes: {:?}",
        notes
    );
    assert_eq!(
        state.upstream_releases["github.com/traefik/traefik"]
            .latest
            .as_deref(),
        Some("v3.7.13")
    );
}

/// GitHub is asked at most once a night per upstream, with a bound on every
/// call, and a GitHub that does not answer costs one timeout, not fifteen.
///
/// covers: fix-83
#[tokio::test]
async fn fix_83_github_is_asked_once_a_night_and_never_waited_on_twice() {
    let exec = MockExecutor::new();
    exec.respond_always(
        &format!("pct exec 104 -- timeout -k 10 60 {}", INSPECT),
        CmdOutput::ok(
            "/traefik|traefik:v3.7.12@sha256:aa|sha256:aa|github.com/traefik/traefik|traefik@sha256:aa\n\
             /grafana|grafana/grafana:13.2.1@sha256:bb|sha256:bb|github.com/grafana/grafana|grafana/grafana@sha256:bb\n\
             /cloudflared|cloudflare/cloudflared:2026.8.3@sha256:cc|sha256:cc|github.com/cloudflare/cloudflared|x@sha256:cc\n",
        ),
    );
    // Asked in name order: cloudflared first, and it gets no answer.
    exec.respond_always(
        "api.github.com/repos/cloudflare/cloudflared",
        CmdOutput::failed(
            28,
            "curl: (28) Operation timed out after 20001 milliseconds",
        ),
    );
    let mut state = HostState::default();
    state.stacks.insert("gateway".into(), stack(104, "gateway"));
    state.upstream_releases.insert(
        "github.com/traefik/traefik".into(),
        UpstreamRelease {
            checked_at: NOW - 2 * 3600,
            latest: Some("v3.7.13".into()),
            published_at: None,
            error: None,
        },
    );
    let (facts, _) = gather(&exec, &state, &[], NOW).await;

    assert!(
        exec.calls_containing("repos/traefik/traefik").is_empty(),
        "asked two hours ago: tonight's answer is still the cached one"
    );
    let asked = exec.calls_containing("api.github.com");
    assert_eq!(
        asked.len(),
        1,
        "after one upstream timed out the rest are not asked: {:?}",
        asked
    );
    assert!(asked[0].contains(" -m "), "curl carries its own bound");
    assert!(
        exec.timeouts_for("api.github.com").iter().all(|t| *t <= 30),
        "and so does the command"
    );
    apply(&mut state, facts);
    let skipped = &state.upstream_releases["github.com/grafana/grafana"];
    assert!(
        skipped.error.as_deref().unwrap_or("").contains("not asked"),
        "the skipped one says why: {:?}",
        skipped
    );
    assert_eq!(
        skipped.checked_at, NOW,
        "and is not asked again tonight either"
    );
    assert!(state.upstream_releases["github.com/cloudflare/cloudflared"]
        .error
        .as_deref()
        .unwrap_or("")
        .contains("timed out"));
    const { assert!(UPSTREAM_MAX_AGE_S >= 12 * 3600 && UPSTREAM_MAX_AGE_S < 24 * 3600) };
}

fn recorded(image: &str, upstream: Option<&str>) -> RunningImage {
    RunningImage {
        image: image.into(),
        digest: "sha256:aa".into(),
        upstream: upstream.map(str::to_string),
        seen_at: NOW,
    }
}

fn release(latest: &str) -> UpstreamRelease {
    UpstreamRelease {
        checked_at: NOW,
        latest: Some(latest.into()),
        published_at: Some("2026-09-04T12:40:17Z".into()),
        error: None,
    }
}

/// covers: fix-83
#[test]
fn fix_83_a_pinned_app_behind_upstream_is_a_noted_finding_and_nothing_else() {
    let mut s = fleet();
    s.last_restore_drill = NOW;
    s.running_images.insert(
        "gateway".into(),
        BTreeMap::from([
            (
                "traefik".to_string(),
                recorded(
                    "10.10.10.17:5000/library/traefik:v3.7.12@sha256:aa",
                    Some("github.com/traefik/traefik"),
                ),
            ),
            (
                "crowdsec".to_string(),
                recorded(
                    "crowdsecurity/crowdsec:v1.8.1@sha256:aa",
                    Some("github.com/crowdsecurity/crowdsec"),
                ),
            ),
            (
                "cadvisor".to_string(),
                recorded(
                    "gcr.io/cadvisor/cadvisor:v0.55.1@sha256:aa",
                    Some("github.com/google/cadvisor"),
                ),
            ),
            (
                "gluetun".to_string(),
                recorded("qmcgaw/gluetun@sha256:aa", Some("github.com/qdm12/gluetun")),
            ),
        ]),
    );
    s.running_images.insert(
        "metrics".into(),
        BTreeMap::from([(
            "cadvisor".to_string(),
            recorded(
                "gcr.io/cadvisor/cadvisor:v0.55.1@sha256:aa",
                Some("github.com/google/cadvisor"),
            ),
        )]),
    );
    s.upstream_releases
        .insert("github.com/traefik/traefik".into(), release("v3.7.13"));
    s.upstream_releases.insert(
        "github.com/crowdsecurity/crowdsec".into(),
        release("v1.8.1"),
    );
    s.upstream_releases
        .insert("github.com/google/cadvisor".into(), release("v0.60.6"));
    s.upstream_releases
        .insert("github.com/qdm12/gluetun".into(), release("v3.41.0"));

    let f = evaluate_pins(&s);
    assert_eq!(
        f.len(),
        2,
        "traefik, and cadvisor once for both stacks: {:#?}",
        f
    );
    assert!(f.iter().all(|x| x.severity == Severity::Noted));
    let traefik = f.iter().find(|x| x.subject.contains("traefik")).unwrap();
    assert!(traefik.what.contains("v3.7.12") && traefik.what.contains("v3.7.13"));
    assert!(traefik.what.contains("2026-09-04"), "{}", traefik.what);
    let cadvisor = f.iter().find(|x| x.subject.contains("cadvisor")).unwrap();
    assert!(
        cadvisor.subject.contains("gateway") && cadvisor.subject.contains("metrics"),
        "{}",
        cadvisor.subject
    );

    // Part of the fleet check, and never what makes it fail.
    let all = evaluate(
        &s,
        &LiveFacts::default(),
        NOW,
        homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
        GrowthLimits::default(),
    );
    assert!(all.iter().any(|x| x.subject.contains("traefik")));
    let only_pins: Vec<_> = all
        .iter()
        .filter(|x| x.what.contains("upstream"))
        .cloned()
        .collect();
    assert!(check_passes(&only_pins));
}

/// covers: fix-83
#[test]
fn fix_83_an_upstream_that_could_not_be_asked_is_said_once() {
    let mut s = fleet();
    s.running_images.insert(
        "gateway".into(),
        BTreeMap::from([(
            "traefik".to_string(),
            recorded(
                "traefik:v3.7.12@sha256:aa",
                Some("github.com/traefik/traefik"),
            ),
        )]),
    );
    s.upstream_releases.insert(
        "github.com/traefik/traefik".into(),
        UpstreamRelease {
            checked_at: NOW,
            latest: None,
            published_at: None,
            error: Some("API rate limit exceeded".into()),
        },
    );
    let f = evaluate_pins(&s);
    assert_eq!(f.len(), 1, "{:#?}", f);
    assert_eq!(f[0].severity, Severity::Noted);
    assert!(f[0].what.contains("rate limit"), "{}", f[0].what);
}

/// covers: fix-83
#[test]
fn fix_83_versions_compare_by_their_numbers_whatever_the_decoration() {
    for (pinned, latest) in [
        ("v3.7.12", "v3.7.13"),
        ("2026.8.3", "2026.9.3"),
        ("13.2.1", "v13.2.2"),
        ("5.2.3_v2.0.14-ls474", "5.2.3_v2.0.15-ls478"),
        ("10.11.11", "v12.1"),
        ("3.0.0", "v3.7.8"),
        ("2.8.3", "v3.1.2"),
    ] {
        assert!(is_behind(pinned, latest), "{} is behind {}", pinned, latest);
    }
    for (pinned, latest) in [
        ("v1.8.1", "v1.8.1"),
        ("8.7.2", "v8.7.2"),
        ("3.10.0", "v3.10.0"),
        ("v3.8.0", "v3.7.13"),
        ("latest", "v3.7.13"),
    ] {
        assert!(
            !is_behind(pinned, latest),
            "{} is not behind {}",
            pinned,
            latest
        );
    }
}
