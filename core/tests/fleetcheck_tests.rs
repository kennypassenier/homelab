//! Y4: the fleet check has one job — find the things that look healthy.
//!
//! Every case below is something that was actually true of this homelab on
//! 2026-08-30 and that nothing reported. If the check cannot find them when
//! they are handed to it, it is decoration.

use homelab_core::manifest::StackManifest;
use homelab_core::ops::fleetcheck::{
    evaluate, evaluate_boot, evaluate_coverage, evaluate_growth, BootFact, CoverageFact,
    GrowthFact, GrowthLimits, LiveFacts, RouteFact, Severity,
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
        incomplete_step: None,
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
        watched_backups: vec![],
        containers: vec![(113, "113-app-metrics".into())],
        routes: vec![RouteFact {
            file: "113-app-metrics.yml".into(),
            target: "10.10.10.13:9090".into(),
            answered: true,
        }],
        stack_files: vec![("stacks/metrics".into(), 113)],
        growth: Vec::new(),
        coverage: Vec::new(),
        boot: Vec::new(),
        host_memory: None,
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
        boot: Vec::new(),
        host_memory: None,
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
    let out = evaluate_growth(&[healthy_growth(114)], GrowthLimits::default(), None);
    assert!(out.is_empty(), "expected silence, got {:?}", out);
}

/// CT 104's shape before the rollout: guards written months earlier that had
/// never run there. This is the finding that would have caught it.
#[test]
fn missing_guards_are_reported() {
    let mut g = healthy_growth(104);
    g.guards = false;
    let out = evaluate_growth(&[g], GrowthLimits::default(), None);
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
    let out = evaluate_growth(&[logs], GrowthLimits::default(), None);
    assert_eq!(out.len(), 1, "only the docker log finding: {:?}", out);
    assert!(out[0].what.contains("908 MB"));

    let mut journal = healthy_growth(104);
    journal.journal_mb = 397;
    let out = evaluate_growth(&[journal], GrowthLimits::default(), None);
    assert_eq!(out.len(), 1, "only the journal finding: {:?}", out);
    assert!(out[0].what.contains("397 MB"));
}

/// A full rootfs is the one growth case that is not merely drift: from here
/// an image pull or an apt upgrade can fail halfway.
#[test]
fn a_nearly_full_disk_is_broken_not_drift() {
    let mut g = healthy_growth(106);
    g.disk_used_pct = 91;
    let out = evaluate_growth(&[g], GrowthLimits::default(), None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Broken);

    let mut g = healthy_growth(106);
    g.disk_used_pct = 75;
    let out = evaluate_growth(&[g], GrowthLimits::default(), None);
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
    let out = evaluate_growth(&[g], GrowthLimits::default(), None);
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
        evaluate_growth(&[g.clone()], GrowthLimits::default(), None).is_empty(),
        "120 MB is under the default 150"
    );
    let strict = GrowthLimits {
        journal_mb: 100,
        ..GrowthLimits::default()
    };
    assert_eq!(evaluate_growth(&[g], strict, None).len(), 1);
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
    let out = evaluate_growth(&[a, b, c], GrowthLimits::default(), None);
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
        dashboard_provisioned: None,
    }]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].severity, Severity::Drift);
    assert!(out[0].what.contains("not being measured"));
}

/// The deploy writing a dashboard is not the same question as Grafana having
/// it, and for seven weeks only the first was asked.
///
/// The finding names the reader, not the writer: the file WAS written, every
/// time, into a directory Grafana does not mount. "Wrote it" was true and
/// useless.
/// covers: F149
#[test]
fn a_dashboard_grafana_never_received_is_reported() {
    let out = evaluate_coverage(&[CoverageFact {
        stack: "media".into(),
        scraped: Some(true),
        logs_recent: Some(true),
        dashboard_provisioned: Some(false),
    }]);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].what.contains("Grafana does not have"),
        "the finding must name the reader: {}",
        out[0].what
    );
}

/// And the unasked question must stay unasked. A host with no dashboards
/// directory configured, or a gateway that did not answer, produces `None` —
/// which must never become thirteen findings about a Grafana that is simply
/// down.
/// covers: F149
#[test]
fn an_unasked_dashboard_question_is_never_a_finding() {
    let out = evaluate_coverage(&[CoverageFact {
        stack: "media".into(),
        scraped: Some(true),
        logs_recent: Some(true),
        dashboard_provisioned: None,
    }]);
    assert!(
        out.is_empty(),
        "None means not asked, not failed: {:?}",
        out
    );
}

/// Logs that go nowhere look exactly like a quiet service — which is why the
/// window this is asked over is a setting and not the hour it started as.
///
/// The finding has to name what was actually measured: LABELLED lines. F79
/// was months of lines arriving without a container name while three
/// dashboards (F72) stayed blank, and a finding that says "nothing is shipping"
/// would send the reader looking in the wrong place.
/// covers: F79
#[test]
fn a_stack_whose_logs_never_arrive_is_reported() {
    let out = evaluate_coverage(&[CoverageFact {
        stack: "media".into(),
        scraped: Some(true),
        logs_recent: Some(false),
        dashboard_provisioned: None,
    }]);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].what.contains("LABELLED"),
        "the finding must say which reading failed: {}",
        out[0].what
    );
    assert!(
        out[0].what.contains("F79"),
        "and point at the fault it exists for: {}",
        out[0].what
    );
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
            dashboard_provisioned: None,
        },
        CoverageFact {
            stack: "almanac".into(),
            scraped: None,
            logs_recent: None,
            dashboard_provisioned: None,
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
        dashboard_provisioned: None,
    }]);
    assert!(out.is_empty(), "{:?}", out);
}

// ── W3: boot policy and resources on containers that already exist ─────────

fn boot_manifest(vmid: u16, onboot: bool, order: u16, mem: u32, cores: u16) -> StackManifest {
    let mut m = homelab_core::manifest::StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: "home".into(),
        vmid,
        hostname: format!("{}-app-home", vmid),
        network: homelab_core::manifest::NetworkSpec {
            ip: "10.10.10.15/24".into(),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: homelab_core::manifest::ResourceSpec {
            cores,
            memory_mb: mem,
            swap_mb: 512,
            disk_gb: 8,
            storage: "local-lvm".into(),
        },
        lxc: homelab_core::manifest::LxcSpec {
            template: "clone:998".into(),
            unprivileged: true,
            features: "nesting=1,keyctl=1".into(),
            protection: true,
            gpu: false,
            vpn: false,
        },
        boot: homelab_core::manifest::BootSpec {
            onboot,
            order: Some(order),
        },
        storage: vec![],
        apps: vec!["homepage".into()],
    };
    m.hostname = format!("{}-app-home", vmid);
    m
}

/// The W3 acceptance criterion, first half: a container whose boot order was
/// moved by hand is reported. After a power cut the fleet starts in whatever
/// order somebody typed years ago, and the rule that everything behind the
/// edge waits for Traefik lives in a file nothing reads.
#[test]
fn w3_a_hand_edited_boot_order_is_reported() {
    let mut state = HostState::default();
    let mut st = stack(115, "115-app-home", true, 100);
    st.manifest = Some(boot_manifest(115, true, 80, 1024, 2));
    state.stacks.insert("home".into(), st);

    let facts = vec![BootFact {
        vmid: 115,
        hostname: "115-app-home".into(),
        live: homelab_core::ops::reconcile::parse(
            "onboot: 1\nstartup: order=1\nmemory: 1024\ncores: 2\n",
        ),
    }];
    let out = evaluate_boot(&state, &facts);
    assert_eq!(out.len(), 1, "{:?}", out);
    assert!(out[0].what.contains("boot order"), "{}", out[0].what);
    assert!(out[0].what.contains("1 on the machine"), "{}", out[0].what);
    assert!(
        out[0].what.contains("80 in the stack file"),
        "{}",
        out[0].what
    );
    assert!(out[0].remedy.contains("a deploy"), "{}", out[0].remedy);
}

/// Resources diverge with a different remedy, because a deploy deliberately
/// does not change them under a running service.
#[test]
fn w3_resources_are_reported_with_the_remedy_that_applies() {
    let mut state = HostState::default();
    let mut st = stack(115, "115-app-home", true, 100);
    st.manifest = Some(boot_manifest(115, true, 80, 2048, 4));
    state.stacks.insert("home".into(), st);

    let facts = vec![BootFact {
        vmid: 115,
        hostname: "115-app-home".into(),
        live: homelab_core::ops::reconcile::parse(
            "onboot: 1\nstartup: order=80\nmemory: 1024\ncores: 2\n",
        ),
    }];
    let out = evaluate_boot(&state, &facts);
    assert_eq!(out.len(), 2, "memory and cores: {:?}", out);
    assert!(
        out.iter().all(|f| f.remedy.contains("homelab resize")),
        "{:?}",
        out
    );
}

/// Agreement is silence, and an unasked question is not a finding: a stack
/// whose record carries no manifest is skipped rather than guessed at.
#[test]
fn w3_a_container_that_matches_says_nothing() {
    let mut state = HostState::default();
    let mut st = stack(115, "115-app-home", true, 100);
    st.manifest = Some(boot_manifest(115, true, 80, 1024, 2));
    state.stacks.insert("home".into(), st);
    let facts = vec![BootFact {
        vmid: 115,
        hostname: "115-app-home".into(),
        live: homelab_core::ops::reconcile::parse(
            "onboot: 1\nstartup: order=80\nmemory: 1024\ncores: 2\n",
        ),
    }];
    assert!(evaluate_boot(&state, &facts).is_empty());

    // No manifest recorded → nothing to compare against.
    let mut bare = HostState::default();
    bare.stacks
        .insert("home".into(), stack(115, "115-app-home", true, 100));
    assert!(evaluate_boot(&bare, &facts).is_empty());

    // A config line that could not be read is unknown, not divergent.
    let unreadable = vec![BootFact {
        vmid: 115,
        hostname: "115-app-home".into(),
        live: homelab_core::ops::reconcile::parse("arch: amd64\n"),
    }];
    assert!(evaluate_boot(&state, &unreadable).is_empty());
}

/// The repair side stays as narrow as the reporting side is wide: the
/// arguments a deploy would apply cover boot policy only, never resources.
#[test]
fn w3_the_repair_touches_boot_policy_and_nothing_else() {
    use homelab_core::ops::reconcile::{boot_set_args, parse};
    let m = boot_manifest(115, true, 80, 2048, 4);
    let live = parse("onboot: 0\nstartup: order=1\nmemory: 1024\ncores: 2\n");
    let args = boot_set_args(&m, &live).join(" ");
    assert_eq!(args, "--onboot 1 --startup order=80", "got: {}", args);
    assert!(
        !args.contains("memory") && !args.contains("cores"),
        "{}",
        args
    );

    // Nothing to do when they agree.
    let same = parse("onboot: 1\nstartup: order=80\nmemory: 1024\ncores: 2\n");
    assert!(boot_set_args(&m, &same).is_empty());
}

/// F184: a remedy that cannot be carried out is not a remedy.
///
/// On 2026-09-02 the fleet check reported swap on EIGHT containers at once
/// and told Kenny, eight times, to give each one more memory. The host has
/// 31 GB of RAM with 47 GB promised to guests and 7 of its 8 GB of swap
/// already in use — so following that advice even once takes memory from
/// another container, and following it eight times makes the machine worse
/// while the check keeps saying the same thing.
///
/// covers: F184
#[test]
fn a_swapping_container_on_a_short_host_is_not_told_to_take_more_memory() {
    let mut g = healthy_growth(114);
    g.swap_used_mb = 510;

    // Host fine: the original advice stands, because raising this one
    // container's memory is something the machine can actually do.
    let roomy = evaluate_growth(&[g.clone()], GrowthLimits::default(), None);
    assert_eq!(roomy.len(), 1);
    assert!(
        roomy[0].remedy.contains("give the container more memory"),
        "{}",
        roomy[0].remedy
    );

    // Host short: the remedy names the real cause instead.
    let short = evaluate_growth(
        &[g],
        GrowthLimits::default(),
        Some("the host has 31744 MB of RAM with 47360 MB promised to guests"),
    );
    assert_eq!(short.len(), 1);
    assert!(
        !short[0].remedy.contains("give the container more memory"),
        "it must not repeat advice the machine cannot follow: {}",
        short[0].remedy
    );
    assert!(
        short[0].remedy.contains("47360") && short[0].remedy.contains("another one"),
        "and it must name the numbers and the trade-off: {}",
        short[0].remedy
    );
    // The finding itself does not change — the container IS swapping.
    assert!(short[0].what.contains("510 MB of swap"));
}

/// covers: F198
///
/// A backup this suite does not make but does watch. OPNsense is on the
/// no-touch list and uploads its own configuration every night; nothing here
/// makes that backup and nothing should. But a backup nobody watches is one
/// you find broken on the day you need it.
#[test]
fn a_watched_backup_that_stopped_is_a_finding() {
    use homelab_core::ops::fleetcheck::{evaluate_watched_backups, Severity, WatchedBackupFact};

    let fresh = WatchedBackupFact {
        name: "opnsense-config".into(),
        newest_age_s: Some(9 * 3600),
        max_age_s: 26 * 3600,
        error: None,
    };
    assert!(
        evaluate_watched_backups(&[fresh]).is_empty(),
        "a backup made nine hours ago is not news"
    );

    let stale = WatchedBackupFact {
        name: "opnsense-config".into(),
        newest_age_s: Some(50 * 3600),
        max_age_s: 26 * 3600,
        error: None,
    };
    let f = evaluate_watched_backups(&[stale]);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Broken);
    assert!(f[0].what.contains("50 hours old"), "{:?}", f[0]);

    // Nothing there at all is a different sentence, because it means
    // something else: not "it stopped" but "it never worked".
    let empty = WatchedBackupFact {
        name: "opnsense-config".into(),
        newest_age_s: None,
        max_age_s: 26 * 3600,
        error: None,
    };
    let f = evaluate_watched_backups(&[empty]);
    assert_eq!(f.len(), 1);
    assert!(f[0].what.contains("no files at all"), "{:?}", f[0]);

    // And a listing that could not run says nothing about the backup either
    // way — which is itself the finding, not silence.
    let broken = WatchedBackupFact {
        name: "opnsense-config".into(),
        newest_age_s: None,
        max_age_s: 26 * 3600,
        error: Some("directory not found".into()),
    };
    let f = evaluate_watched_backups(&[broken]);
    assert_eq!(f.len(), 1);
    assert!(f[0].what.contains("could not be listed"), "{:?}", f[0]);
}

// ── G21 of the Phase-7 gate: two branches nobody ever ran ───────────────────

/// covers: F211
///
/// A stack whose golden template no longer exists on the hypervisor. This
/// guard was written for the `clone:999` drift — CT 999 is the retired v1
/// template — and no test had ever executed it.
#[test]
fn a_stack_that_clones_a_template_which_is_gone_is_a_finding() {
    let mut m = boot_manifest(118, true, 50, 512, 1);
    m.lxc.template = "clone:999".into();
    let mut st = stack(118, "118-app-drill", true, NOW - 3600);
    st.manifest = Some(m);

    // The hypervisor has 118 but no 999.
    let live = LiveFacts {
        containers: vec![(118, "118-app-drill".into())],
        ..Default::default()
    };
    let found = check(&state(vec![("drill", st)]), &live);
    let f = found
        .iter()
        .find(|f| f.what.contains("clones template 999"))
        .unwrap_or_else(|| panic!("no finding about the missing template: {:?}", found));
    assert_eq!(f.severity, Severity::Broken);
    assert!(
        f.remedy.contains("template-build"),
        "the remedy must say how to get one back: {}",
        f.remedy
    );

    // And when the template IS there, nothing is said.
    let live_ok = LiveFacts {
        containers: vec![(118, "118-app-drill".into()), (999, "999-tmpl".into())],
        ..Default::default()
    };
    let mut m2 = boot_manifest(118, true, 50, 512, 1);
    m2.lxc.template = "clone:999".into();
    let mut st2 = stack(118, "118-app-drill", true, NOW - 3600);
    st2.manifest = Some(m2);
    assert!(
        !check(&state(vec![("drill", st2)]), &live_ok)
            .iter()
            .any(|f| f.what.contains("clones template")),
        "a template that exists is not news"
    );
}

/// covers: F211
///
/// The branch that decides whether "never been backed up" is a fault or a
/// decision. Invert it and either every reproducible stack starts screaming,
/// or a genuinely unprotected one goes quiet — so the finding most likely to
/// be believed becomes the one most likely to be wrong. Only the registry
/// stack exercises it in production, which is one accident away from none.
#[test]
fn a_stack_that_deliberately_keeps_nothing_is_noted_not_broken() {
    use homelab_core::manifest::MountSpec;

    let mut m = boot_manifest(117, true, 50, 512, 1);
    m.storage = vec![MountSpec {
        host_path: "/appdata/registry/registry-config".into(),
        mount_point: "/appdata/registry/registry-config".into(),
        no_data: false,
        no_backup: Some("a pull-through cache: every layer is re-downloadable".into()),
        host_owner_uid: None,
        app: Some("registry".into()),
    }];
    let mut st = stack(117, "117-app-registry", true, 0); // never backed up
    st.manifest = Some(m);
    let live = LiveFacts {
        containers: vec![(117, "117-app-registry".into())],
        ..Default::default()
    };

    let found = check(&state(vec![("registry", st)]), &live);
    assert!(
        !found
            .iter()
            .any(|f| f.severity == Severity::Broken && f.what.contains("never been backed up")),
        "a declared decision must not be reported as a fault: {:?}",
        found
    );
    assert!(
        found.iter().any(|f| f.severity == Severity::Noted),
        "but it must still be listed, so a deliberate gap never looks like a \
         forgotten one: {:?}",
        found
    );

    // The mirror: no declaration, never backed up → this IS broken.
    let mut m2 = boot_manifest(117, true, 50, 512, 1);
    m2.storage = vec![MountSpec {
        host_path: "/appdata/x/x-config".into(),
        mount_point: "/appdata/x/x-config".into(),
        no_data: false,
        no_backup: None,
        host_owner_uid: None,
        app: Some("x".into()),
    }];
    let mut st2 = stack(117, "117-app-registry", true, 0);
    st2.manifest = Some(m2);
    assert!(
        check(&state(vec![("registry", st2)]), &live)
            .iter()
            .any(|f| f.severity == Severity::Broken && f.what.contains("never been backed up")),
        "an undeclared stack that was never backed up is a fault"
    );
}

/// covers: F214
///
/// G15 of the Phase-7 gate, the testable half. The rule that decides what is
/// worth waking Kenny for lived inside a 340-line async loop that no test
/// could reach — so the rule deciding what counts as an alarm was itself
/// unguarded.
///
/// Z3 in one line: a `Noted` finding is a decision, not a fault. Let one
/// raise the alarm and the reader learns to ignore the notification, and
/// that notification is the one that has to be believed when it IS real.
#[test]
fn a_deliberate_decision_never_raises_the_alarm_but_is_never_hidden_either() {
    use homelab_core::ops::fleetcheck::{alarming, Finding};

    let noted = Finding {
        severity: Severity::Noted,
        subject: "registry".into(),
        what: "deliberately not backed up — a pull-through cache".into(),
        remedy: "nothing to do".into(),
    };
    let broken = Finding {
        severity: Severity::Broken,
        subject: "gateway".into(),
        what: "has never been backed up".into(),
        remedy: "run a backup now".into(),
    };
    let drift = Finding {
        severity: Severity::Drift,
        subject: "media".into(),
        what: "78 MB of swap in use".into(),
        remedy: "give it more memory".into(),
    };

    assert!(
        alarming(std::slice::from_ref(&noted)).is_empty(),
        "a decision alone is a quiet night"
    );
    assert_eq!(
        alarming(&[noted.clone(), broken.clone(), drift.clone()]).len(),
        2,
        "a real fault beside a decision still wakes somebody"
    );
    assert!(
        !alarming(&[noted.clone(), broken])
            .iter()
            .any(|f| f.severity == Severity::Noted),
        "and the decision is not smuggled into the alarm to pad it out"
    );
}

/// G8: a half-applied stack is only useful if somebody is told. The field has
/// been written since S2; until now nothing read it.
mod incomplete_deploys {
    use super::*;
    use homelab_core::ops::fleetcheck::{evaluate_incomplete, Severity};

    fn state_with(step: Option<&str>) -> HostState {
        let mut st = HostState::default();
        let s = homelab_core::state::StackState {
            vmid: 118,
            hostname: "118-app-drill".into(),
            apps: Vec::new(),
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: None,
            enabled: true,
            native: None,
            natives: Vec::new(),
            incomplete_step: step.map(|x| x.to_string()),
        };
        st.stacks.insert("drill".into(), s);
        st
    }

    #[test]
    fn a_finished_deploy_says_nothing() {
        assert!(evaluate_incomplete(&state_with(None)).is_empty());
    }

    #[test]
    fn a_deploy_that_stopped_halfway_is_reported_with_the_step_it_stopped_at() {
        let f = evaluate_incomplete(&state_with(Some("start apps")));
        assert_eq!(f.len(), 1, "one stack, one finding");
        assert_eq!(f[0].severity, Severity::Broken);
        assert_eq!(f[0].subject, "drill");
        assert!(
            f[0].what.contains("start apps"),
            "naming the step is the whole point: {}",
            f[0].what
        );
        assert!(!f[0].remedy.is_empty(), "standing rule 11");
    }

    #[test]
    fn the_full_round_carries_it_so_the_nightly_report_does_too() {
        let st = state_with(Some("bootstrap"));
        let findings = check(&st, &LiveFacts::default());
        assert!(
            findings.iter().any(|f| f.what.contains("bootstrap")),
            "evaluate() must fan out to it, or the reader is wired to nothing again: {:?}",
            findings
        );
    }
}
