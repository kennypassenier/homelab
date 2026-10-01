//! G6 · the fact gatherer, driven through a mock fleet.
//!
//! Every nightly finding starts with these readings. Until T79 they were
//! taken by a host-only function nothing could run without a hypervisor.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::facts::*;

/// How a read-only probe inside container `vmid` renders: `lxc-attach`
/// directly, not `pct exec` (pct's Perl start-up is 0.45 s a call).
fn attach(vmid: u16) -> String {
    format!(
        "lxc-attach -n {} --clear-env --set-var {} -- ",
        vmid,
        homelab_core::executor::ATTACH_PATH
    )
}

fn inputs() -> FactsInputs {
    FactsInputs {
        watched_backups: vec![],
        state_dir: "/var/lib/homelab".into(),
        gateway_vmid: 104,
        gateway_routes_dir: "/appdata/gateway/traefik-config/routes".into(),
        no_touch: vec![100, 101],
        prometheus_url: None,
        loki_url: None,
        loki_vmid: None,
        logs_window: "24h".into(),
        now_unix: 1_789_704_000,
        watched_fresh: true,
    }
}

/// long-silences (expert panel, 2026-09-27): `homelab check` took 41 s with
/// nothing on the screen after "link up". The gatherer reports each phase,
/// and each container it probes, as it goes.
/// covers: fix-104
#[tokio::test]
async fn fix_104_the_fact_gatherer_reports_each_phase_as_it_goes() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct list",
        CmdOutput::ok(
            "VMID Status Lock Name\n109 running 109-app-kyu\n112 running 112-app-almanac\n",
        ),
    );
    let lines = std::sync::Mutex::new(Vec::<String>::new());
    let progress = |l: &str| lines.lock().unwrap().push(l.to_string());
    let _ = gather_live_facts_with(&exec, &inputs(), &[], &progress).await;
    let lines = lines.into_inner().unwrap();
    assert!(
        lines.iter().any(|l| l.contains("probing 2 container(s)")),
        "{:?}",
        lines
    );
    assert!(lines.iter().any(|l| l.contains("1/2")), "{:?}", lines);
    assert!(lines.iter().any(|l| l.contains("2/2")), "{:?}", lines);
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
/// a boot fact, because a stopped guest's configuration can still be read.
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
        &format!("{}sh -c df", attach(109)),
        CmdOutput::ok("disk=46\nmem=37\nswap=0\njournal=12\ndockerlogs=3\nguards=1\n"),
    );
    exec.respond_always(&format!("{}sh -c df", attach(998)), CmdOutput::ok(""));
    exec.seed_file("/etc/pve/lxc/109.conf", "onboot: 1\nmemory: 256\n");
    exec.seed_file("/etc/pve/lxc/998.conf", "memory: 2048\n");
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
            && exec.calls_containing("pct config 100").is_empty()
            && exec.calls_containing("-n 100 ").is_empty()
            && exec.calls_containing("/100.conf").is_empty(),
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
        !exec.calls_containing(&attach(104)).is_empty(),
        "asked inside the gateway: {:?}",
        exec.calls()
    );
    assert!(
        exec.calls_containing(&attach(104))
            .iter()
            // fix-92 adds the one listing of the directory's names.
            .all(|c| c.contains("bash -c") || c.contains("for f in") || c.contains("ls -1A")),
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
    use homelab_core::ops::fleetcheck::{Severity, evaluate_coverage};
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
    inp.loki_url = Some("http://10.10.10.13:3100".into());
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    let media = facts.coverage.iter().find(|c| c.stack == "media").unwrap();
    assert_eq!(media.logs_recent, Some(false), "{:?}", exec.calls());
    let kyu = facts.coverage.iter().find(|c| c.stack == "kyu").unwrap();
    assert_eq!(kyu.logs_recent, Some(true), "{:?}", exec.calls());
}

/// fix-93 (expert panel 2026-09-27, loki-unauthenticated-open): the LAN port
/// takes pushes only, so the host can no longer read Loki from outside. With
/// `loki_vmid` set it asks from inside Loki's container, on the loopback port
/// the stack publishes there, and the push address stays what Alloy uses.
/// covers: fix-93
#[tokio::test]
async fn fix_93_the_log_question_is_asked_inside_lokis_container() {
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::json!({
            "schema_version": 1,
            "stacks": {
                "media": {"vmid": 106, "hostname": "106-app-media", "apps": ["jellyfin"], "applied_at": 1, "manifest": null}
            }
        })
        .to_string(),
    );
    exec.respond_always(
        "stack%3D%22media%22%2Ccontainer_name",
        CmdOutput::ok(r#"{"data":{"result":[{"value":[1,"42"]}]}}"#),
    );
    let mut inp = inputs();
    inp.loki_url = Some("http://10.10.10.13:3100".into());
    inp.loki_vmid = Some(113);
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    let asked = exec.calls_containing("loki/api/v1/query");
    assert_eq!(asked.len(), 1, "{asked:?}");
    let inside = format!(
        "{}/loki/api/v1/query?",
        homelab_core::ops::logshipper::LOKI_QUERY_LOOPBACK
    );
    assert!(
        asked[0].starts_with(&format!("{}curl ", attach(113))) && asked[0].contains(&inside),
        "{asked:?}"
    );
    assert!(!asked[0].contains("10.10.10.13:3100"), "{asked:?}");
    let media = facts.coverage.iter().find(|c| c.stack == "media").unwrap();
    assert_eq!(media.logs_recent, Some(true));
}

/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): the host's
/// intent-history copy of each stack the client named, hashed file by file,
/// so the check can say which files differ. A stack whose copy cannot be
/// read is absent, never "has no files".
#[tokio::test]
async fn fix_142_the_intent_copy_is_read_file_by_file() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "/var/lib/homelab/repo/stacks/home",
        CmdOutput::ok(
            "aaa111  ./app/docker-compose.yml\n\
             bbb222  ./rootfs/usr/local/bin/tool\n",
        ),
    );
    exec.respond_always(
        "/var/lib/homelab/repo/stacks/gone",
        CmdOutput::failed(1, "sh: cd: can't cd"),
    );
    exec.respond_always("/var/lib/homelab/repo/stacks/empty", CmdOutput::ok(""));
    let got = intent_files(
        &exec,
        "/var/lib/homelab",
        &["home".to_string(), "gone".to_string(), "empty".to_string()],
    )
    .await;
    let home = got.get("home").expect("home read");
    assert_eq!(home.len(), 2);
    assert_eq!(home["app/docker-compose.yml"], "aaa111");
    assert_eq!(home["rootfs/usr/local/bin/tool"], "bbb222");
    assert!(!got.contains_key("gone"), "unread is absent: {got:?}");
    assert!(got.get("empty").is_some_and(|m| m.is_empty()), "{got:?}");
    // A name that could leave the repository is never asked about.
    let odd = intent_files(&exec, "/var/lib/homelab", &["../etc".to_string()]).await;
    assert!(odd.is_empty());
}

/// Decision "Fleet check speed" (2026-09-29, backup-read = reuse): the check
/// reads the watched backups the host recorded and asks rclone nothing.
#[tokio::test]
async fn speed_a_check_with_recorded_backups_asks_no_remote() {
    use homelab_core::ops::watched::{WATCHED_BACKUPS_FILE, WatchedRecord, WatchedRecords};
    let exec = MockExecutor::new();
    let mut rec = WatchedRecords::new();
    rec.insert(
        "opnsense".into(),
        WatchedRecord {
            rclone_path: "gdrive:ok".into(),
            newest_unix: Some(1_789_700_400),
            learned_at: 1_789_701_000,
            source: "nightly".into(),
        },
    );
    exec.seed_file(
        &format!("/var/lib/homelab/{}", WATCHED_BACKUPS_FILE),
        &serde_json::to_string(&rec).unwrap(),
    );
    let mut inp = inputs();
    inp.watched_fresh = false;
    inp.watched_backups = vec![WatchedBackupSpec {
        name: "opnsense".into(),
        rclone_path: "gdrive:ok".into(),
        max_age_hours: 26,
    }];
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(
        exec.calls_containing("rclone").len(),
        0,
        "{:?}",
        exec.calls()
    );
    assert_eq!(
        exec.calls_containing("restic").len(),
        0,
        "{:?}",
        exec.calls()
    );
    assert_eq!(
        exec.calls_containing("date -d").len(),
        0,
        "{:?}",
        exec.calls()
    );
    let w = &facts.watched_backups[0];
    assert_eq!(w.newest_age_s, Some(3600));
    assert_eq!(w.max_age_s, 26 * 3600);
    assert!(w.error.is_none());
}

/// No record yet (the first check after the upgrade, or a watcher moved to
/// another path): asked once, recorded, and the next check asks nothing.
#[tokio::test]
async fn speed_a_check_without_a_record_asks_once_and_records_it() {
    use homelab_core::ops::watched::{WATCHED_BACKUPS_FILE, WatchedRecord, WatchedRecords, load};
    let exec = MockExecutor::new();
    exec.respond_always(
        "rclone lsjson --files-only 'gdrive:ok'",
        CmdOutput::ok("2026-09-19T01:00:00Z\n"),
    );
    exec.respond_always(
        "date -d 2026-09-19T01:00:00Z",
        CmdOutput::ok("1789700400\n"),
    );
    // A record about another path does not count.
    let mut rec = WatchedRecords::new();
    rec.insert(
        "opnsense".into(),
        WatchedRecord {
            rclone_path: "gdrive:old-place".into(),
            newest_unix: Some(1),
            learned_at: 1,
            source: "nightly".into(),
        },
    );
    exec.seed_file(
        &format!("/var/lib/homelab/{}", WATCHED_BACKUPS_FILE),
        &serde_json::to_string(&rec).unwrap(),
    );
    let mut inp = inputs();
    inp.watched_fresh = false;
    inp.watched_backups = vec![WatchedBackupSpec {
        name: "opnsense".into(),
        rclone_path: "gdrive:ok".into(),
        max_age_hours: 26,
    }];
    let (facts, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(facts.watched_backups[0].newest_age_s, Some(3600));
    assert_eq!(exec.calls_containing("rclone lsjson").len(), 1);
    let saved = load(&exec, "/var/lib/homelab").await;
    assert_eq!(saved["opnsense"].rclone_path, "gdrive:ok");
    assert_eq!(saved["opnsense"].newest_unix, Some(1_789_700_400));

    let (again, _) = gather_live_facts(&exec, &inp, &[]).await;
    assert_eq!(
        exec.calls_containing("rclone lsjson").len(),
        1,
        "asked twice"
    );
    assert_eq!(again.watched_backups[0].newest_age_s, Some(3600));
}

/// The nightly round always lists, and its answer replaces the record; a
/// listing that fails is reported that night and never recorded.
#[tokio::test]
async fn speed_the_nightly_round_lists_and_records_but_not_a_failure() {
    use homelab_core::ops::watched::{WATCHED_BACKUPS_FILE, WatchedRecord, WatchedRecords, load};
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
    let old = WatchedRecord {
        rclone_path: "gdrive:broken".into(),
        newest_unix: Some(1_789_600_000),
        learned_at: 1_789_600_000,
        source: "nightly".into(),
    };
    let mut rec = WatchedRecords::new();
    rec.insert("nothing".into(), old.clone());
    rec.insert(
        "opnsense".into(),
        WatchedRecord {
            rclone_path: "gdrive:ok".into(),
            newest_unix: Some(5),
            learned_at: 5,
            source: "nightly".into(),
        },
    );
    exec.seed_file(
        &format!("/var/lib/homelab/{}", WATCHED_BACKUPS_FILE),
        &serde_json::to_string(&rec).unwrap(),
    );
    let mut inp = inputs();
    inp.watched_fresh = true;
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
    assert_eq!(exec.calls_containing("rclone lsjson").len(), 2);
    assert_eq!(facts.watched_backups[0].newest_age_s, Some(3600));
    assert!(facts.watched_backups[1].error.is_some());
    let saved = load(&exec, "/var/lib/homelab").await;
    assert_eq!(saved["opnsense"].newest_unix, Some(1_789_700_400));
    assert_eq!(saved["opnsense"].source, "nightly");
    assert_eq!(
        saved["nothing"], old,
        "a failed listing replaced the last good one"
    );
}

/// A device backup this suite made is recorded for the watcher on its
/// repository, in either spelling F259 accepts, and for no other watcher.
#[tokio::test]
async fn speed_a_device_backup_records_its_watchers() {
    use homelab_core::ops::watched::{feeds, load, record_device_backup};
    assert!(feeds(
        "gdrive:homelab-backups/opnsense-config/snapshots",
        "rclone:gdrive:homelab-backups",
        "opnsense"
    ));
    assert!(feeds(
        "rclone:gdrive:homelab-backups/opnsense-config",
        "rclone:gdrive:homelab-backups",
        "opnsense"
    ));
    assert!(!feeds(
        "gdrive:homelab-backups/opnsense-config-old",
        "rclone:gdrive:homelab-backups",
        "opnsense"
    ));
    let exec = MockExecutor::new();
    let watchers = vec![
        (
            "router".to_string(),
            "gdrive:homelab-backups/opnsense-config/snapshots".to_string(),
        ),
        ("other".to_string(), "gdrive:elsewhere".to_string()),
    ];
    let fed = record_device_backup(
        &exec,
        "/var/lib/homelab",
        &watchers,
        "rclone:gdrive:homelab-backups",
        "opnsense",
        1_789_704_000,
    )
    .await;
    assert_eq!(fed, vec!["router".to_string()]);
    let saved = load(&exec, "/var/lib/homelab").await;
    assert_eq!(saved["router"].newest_unix, Some(1_789_704_000));
    assert!(!saved.contains_key("other"));
    assert!(
        exec.calls().iter().all(|c| c.starts_with("write_file ")),
        "recording asks nothing: {:?}",
        exec.calls()
    );
}

/// Measured on pve 2026-09-29 15:14: `pct config` for each of 19 containers
/// took 9.25 s, the bulk of the fleet check's first stage; the same
/// committed-memory total (38144) reads from the config files in 2 ms.
#[tokio::test]
async fn host_memory_reads_the_config_files_not_pct_config() {
    let exec = MockExecutor::new();
    exec.respond_always("free -m", CmdOutput::ok("64000\n0 8192\n38144\n4096\n"));
    let m = read_host_memory(&exec).await;
    assert_eq!(m, Some((64000, 42240, 0, 8192)));
    let calls = exec.calls();
    assert_eq!(calls.len(), 1, "{:?}", calls);
    assert!(!calls[0].contains("pct config"), "{}", calls[0]);
    assert!(calls[0].contains("/etc/pve/lxc/"), "{}", calls[0]);
}
