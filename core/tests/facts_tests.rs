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
