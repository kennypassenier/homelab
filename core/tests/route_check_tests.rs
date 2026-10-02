//! fix-92 (expert panel, routes-outside-repo-unvalidated; Kenny 2026-09-27:
//! "alles-in-repo-plus-controle"): the routes are checked before they go
//! live, and the gateway is checked for route files no stack owns.
//!
//! Until this, homelab checked a route file's NAME and nothing in it. Two
//! stacks could claim the same hostname — copy a route as an example, forget
//! to change the host — and which of the two Traefik served was not decided
//! anywhere; duplicate routers across files already caused one outage (F115).
//! A stack file could also send any hostname to any address, the management
//! network included, without a word.
//!
//! Each test here was written before the code and failed on it first.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::facts::{FactsInputs, gather_live_facts};
use homelab_core::ops::fleetcheck::{Severity, evaluate_route_owners};
use homelab_core::routes::{RouteFacts, fleet_route_problems};
use homelab_core::state::{HostState, StackState};

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// One route file as the client reads it: the hostnames its routers match
/// and the backends it forwards to.
fn route(
    stack: &str,
    filename: &str,
    hosts: &[&str],
    backends: &[&str],
    external: &[&str],
) -> RouteFacts {
    RouteFacts {
        stack: stack.into(),
        filename: filename.into(),
        hosts: strings(hosts),
        backends: strings(backends),
        external: strings(external),
    }
}

fn stacks() -> Vec<(String, String)> {
    vec![
        ("media".into(), "10.10.10.6".into()),
        ("paperwork".into(), "10.10.10.14".into()),
        ("gateway".into(), "10.10.10.4".into()),
    ]
}

/// The copy-paste scenario from the finding: a new stack copies media's
/// route as its example and keeps `fin.kp-soft.dev`.
/// covers: fix-92
#[test]
fn a_hostname_claimed_by_two_route_files_is_refused() {
    let routes = vec![
        route(
            "media",
            "106-app-media.yml",
            &["fin.kp-soft.dev"],
            &["http://10.10.10.6:8096"],
            &[],
        ),
        route(
            "paperwork",
            "114-app-paperwork.yml",
            &["fin.kp-soft.dev"],
            &["http://10.10.10.14:8000"],
            &[],
        ),
    ];
    let problems = fleet_route_problems(&stacks(), &routes);
    assert_eq!(problems.len(), 1, "{:?}", problems);
    assert!(
        problems[0].contains("fin.kp-soft.dev")
            && problems[0].contains("106-app-media.yml")
            && problems[0].contains("114-app-paperwork.yml"),
        "{}",
        problems[0]
    );
}

/// One file may name its own host in several routers — almanac's blocks
/// `/metrics` with a second router on the same host.
/// covers: fix-92
#[test]
fn a_hostname_repeated_inside_one_file_is_fine() {
    let st = vec![("almanac".to_string(), "10.10.10.12".to_string())];
    let problems = fleet_route_problems(
        &st,
        &[route(
            "almanac",
            "112-app-almanac.yml",
            &["almanac.kp-soft.dev", "almanac.kp-soft.dev"],
            &["http://10.10.10.12:8080"],
            &[],
        )],
    );
    assert!(problems.is_empty(), "{:?}", problems);
}

/// A backend that is no stack's address is refused unless the file declares
/// it external — which is what keeps a route to the management network
/// (10.10.5.0/24: pve, OPNsense) from appearing unannounced.
/// covers: fix-92
#[test]
fn a_backend_that_is_no_stacks_address_needs_an_external_declaration() {
    let prox = |external: &[&str]| {
        route(
            "gateway",
            "manual-routes.yml",
            &["prox.kp-soft.dev"],
            &["https://10.10.5.250:8006"],
            external,
        )
    };
    let bare = fleet_route_problems(&stacks(), &[prox(&[])]);
    assert_eq!(bare.len(), 1, "{:?}", bare);
    assert!(
        bare[0].contains("https://10.10.5.250:8006") && bare[0].contains("external"),
        "{}",
        bare[0]
    );
    let declared = fleet_route_problems(&stacks(), &[prox(&["https://10.10.5.250:8006"])]);
    assert!(declared.is_empty(), "{:?}", declared);
}

/// A backend on a managed stack's address passes, its own or another's.
/// covers: fix-92
#[test]
fn a_backend_on_a_managed_stacks_address_passes() {
    let own = route(
        "gateway",
        "104-app-gateway.yml",
        &["grafana.kp-soft.dev"],
        &["http://10.10.10.4:3000"],
        &[],
    );
    let other = route(
        "gateway",
        "manual-x.yml",
        &["x.kp-soft.dev"],
        &["http://10.10.10.14:5006"],
        &[],
    );
    assert!(fleet_route_problems(&stacks(), &[own, other]).is_empty());
}

/// An external declaration the file does not route to is stale: it would
/// wave through a backend nobody meant.
/// covers: fix-92
#[test]
fn an_external_declaration_the_file_does_not_use_is_refused() {
    let problems = fleet_route_problems(
        &stacks(),
        &[route(
            "gateway",
            "manual-homeassistant.yml",
            &["ha.kp-soft.dev"],
            &["http://10.10.10.2:8123"],
            &["http://10.10.10.2:8123", "http://10.10.10.2:2300"],
        )],
    );
    assert_eq!(problems.len(), 1, "{:?}", problems);
    assert!(
        problems[0].contains("http://10.10.10.2:2300"),
        "{}",
        problems[0]
    );
}

/// Two stacks declaring the same file name would overwrite each other on
/// the gateway.
/// covers: fix-92
#[test]
fn the_same_route_file_declared_by_two_stacks_is_refused() {
    let a = route(
        "media",
        "shared.yml",
        &["a.kp-soft.dev"],
        &["http://10.10.10.6:1"],
        &[],
    );
    let b = route(
        "paperwork",
        "shared.yml",
        &["b.kp-soft.dev"],
        &["http://10.10.10.14:1"],
        &[],
    );
    let problems = fleet_route_problems(&stacks(), &[a, b]);
    assert!(
        problems
            .iter()
            .any(|p| p.contains("shared.yml") && p.contains("media") && p.contains("paperwork")),
        "{:?}",
        problems
    );
}

fn recorded(route_file: Option<&str>, extras: &[&str]) -> StackState {
    StackState {
        pushed_file_hashes: std::collections::BTreeMap::new(),
        component_digests: Default::default(),
        applied_source: None,
        vmid: 104,
        hostname: "104-app-gateway".into(),
        apps: vec![],
        applied_at: 1,
        last_backup: 1,
        applied_hash: String::new(),
        manifest: None,
        enabled: true,
        natives: Vec::new(),
        incomplete_step: None,
        route_file: route_file.map(str::to_string),
        extra_route_files: extras.iter().map(|s| s.to_string()).collect(),
    }
}

/// The nightly check names every file in the gateway's routes directory
/// that no stack's deploy recorded — the `.bak` Traefik ignores included,
/// because a file there that nobody owns is exactly what the repository
/// cannot account for.
/// covers: fix-92
#[test]
fn the_nightly_check_names_route_files_no_stack_declares() {
    let mut st = HostState::default();
    st.stacks.insert(
        "gateway".into(),
        recorded(Some("104-app-gateway.yml"), &["manual-routes.yml"]),
    );
    st.stacks
        .insert("kyu".into(), recorded(None, &["manual-kyu.yml"]));
    let on_disk: Vec<String> = [
        "104-app-gateway.yml",
        "manual-routes.yml",
        "manual-kyu.yml",
        "manual-homeassistant.yml",
        "108-app-synctest.yml.bak-20260830-200124",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let findings = evaluate_route_owners(&st, &on_disk);
    let subjects: Vec<&str> = findings.iter().map(|f| f.subject.as_str()).collect();
    assert_eq!(findings.len(), 2, "{:?}", findings);
    assert!(
        subjects
            .iter()
            .any(|s| s.contains("manual-homeassistant.yml"))
    );
    assert!(
        subjects
            .iter()
            .any(|s| s.contains("108-app-synctest.yml.bak-20260830-200124"))
    );
    assert!(findings.iter().all(|f| f.severity == Severity::Drift));
    assert!(
        findings.iter().all(|f| f.remedy.contains("never removes")),
        "the remedy says homelab will not delete it: {:?}",
        findings
    );
}

/// `evaluate` — the nightly round and `homelab check` — carries the route
/// owner check, or the reader below is wired to nothing.
/// covers: fix-92
#[test]
fn the_full_round_carries_the_route_owner_check() {
    use homelab_core::ops::fleetcheck::{GrowthLimits, LiveFacts, evaluate};
    let live = LiveFacts {
        route_files: vec!["manual-x.yml".into()],
        ..Default::default()
    };
    let findings = evaluate(
        &HostState::default(),
        &live,
        1_789_704_000,
        homelab_core::ops::fleetcheck::DEFAULT_BACKUP_MAX_AGE_S,
        GrowthLimits::default(),
        None,
        homelab_core::ops::fleetcheck::PATCH_THRESHOLD_S,
        homelab_core::ops::fleetcheck::HOST_META_MAX_AGE_S,
        homelab_core::ops::fleetcheck::HostCapacityThresholds::default(),
    );
    assert!(
        findings.iter().any(|f| f.subject.contains("manual-x.yml")),
        "{:?}",
        findings
    );
}

/// The nightly round reads every name in the directory, not only `*.yml`:
/// the fact the check above judges.
/// covers: fix-92
#[tokio::test]
async fn the_gatherer_lists_every_file_in_the_routes_directory() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "ls -1A '/appdata/gateway/traefik-config/routes'",
        CmdOutput::ok("104-app-gateway.yml\n108-app-synctest.yml.bak-20260830-200124\n"),
    );
    let inp = FactsInputs {
        watched_backups: vec![],
        state_dir: "/var/lib/homelab".into(),
        loki_vmid: None,
        gateway_vmid: 104,
        gateway_routes_dir: "/appdata/gateway/traefik-config/routes".into(),
        no_touch: vec![100, 101],
        prometheus_url: None,
        loki_url: None,
        logs_window: "24h".into(),
        now_unix: 1_789_704_000,
        watched_fresh: true,
    };
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(
        facts.route_files,
        vec![
            "104-app-gateway.yml".to_string(),
            "108-app-synctest.yml.bak-20260830-200124".to_string()
        ]
    );
    assert!(
        exec.calls_containing("ls -1A")
            .iter()
            .all(|c| c.starts_with("lxc-attach -n 104 ")),
        "read inside the gateway: {:?}",
        exec.calls()
    );
}
