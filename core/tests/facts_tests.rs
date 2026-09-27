//! G6 · the fact gatherer, driven through a mock fleet.
//!
//! Every nightly finding starts with these readings. Until T79 they were
//! taken by a host-only function nothing could run without a hypervisor.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::facts::*;

fn inputs() -> FactsInputs {
    FactsInputs {
        watched_backups: vec![],
        kuma_monitors_file: None,
        state_dir: "/var/lib/homelab".into(),
        gateway_vmid: 104,
        gateway_routes_dir: "/appdata/gateway/traefik-config/routes".into(),
        no_touch: vec![100, 101],
        prometheus_url: None,
        loki_url: None,
        logs_window: "24h".into(),
        grafana_dashboards_dir: None,
        now_unix: 1_789_704_000,
    }
}

#[test]
fn g6_pct_list_becomes_vmid_and_hostname_pairs() {
    let got = parse_pct_list(
        "VMID       Status     Lock         Name\n\
         100        running                 100-router\n\
         109        running                 109-app-kyu\n\
         998        stopped                 998-template\n",
    );
    assert_eq!(
        got,
        vec![
            (100, "100-router".into()),
            (109, "109-app-kyu".into()),
            (998, "998-template".into())
        ]
    );
}

/// The `guards` line is the proof the probe ran inside the container. Its
/// absence (a stopped guest) yields no fact at all — the three golden
/// templates once read as unguarded because every field kept its zero.
#[test]
fn g6_a_growth_probe_that_never_ran_is_no_fact() {
    let g = parse_growth(
        109,
        "109-app-kyu",
        "disk=46\nmem=37\nswap=0\njournal=12\ndockerlogs=\nguards=1\n",
    )
    .expect("probed");
    assert_eq!(g.disk_used_pct, 46);
    assert_eq!(g.mem_used_pct, 37);
    assert_eq!(g.journal_mb, 12);
    assert_eq!(
        g.docker_logs_mb, 0,
        "an empty du reads as 0, not as a failure"
    );
    assert!(g.guards);
    assert!(parse_growth(998, "998-template", "").is_none());
    assert!(parse_growth(998, "998-template", "disk=90\n").is_none());
}

#[test]
fn g6_host_memory_is_five_numbers_in_the_script_order() {
    // total, swap_used, swap_total, lxc_committed, vm_committed
    assert_eq!(
        parse_host_memory("31000\n6498 8178\n12000\n35000\n"),
        Some((31000, 47000, 6498, 8178))
    );
    assert_eq!(
        parse_host_memory("31000\n6498 8178\n"),
        None,
        "half an answer is none"
    );
    assert_eq!(parse_host_memory("x y z w v"), None);
}

#[test]
fn g6_the_logs_window_only_passes_digits_and_a_unit() {
    assert_eq!(sane_window("24h", "1h"), "24h");
    assert_eq!(sane_window("7d", "1h"), "7d");
    assert_eq!(sane_window("24 hours", "1h"), "1h");
    assert_eq!(sane_window("h", "1h"), "1h");
    assert_eq!(sane_window("24h;rm", "1h"), "1h");
}

/// Containers come from `pct list`; the untouchable ones are never probed;
/// a stopped guest that answers nothing produces no growth fact but still
/// a boot fact, because `pct config` can be asked about a stopped guest.
#[tokio::test]
async fn g6_managed_containers_are_probed_and_the_untouchable_are_not() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct list",
        CmdOutput::ok(
            "VMID Status Lock Name\n100 running 100-router\n109 running 109-app-kyu\n998 stopped 998-template\n",
        ),
    );
    exec.respond_always(
        "pct exec 109 -- sh -c df",
        CmdOutput::ok("disk=46\nmem=37\nswap=0\njournal=12\ndockerlogs=3\nguards=1\n"),
    );
    exec.respond_always("pct exec 998 -- sh -c df", CmdOutput::ok(""));
    exec.respond_always("pct config 109", CmdOutput::ok("onboot: 1\nmemory: 256\n"));
    exec.respond_always("pct config 998", CmdOutput::ok("memory: 2048\n"));
    let (facts, _) = gather_live_facts(&exec, &inputs(), &[]).await;
    assert_eq!(facts.containers.len(), 3);
    assert_eq!(facts.growth.len(), 1, "{:?}", facts.growth);
    assert_eq!(facts.growth[0].vmid, 109);
    assert!(facts.growth[0].guards);
    assert_eq!(
        facts.boot.len(),
        2,
        "109 and 998, never 100: {:?}",
        facts.boot
    );
    assert!(
        exec.calls_containing("pct exec 100").is_empty()
            && exec.calls_containing("pct config 100").is_empty(),
        "the no-touch list is honoured absolutely: {:?}",
        exec.calls()
    );
}

/// Every route fragment on the gateway is read, its target extracted, and
/// the target knocked on from inside the gateway with bash's /dev/tcp.
#[tokio::test]
async fn g6_routes_are_read_from_the_gateway_and_knocked_on() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "for f in /appdata/gateway/traefik-config/routes/*.yml",
        CmdOutput::ok(
            &[
                "### 110-app-syncthing.yml",
                "http:",
                "  services:",
                "    syncthing:",
                "      loadBalancer:",
                "        servers:",
                "          - url: \"http://10.10.10.10:8384\"",
                "### 106-app-media.yml",
                "          - url: http://10.10.10.6:8096",
                "",
            ]
            .join("\n"),
        ),
    );
    exec.respond_always("/dev/tcp/10.10.10.10/8384", CmdOutput::ok("up\n"));
    exec.respond_always("/dev/tcp/10.10.10.6/8096", CmdOutput::ok("down\n"));
    let (facts, _) = gather_live_facts(&exec, &inputs(), &[]).await;
    assert_eq!(facts.routes.len(), 2, "{:?}", facts.routes);
    let sync = facts
        .routes
        .iter()
        .find(|r| r.file == "110-app-syncthing.yml")
        .unwrap();
    assert!(sync.answered);
    assert_eq!(sync.target, "http://10.10.10.10:8384");
    let media = facts
        .routes
        .iter()
        .find(|r| r.file == "106-app-media.yml")
        .unwrap();
    assert!(!media.answered);
    assert!(
        exec.calls_containing("pct exec 104 --")
            .iter()
            .all(|c| c.contains("bash -c") || c.contains("for f in")),
        "the knock uses bash, not dash: {:?}",
        exec.calls_containing("/dev/tcp")
    );
}

/// O1: a watched backup's age is the difference between now and the newest
/// file rclone lists; a listing that fails is an error, not an old backup.
#[tokio::test]
async fn g6_a_watched_backup_is_aged_against_now_and_a_failed_listing_is_an_error() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "rclone lsjson --files-only 'gdrive:ok'",
        CmdOutput::ok("2026-09-19T01:00:00Z\n"),
    );
    exec.respond_always(
        "date -d 2026-09-19T01:00:00Z",
        CmdOutput::ok("1789700400\n"),
    );
    exec.respond_always(
        "rclone lsjson --files-only 'gdrive:broken'",
        CmdOutput::failed(1, "directory not found"),
    );
    let mut inp = inputs();
    inp.watched_backups = vec![
        WatchedBackupSpec {
            name: "opnsense".into(),
            rclone_path: "gdrive:ok".into(),
            max_age_hours: 26,
        },
        WatchedBackupSpec {
            name: "nothing".into(),
            rclone_path: "gdrive:broken".into(),
            max_age_hours: 26,
        },
    ];
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(facts.watched_backups.len(), 2);
    let ok = &facts.watched_backups[0];
    assert_eq!(ok.newest_age_s, Some(3600));
    assert_eq!(ok.max_age_s, 26 * 3600);
    assert!(ok.error.is_none());
    let broken = &facts.watched_backups[1];
    assert!(broken.newest_age_s.is_none());
    assert!(
        broken.error.as_deref().unwrap_or("").contains("not found"),
        "{:?}",
        broken
    );
}

/// T49: the seeder's verdict is read from the file beside the monitor list;
/// no configured file means nothing to judge, which is not a finding.
#[tokio::test]
async fn g6_the_seed_verdict_comes_from_last_seed_json_beside_the_monitors() {
    let exec = MockExecutor::new();
    exec.seed_file(
        "/appdata/uptime/kuma-seeder-config/last-seed.json",
        r#"{"at": 1789700400, "judged": true, "stale": ["host · drill"]}"#,
    );
    let mut inp = inputs();
    inp.kuma_monitors_file = Some("/appdata/uptime/kuma-seeder-config/host-monitors.json".into());
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(facts.seed.age_s, Some(3600));
    assert!(facts.seed.judged);
    assert_eq!(facts.seed.stale, vec!["host · drill".to_string()]);
    assert!(facts.seed.error.is_none());

    let (facts, _) = gather_live_facts(&MockExecutor::new(), &inputs(), &[]).await;
    assert!(facts.seed.judged, "unconfigured = nothing to judge");
    assert_eq!(facts.seed.age_s, Some(0));
}

/// Coverage is asked only where an address is configured, per recorded
/// stack; Prometheus' answer is read for a `1`, and an unconfigured Loki
/// leaves the logs question unasked rather than answered.
#[tokio::test]
async fn g6_coverage_asks_prometheus_per_recorded_stack_and_leaves_loki_unasked() {
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::json!({
            "schema_version": 1,
            "stacks": {
                "media": {"vmid": 106, "hostname": "106-app-media", "apps": ["jellyfin"], "applied_at": 1, "manifest": null},
                "kyu": {"vmid": 109, "hostname": "109-app-kyu", "apps": [], "applied_at": 1, "manifest": null}
            }
        })
        .to_string(),
    );
    exec.respond_always(
        "up%7Bstack%3D%22media%22",
        CmdOutput::ok(r#"{"data":{"result":[{"value":[1,"1"]}]}}"#),
    );
    exec.respond_always(
        "up%7Bstack%3D%22kyu%22",
        CmdOutput::ok(r#"{"data":{"result":[]}}"#),
    );
    let mut inp = inputs();
    inp.prometheus_url = Some("http://10.10.10.13:9090/".into());
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(facts.coverage.len(), 2, "{:?}", facts.coverage);
    let media = facts.coverage.iter().find(|c| c.stack == "media").unwrap();
    assert_eq!(media.scraped, Some(true));
    assert_eq!(
        media.logs_recent, None,
        "Loki was not configured, so not asked"
    );
    assert_eq!(
        media.dashboard_provisioned, None,
        "Grafana was not configured"
    );
    let kyu = facts.coverage.iter().find(|c| c.stack == "kyu").unwrap();
    assert_eq!(kyu.scraped, Some(false));
    assert!(
        exec.calls_containing("loki/api").is_empty(),
        "no Loki question without a Loki address: {:?}",
        exec.calls()
    );
}

/// Nothing configured, nothing recorded: the gatherer still returns, with
/// every unasked question absent rather than failed.
#[tokio::test]
async fn g6_an_empty_fleet_yields_empty_facts_not_findings() {
    let exec = MockExecutor::new();
    let (facts, notes) = gather_live_facts(&exec, &inputs(), &[("kyu".into(), 109)]).await;
    assert!(facts.containers.is_empty());
    assert!(facts.growth.is_empty());
    assert!(facts.routes.is_empty());
    assert!(facts.coverage.is_empty());
    assert!(facts.pools.is_empty());
    assert!(facts.watched_backups.is_empty());
    assert_eq!(facts.stack_files, vec![("kyu".to_string(), 109)]);
    assert!(
        notes.is_empty(),
        "nothing measured, nothing to say: {:?}",
        notes
    );
}

/// inbox-guards (Kenny, Phase 9 form 2026-09-27): a native service whose
/// service.yml says `metrics: false` is deliberately not measured. Prometheus
/// is not asked about it, and the fleet check notes it instead of reporting
/// drift every run.
///
/// covers: step-20
#[tokio::test]
async fn step_20_a_service_declared_unmeasured_is_not_asked_and_is_noted() {
    use homelab_core::ops::fleetcheck::{evaluate_coverage, Severity};
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::json!({
            "schema_version": 1,
            "stacks": {
                "inbox": {"vmid": 118, "hostname": "118-app-inbox", "apps": ["inbox"], "applied_at": 1, "manifest": null,
                    "natives": [{"stack_name": "inbox", "vmid": 118, "hostname": "118-app-inbox", "unit": "inbox",
                                 "binary": "/opt/inbox/bin/inbox", "metrics": false}]}
            }
        })
        .to_string(),
    );
    let mut inp = inputs();
    inp.prometheus_url = Some("http://10.10.10.13:9090/".into());
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert!(
        exec.calls_containing("up%7Bstack%3D%22inbox%22").is_empty(),
        "Prometheus must not be asked about a service declared unmeasured"
    );
    let findings = evaluate_coverage(&facts.coverage);
    assert_eq!(findings.len(), 1, "{:?}", findings);
    assert_eq!(findings[0].severity, Severity::Noted);
    assert!(
        findings[0].what.contains("deliberately"),
        "{:?}",
        findings[0]
    );
}

/// gap-27 (Kenny, Phase 9 form 2026-09-27: "Echt maken"): the doctor's env
/// check was fed "sealed" for every stack and could never fire. A stack is
/// sealed when every secret file its containers hold (a compose app's `.env`,
/// a native unit's env file) has a copy in the host's vault, so a lost
/// container can be rebuilt without latch.
///
/// covers: gap-27
#[tokio::test]
async fn gap_27_a_secret_file_without_a_vault_copy_is_named_unsealed() {
    use homelab_core::ops::facts::unsealed_secret_files;
    let st: homelab_core::state::StackState = serde_json::from_value(serde_json::json!({
        "vmid": 116, "hostname": "116-app-kp-soft", "apps": ["web", "db"], "applied_at": 1,
        "manifest": null,
        "natives": [{"stack_name": "kp-soft", "vmid": 116, "hostname": "116-app-kp-soft",
                     "unit": "job", "binary": "/usr/local/bin/job",
                     "env_file": "/appdata/kp-soft/job-config/job.env"}]
    }))
    .unwrap();
    let exec = MockExecutor::new();
    // web has an .env on the container, db has none, the native unit has one.
    exec.respond_always("test -s '/opt/kp-soft/web/.env'", CmdOutput::ok("yes"));
    exec.respond_always("test -s '/opt/kp-soft/db/.env'", CmdOutput::ok(""));
    exec.respond_always(
        "test -s '/appdata/kp-soft/job-config/job.env'",
        CmdOutput::ok("yes"),
    );
    let missing = unsealed_secret_files(&exec, "/var/lib/homelab", "kp-soft", &st).await;
    assert_eq!(
        missing,
        vec![
            "/opt/kp-soft/web/.env".to_string(),
            "/appdata/kp-soft/job-config/job.env".to_string()
        ]
    );
    exec.seed_file("/var/lib/homelab/secrets/kp-soft/web.env", "X=1\n");
    exec.seed_file(
        "/var/lib/homelab/secrets/kp-soft/job-config/job.env",
        "Y=1\n",
    );
    let missing = unsealed_secret_files(&exec, "/var/lib/homelab", "kp-soft", &st).await;
    assert!(missing.is_empty(), "{missing:?}");
}

/// gap-33 follow-up: a container without docker is guarded by the journald
/// cap alone. The probe required the docker log cap everywhere, so inbox on
/// CT 118 (no docker) was reported unguarded after its guards were applied.
/// Runs the probe's guard clause in `sh` with no docker on PATH.
///
/// covers: gap-33
#[test]
fn gap_33_a_container_without_docker_needs_only_the_journald_cap() {
    use homelab_core::ops::facts::GROWTH_PROBE;
    let clause = GROWTH_PROBE
        .split("if ls /etc/systemd")
        .nth(1)
        .expect("the guard clause");
    // Replace the journald check with `true` (this machine's /etc is not a
    // container's) and keep the docker half exactly as shipped.
    let clause = format!("if true{}", &clause[clause.find(" && ").unwrap()..]);
    let out = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(&clause)
        .env("PATH", "/nonexistent")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "guards=1");
}

/// Expert panel 2026-09-27 (container-logs-missing-in-loki): the coverage
/// check only asked Loki about stacks listing an app named `promtail`, and
/// since the fleet moved to Alloy (2026-09-02) no stack does, so the check
/// never fired while no container line arrived for 24 days. It now asks every
/// compose stack for labelled container lines and every native stack for
/// journal lines with a `unit` label, which is what Alloy ships for each.
#[tokio::test]
async fn coverage_asks_loki_for_every_stack_without_a_promtail_app() {
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::json!({
            "schema_version": 1,
            "stacks": {
                "media": {"vmid": 106, "hostname": "106-app-media", "apps": ["jellyfin"], "applied_at": 1, "manifest": null},
                "kyu": {"vmid": 109, "hostname": "109-app-kyu", "apps": [], "applied_at": 1, "manifest": null,
                        "natives": [{"stack_name": "kyu", "vmid": 109, "hostname": "109-app-kyu", "unit": "kyu", "binary": "/usr/local/bin/kyu"}]}
            }
        })
        .to_string(),
    );
    exec.respond_always(
        "stack%3D%22media%22%2Ccontainer_name",
        CmdOutput::ok(r#"{"data":{"result":[]}}"#),
    );
    exec.respond_always(
        "stack%3D%22kyu%22%2Cunit",
        CmdOutput::ok(r#"{"data":{"result":[{"value":[1,"42"]}]}}"#),
    );
    let mut inp = inputs();
    inp.loki_url = Some("http://10.10.10.4:3100".into());
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    let media = facts.coverage.iter().find(|c| c.stack == "media").unwrap();
    assert_eq!(media.logs_recent, Some(false), "{:?}", exec.calls());
    let kyu = facts.coverage.iter().find(|c| c.stack == "kyu").unwrap();
    assert_eq!(kyu.logs_recent, Some(true), "{:?}", exec.calls());
}
