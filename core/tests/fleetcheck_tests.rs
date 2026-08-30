//! Y4: the fleet check has one job — find the things that look healthy.
//!
//! Every case below is something that was actually true of this homelab on
//! 2026-08-30 and that nothing reported. If the check cannot find them when
//! they are handed to it, it is decoration.

use homelab_core::ops::fleetcheck::{evaluate, LiveFacts, RouteFact, Severity};
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
    )
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
