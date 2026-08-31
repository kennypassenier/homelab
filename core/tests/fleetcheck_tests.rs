//! Y4: the fleet check has one job — find the things that look healthy.
//!
//! Every case below is something that was actually true of this homelab on
//! 2026-08-30 and that nothing reported. If the check cannot find them when
//! they are handed to it, it is decoration.

use homelab_core::ops::fleetcheck::{
    evaluate, evaluate_coverage, evaluate_growth, CoverageFact, GrowthFact, GrowthLimits,
    LiveFacts, RouteFact, Severity,
};
use homelab_core::state::{HostState, StackState};

const NOW: u64 = 1_788_000_000;

fn stack(vmid: u16, hostname: &str, enabled: bool, last_backup: u64) -> StackState {
    StackState {
        vmid,
        hostname: hostname.into(),
        apps: vec!["app".into()],
        applied_at: NOW,
        last_backup,
        applied_hash: String::new(),
        manifest: None,
        native: None,
        natives: Vec::new(),
        enabled,
    }
}

fn state(entries: Vec<(&str, StackState)>) -> HostState {
    let mut s = HostState::default();
    for (name, st) in entries {
        s.stacks.insert(name.into(), st);
    }
    s
}

fn check(state: &HostState, live: &LiveFacts) -> Vec<homelab_core::ops::fleetcheck::Finding> {
    evaluate(
        state,
        live,
        NOW,
        homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
        GrowthLimits::default(),
    )
}

/// A container with nothing wrong with it. Every growth test below starts
/// here and changes exactly one thing, so the finding it produces can only
/// have come from that change.
fn healthy_growth(vmid: u16) -> GrowthFact {
    GrowthFact {
        vmid,
        hostname: format!("{}-app-test", vmid),
        disk_used_pct: 30,
        mem_used_pct: 40,
        swap_used_mb: 0,
        journal_mb: 40,
        docker_logs_mb: 20,
        guards: true,
    }
}

/// A healthy fleet produces nothing. A check that always finds something is
/// as useless as one that never does.
#[test]
fn y4_a_healthy_fleet_is_silent() {
    let st = state(vec![(
        "metrics",
        stack(113, "113-app-metrics", true, NOW - 3600),
    )]);
    let live = LiveFacts {
        containers: vec![(113, "113-app-metrics".into())],
        routes: vec![RouteFact {
            file: "113-app-metrics.yml".into(),
            target: "10.10.10.13:9090".into(),
            answered: true,
        }],
        stack_files: vec![("stacks/metrics".into(), 113)],
        growth: Vec::new(),
        coverage: Vec::new(),
    };
    assert!(check(&st, &live).is_empty(), "{:?}", check(&st, &live));
}

/// The kyu case, exactly: the record still describes the pre-rename hostname,
/// so every operation fails the guard, the nightly run failed, H8 disabled the
/// stack, and it had never been backed up. Four ways of being invisible at
/// once, and not one of them was a failure anybody saw.
#[test]
fn y4_finds_the_kyu_case() {
    let st = state(vec![("mailbox", stack(109, "109-app-mailbox", false, 0))]);
    let live = LiveFacts {
        containers: vec![(109, "109-app-kyu".into())],
        ..Default::default()
    };
    let found = check(&st, &live);
    let text: Vec<&str> = found.iter().map(|f| f.what.as_str()).collect();
    assert!(
        text.iter().any(|w| w.contains("hostname guard")),
        "the rename must be caught: {:?}",
        text
    );
    assert!(
        text.iter().any(|w| w.contains("disabled")),
        "the auto-disable must be caught: {:?}",
        text
    );
    assert!(
        text.iter().any(|w| w.contains("never been backed up")),
        "the missing backup must be caught: {:?}",
        text
    );
    assert!(found.iter().all(|f| !f.remedy.is_empty()));
}

/// A container removed outside the orchestrator leaves a record pointing at
/// nothing.
#[test]
fn y4_finds_a_vanished_container() {
    let st = state(vec![(
        "ghost",
        stack(190, "190-app-ghost", true, NOW - 100),
    )]);
    let found = check(&st, &LiveFacts::default());
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(found[0].what.contains("does not exist"));
    assert_eq!(found[0].severity, Severity::Broken);
}

/// The stacks/cloudflared case: a file claiming vmid 109, which was kyu. Only
/// the hostname guard stood between it and a deploy onto a live container.
#[test]
fn y4_finds_a_stack_file_aimed_at_someone_elses_container() {
    let st = state(vec![("metrics", stack(113, "113-app-metrics", true, NOW))]);
    let live = LiveFacts {
        containers: vec![(113, "113-app-metrics".into()), (109, "109-app-kyu".into())],
        stack_files: vec![("stacks/cloudflared".into(), 109)],
        growth: Vec::new(),
        coverage: Vec::new(),
        ..Default::default()
    };
    let found = check(&st, &live);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(found[0].what.contains("does not manage"));
    assert_eq!(found[0].subject, "stacks/cloudflared");
}

/// Both dead routes: the one to the empty MQTT container and the one to
/// Kenny's own workstation. Neither was a failure; both were simply never
/// asked.
#[test]
fn y4_finds_routes_that_lead_nowhere() {
    let live = LiveFacts {
        routes: vec![
            RouteFact {
                file: "lxc-mqtt-stack.yml".into(),
                target: "10.10.10.7:1883".into(),
                answered: false,
            },
            RouteFact {
                file: "108-app-synctest.yml".into(),
                target: "10.10.10.10:8384".into(),
                answered: false,
            },
        ],
        ..Default::default()
    };
    let found = check(&HostState::default(), &live);
    assert_eq!(found.len(), 2, "{:?}", found);
    assert!(found.iter().all(|f| f.what.contains("nothing answers")));
}

/// A backup that stopped is not visible from the outside — the files are
/// still there, they are just old.
#[test]
fn y4_finds_a_backup_that_quietly_stopped() {
    let st = state(vec![(
        "media",
        stack(106, "106-app-media", true, NOW - 8 * 7 * 24 * 3600),
    )]);
    let live = LiveFacts {
        containers: vec![(106, "106-app-media".into())],
        ..Default::default()
    };
    let found = check(&st, &live);
    assert_eq!(found.len(), 1, "{:?}", found);
    assert!(found[0].what.contains("hours ago"), "{:?}", found[0]);
}

/// The probe address, which the first live run got wrong twice. Every case
/// below is a route that actually exists in this homelab.
#[test]
fn y4_probe_address_covers_every_route_shape_in_the_house() {
    use homelab_core::ops::fleetcheck::probe_hostport;
    // The ordinary case: scheme and explicit port.
    assert_eq!(probe_hostport("http://10.10.10.6:8096"), "10.10.10.6/8096");
    // A TCP route fragment, which carries no scheme at all.
    assert_eq!(probe_hostport("10.10.10.7:1883"), "10.10.10.7/1883");
    // OPNsense: https and no port. Probing this as written asked for a path
    // instead of a socket, and reported a working router as dead.
    assert_eq!(probe_hostport("https://10.10.5.1"), "10.10.5.1/443");
    // The http default, for symmetry.
    assert_eq!(probe_hostport("http://10.10.10.2"), "10.10.10.2/80");
    // A trailing path must not be mistaken for the port.
    assert_eq!(
        probe_hostport("http://10.10.10.9:8080/healthz"),
        "10.10.10.9/8080"
    );
}

// ── G3: the growth check ────────────────────────────────────────────────
//
// Each case below is a shape this fleet actually took on 2026-08-31, when
// the guards were rolled out and five containers turned out to have none.

/// The baseline that makes the rest meaningful: a container inside every
/// limit produces nothing at all.
#[test]
fn healthy_container_reports_no_growth_findings() {
    let out = evaluate_growth(&[healthy_growth(114)], GrowthLimits::default());
    assert!(out.is_empty(), "expected silence, got {:?}", out);
}

/// CT 104's shape before the rollout: guards written months earlier that had
/// never run there. This is the finding that would have caught it.
#[test]
fn missing_guards_are_reported() {
    let mut g = healthy_growth(104);
    g.guards = false;
    let out = evaluate_growth(&[g], GrowthLimits::default());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Drift);
    assert!(out[0].what.contains("no runaway guards"));
    assert!(
        out[0].remedy.contains("homelab guards 104"),
        "the remedy must name the command and the container: {}",
        out[0].remedy
    );
}

/// Loki's 923 MB lived in docker's log directory, not the journal. Both are
/// checked, and a finding in one must not depend on the other.
#[test]
fn docker_logs_and_journal_are_checked_separately() {
    let mut logs = healthy_growth(104);
    logs.docker_logs_mb = 908;
    let out = evaluate_growth(&[logs], GrowthLimits::default());
    assert_eq!(out.len(), 1, "only the docker log finding: {:?}", out);
    assert!(out[0].what.contains("908 MB"));

    let mut journal = healthy_growth(104);
    journal.journal_mb = 397;
    let out = evaluate_growth(&[journal], GrowthLimits::default());
    assert_eq!(out.len(), 1, "only the journal finding: {:?}", out);
    assert!(out[0].what.contains("397 MB"));
}

/// A full rootfs is the one growth case that is not merely drift: from here
/// an image pull or an apt upgrade can fail halfway.
#[test]
fn a_nearly_full_disk_is_broken_not_drift() {
    let mut g = healthy_growth(106);
    g.disk_used_pct = 91;
    let out = evaluate_growth(&[g], GrowthLimits::default());
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Broken);

    let mut g = healthy_growth(106);
    g.disk_used_pct = 75;
    let out = evaluate_growth(&[g], GrowthLimits::default());
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].severity,
        Severity::Drift,
        "70-85% is a trend, not a failure"
    );
}

/// CT 106 sat at 1028 MB of swap, which is what prompted G2. Swap in use is
/// memory pressure being hidden rather than reported, so it is a finding
/// even while everything still works.
#[test]
fn swap_in_use_is_reported() {
    let mut g = healthy_growth(106);
    g.swap_used_mb = 1028;
    let out = evaluate_growth(&[g], GrowthLimits::default());
    assert_eq!(out.len(), 1);
    assert!(out[0].what.contains("1028 MB of swap"));
    assert!(
        out[0].remedy.contains("more memory rather than more swap"),
        "the remedy must point at the allocation, not at the swap size"
    );
}

/// The limits are configurable (standing rule 27), so a fleet with different
/// tolerances gets different answers from the same facts.
#[test]
fn limits_are_honoured_rather_than_hardcoded() {
    let mut g = healthy_growth(114);
    g.journal_mb = 120;
    assert!(
        evaluate_growth(&[g.clone()], GrowthLimits::default()).is_empty(),
        "120 MB is under the default 150"
    );
    let strict = GrowthLimits {
        journal_mb: 100,
        ..GrowthLimits::default()
    };
    assert_eq!(evaluate_growth(&[g], strict).len(), 1);
}

/// The check runs over the whole fleet, and one loud container must not
/// mask the others.
#[test]
fn every_container_is_reported_independently() {
    let mut a = healthy_growth(104);
    a.guards = false;
    a.docker_logs_mb = 908;
    let b = healthy_growth(112);
    let mut c = healthy_growth(106);
    c.swap_used_mb = 1028;
    let out = evaluate_growth(&[a, b, c], GrowthLimits::default());
    assert_eq!(out.len(), 3, "two for 104, none for 112, one for 106");
    assert!(out.iter().all(|f| !f.subject.contains("112")));
}

// ── Coverage: is the safety net actually attached? ──────────────────────
//
// The class this exists for, all found on 2026-08-31 and none by a test:
// log caps that ran on five of nine containers, a growth check watching five
// of nine, a discovery file written for weeks that Prometheus was never told
// to read, a promtail pipeline reading a field docker does not write, a
// database passing its healthcheck while every query failed, and an alert
// chain finished on every side but the middle.

/// A stack nobody is scraping is a stack whose failure nobody will see.
#[test]
fn a_stack_with_no_prometheus_target_is_reported() {
    let out = evaluate_coverage(&[CoverageFact {
        stack: "paperwork".into(),
        scraped: Some(false),
        logs_recent: Some(true),
    }]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Drift);
    assert!(out[0].what.contains("not being measured"));
}

/// Logs that go nowhere look exactly like a quiet service.
#[test]
fn a_stack_whose_logs_never_arrive_is_reported() {
    let out = evaluate_coverage(&[CoverageFact {
        stack: "media".into(),
        scraped: Some(true),
        logs_recent: Some(false),
    }]);
    assert_eq!(out.len(), 1);
    assert!(out[0].what.contains("going nowhere"));
}

/// The rule that keeps this check believable: a question that was not asked
/// must never become a finding. `None` means unasked — no address configured,
/// or a native service that ships no logs by design. A finding it could never
/// clear would teach Kenny to skim past the whole report.
#[test]
fn an_unasked_question_is_never_a_finding() {
    let out = evaluate_coverage(&[
        CoverageFact {
            stack: "kyu".into(),
            scraped: Some(true),
            logs_recent: None,
        },
        CoverageFact {
            stack: "almanac".into(),
            scraped: None,
            logs_recent: None,
        },
    ]);
    assert!(out.is_empty(), "expected silence, got {:?}", out);
}

/// A fully covered stack says nothing at all.
#[test]
fn a_covered_stack_is_silent() {
    let out = evaluate_coverage(&[CoverageFact {
        stack: "home".into(),
        scraped: Some(true),
        logs_recent: Some(true),
    }]);
    assert!(out.is_empty(), "{:?}", out);
}
