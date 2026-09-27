//! fix-113 (native-tar-no-quiesce, 2026-09-27): a native backup streamed a
//! tar of live data with no way to quiesce the service, no stale-lock
//! cleanup, and the fleet-wide retention whatever the stack file said.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::native::{validate_native, NativeServiceManifest};
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::native::backup_native;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const NOW: u64 = 1_790_000_000;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    }
}

fn almanac(pause: bool) -> NativeServiceManifest {
    NativeServiceManifest {
        stack_name: "almanac".into(),
        vmid: 112,
        hostname: "112-app-almanac".into(),
        unit: "almanac".into(),
        binary: "/opt/almanac/bin/almanac".into(),
        env_file: None,
        data_dirs: vec!["/appdata/almanac/almanac-config".into()],
        update_cmd: None,
        stateless: false,
        release_repo: None,
        release_asset: None,
        backup_from_newest: None,
        backup_pause: pause,
        update_policy: Default::default(),
        metrics: None,
    }
}

fn harness() -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 112",
        CmdOutput::ok("hostname: 112-app-almanac\n"),
    );
    // The data directory holds files, so the fix-63 guard stands aside.
    exec.respond_always(
        "-type f",
        CmdOutput::ok("/appdata/almanac/almanac-config/db\n"),
    );
    exec.respond_always("snapshots --json", CmdOutput::ok("[]"));
    exec.respond_always("is-active", CmdOutput::ok("active\n"));
    exec
}

fn position(calls: &[String], needle: &str) -> usize {
    calls
        .iter()
        .position(|c| c.contains(needle))
        .unwrap_or_else(|| panic!("no call containing {:?}: {:?}", needle, calls))
}

#[tokio::test]
async fn a_stale_lock_is_cleared_before_the_snapshot() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(false),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    assert!(position(&calls, "restic unlock") < position(&calls, "tar -cf -"));
}

#[tokio::test]
async fn a_paused_service_is_stopped_for_the_tar_and_started_again() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(true),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    let stop = position(&calls, "systemctl stop almanac.service");
    let tar = position(&calls, "tar -cf -");
    let start = position(&calls, "systemctl start almanac.service");
    assert!(stop < tar && tar < start, "{:?}", calls);
}

#[tokio::test]
async fn a_failed_snapshot_still_starts_the_paused_service() {
    let exec = harness();
    exec.respond_first(
        "tar -cf -",
        CmdOutput::failed(1, "restic: repository is locked"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(true),
        &BackupCfg::default(),
    )
    .await;
    assert!(!r.ok);
    assert_eq!(
        exec.calls_containing("systemctl start almanac.service")
            .len(),
        1,
        "a backup must never leave the service it paused stopped: {:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn an_unpaused_service_is_never_stopped() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(false),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    assert!(exec.calls_containing("systemctl stop").is_empty());
}

#[tokio::test]
async fn the_stack_files_own_retention_wins_over_the_fleet_tiers() {
    let exec = harness();
    // Two snapshots two days apart: the fleet tiers keep both (daily for a
    // week); the stack's own 30-day tier keeps only the newer.
    exec.respond_first(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"short_id":"older001","time":"2026-09-19T02:00:00Z"},
                {"short_id":"newer002","time":"2026-09-21T02:00:00Z"}]"#,
        ),
    );
    let mut st = homelab_core::state::HostState::default();
    let manifest: homelab_core::manifest::StackManifest = serde_yaml::from_str(
        r#"
stack_name: almanac
vmid: 112
hostname: 112-app-almanac
native_only: true
network: {ip: 10.10.10.12/24, gateway: 10.10.10.1, bridge: vmbr0, vlan: 10}
resources: {cores: 1, memory_mb: 512, swap_mb: 0, disk_gb: 4, storage: local-lvm}
lxc: {template: "clone:998", unprivileged: true, features: "nesting=1", protection: true, gpu: false, vpn: false}
boot: {onboot: true}
storage: []
apps: []
natives: [almanac]
retention:
  - every_days: 30
"#,
    )
    .expect("a native stack file with its own retention");
    st.stacks.insert(
        "almanac".into(),
        homelab_core::state::StackState {
            extra_route_files: Vec::new(),
            vmid: 112,
            hostname: "112-app-almanac".into(),
            apps: vec![],
            applied_at: NOW,
            last_backup: NOW,
            applied_hash: String::new(),
            manifest: Some(manifest),
            native: None,
            natives: vec![almanac(false)],
            enabled: true,
            incomplete_step: None,
            route_file: None,
        },
    );
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &serde_json::to_string(&st).unwrap(),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup_native(
        &ctx(&exec, &sink, &j),
        &almanac(false),
        &BackupCfg::default(),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let forget = exec.calls_containing("restic forget");
    assert_eq!(forget.len(), 1, "{:?}", exec.calls());
    assert!(forget[0].contains("older001") && !forget[0].contains("newer002"));
}

#[test]
fn pausing_a_stateless_service_is_refused() {
    let m = NativeServiceManifest {
        data_dirs: vec![],
        stateless: true,
        ..almanac(true)
    };
    let problems = validate_native(&m).unwrap_err();
    assert!(
        problems.iter().any(|p| p.contains("backup_pause")),
        "{:?}",
        problems
    );
}
