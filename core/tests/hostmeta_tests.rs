//! fix-111 (host-meta-gaps, 2026-09-27): the daemon's own repository was not
//! watched, never pruned, and missed files a host rebuild needs.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::backup::{backup_host_meta, BackupCfg, HOST_META_EXTRAS};
use homelab_core::ops::fleetcheck::{evaluate_host_meta, Severity, HOST_META_MAX_AGE_S};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::{HostState, StackState};

const NOW: u64 = 1_760_000_000;

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

/// The VLAN bridges, the storage and job definitions, the VM configurations
/// (Home Assistant's Zigbee USB passthrough), the swappiness drop-in and
/// rclone's remote: none of them was in any backup.
#[tokio::test]
async fn the_host_meta_snapshot_carries_what_a_host_rebuild_needs() {
    let exec = MockExecutor::new();
    exec.respond_always("test -e", CmdOutput::ok("yes\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup_host_meta(&ctx(&exec, &sink, &j), &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let snap = exec
        .calls_containing("restic backup")
        .into_iter()
        .next()
        .expect("a snapshot command");
    for p in [
        "/etc/network/interfaces",
        "/etc/pve/storage.cfg",
        "/etc/pve/jobs.cfg",
        "/etc/pve/qemu-server",
        "/etc/sysctl.d/99-homelab-swappiness.conf",
        "/root/.config/rclone/rclone.conf",
    ] {
        assert!(snap.contains(p), "{} is not in the snapshot: {}", p, snap);
        assert!(
            HOST_META_EXTRAS.contains(&p),
            "the runbook lists the same set: {}",
            p
        );
    }
}

/// Every rotated secret was kept for ever. The fleet-wide tiers prune it now.
#[tokio::test]
async fn host_meta_is_pruned_by_the_fleet_tiers() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"short_id":"old00001","time":"2025-10-06T02:00:00Z"},
                {"short_id":"old00002","time":"2025-10-06T03:00:00Z"},
                {"short_id":"new00003","time":"2025-10-08T03:00:00Z"}]"#,
        ),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup_host_meta(&ctx(&exec, &sink, &j), &BackupCfg::default()).await;
    assert!(report.ok, "{:?}", report.error);
    let forget = exec.calls_containing("restic forget");
    assert_eq!(forget.len(), 1, "{:?}", exec.calls());
    assert!(forget[0].contains("host-meta-config"), "{}", forget[0]);
    assert!(forget[0].contains("old00001") && !forget[0].contains("new00003"));
}

fn fleet_with(last_host_meta: u64) -> HostState {
    let mut st = HostState {
        last_host_meta,
        ..Default::default()
    };
    st.stacks.insert(
        "kyu".into(),
        StackState {
            applied_source: None,
            extra_route_files: Vec::new(),
            vmid: 109,
            hostname: "109-app-kyu".into(),
            apps: vec![],
            applied_at: NOW,
            last_backup: NOW,
            applied_hash: String::new(),
            manifest: None,

            natives: vec![],
            enabled: true,
            incomplete_step: None,
            route_file: None,
        },
    );
    st
}

/// Nobody watched it: a host-meta backup that stopped was reported nowhere.
#[test]
fn a_stale_or_missing_host_meta_backup_is_a_finding() {
    let stale = evaluate_host_meta(&fleet_with(NOW - HOST_META_MAX_AGE_S - 3600), NOW);
    assert_eq!(stale.len(), 1, "{:?}", stale);
    assert_eq!(stale[0].severity, Severity::Broken);
    assert_eq!(stale[0].subject, "host-meta");
    let never = evaluate_host_meta(&fleet_with(0), NOW);
    assert!(never[0].what.contains("never"), "{:?}", never);
    assert!(evaluate_host_meta(&fleet_with(NOW - 3600), NOW).is_empty());
    // A host that manages nothing yet has nothing of its own worth keeping.
    assert!(evaluate_host_meta(&HostState::default(), NOW).is_empty());
}
