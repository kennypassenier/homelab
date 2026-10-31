//! The doctor probes, incident bundles and the daemon's own records.

use super::support::*;
use crate::*;

/// H8: the probe layer feeds real data — stale backups and a dead
/// offsite remote must surface, healthy state must not.
///
/// fix-218: the backup-age reading now comes from the host's
/// `SnapshotCache` (the same one `homelab snapshots` answers from), not
/// `StackState::last_backup` — so this test deliberately sets
/// `last_backup` to `now` (the live fault's exact shape: a cached
/// timestamp that reads as fresh) and proves the age comes out stale
/// anyway, from the repository's own newest snapshot.
#[tokio::test]
pub(crate) async fn doctor_probes_surface_stale_backup_and_dead_offsite() {
    use homelab_core::executor::{CmdOutput, MockExecutor};
    let now = 1_800_000_000u64;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &format!(
            r#"{{"schema_version":1,"stacks":{{"synctest":{{"vmid":108,"hostname":"108-app-synctest","apps":["syncthing"],"applied_at":1,"last_backup":{now},"natives":[{{"stack_name":"synctest","vmid":108,"hostname":"108-app-synctest","unit":"synctest","binary":"/opt/synctest/bin/synctest","data_dirs":["/var/lib/synctest"]}}]}}}}}}"#,
        ),
    );
    // Present = Proxmox holds a configuration for it (what `pct status`
    // answered), read off pmxcfs.
    exec.seed_file("/etc/pve/lxc/108.conf", "hostname: 108-app-synctest\n");
    exec.respond_always(
        "listremotes",
        CmdOutput::ok(
            "gdrive:
",
        ),
    );
    exec.respond_always(
        "lsd gdrive:homelab-backups",
        CmdOutput::failed(3, "token expired"),
    );
    // The repository's own newest snapshot is 80h old — stale, even
    // though `last_backup` above claims `now`.
    exec.respond_always(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"time":"2023-11-11T14:13:20Z","short_id":"ab12cd34","id":"ab12cd34ef"}]"#,
        ),
    );
    let snapshot_cache =
        std::sync::Arc::new(homelab_core::ops::snapshot_cache::SnapshotCache::new(3));
    let backup_cfg = homelab_core::ops::backup::BackupCfg::default();
    // 2023-11-11T14:13:20Z is exactly 80h before 1_700_000_000; `now`
    // here is unrelated to that epoch, so the cache is seeded directly
    // with the same reading `gather_probes` would otherwise have to kick
    // a background refresh for.
    snapshot_cache
        .refresh(&exec, &backup_cfg, "synctest", 1_700_000_000)
        .await;
    let probes = gather_probes(
        &exec,
        "/var/lib/homelab",
        None,
        None,
        homelab_core::ops::restoredrill::DEFAULT_DRILL_SCRATCH_DIR,
        1_700_000_000,
        &snapshot_cache,
        &backup_cfg,
        &|_| {},
    )
    .await;
    assert_eq!(probes.managed_stacks.len(), 1);
    assert!(
        probes.managed_stacks[0].snapshot_read,
        "the cache was seeded before gather_probes ran"
    );
    assert_eq!(probes.managed_stacks[0].snapshot_age_h, Some(80));
    assert!(probes.managed_stacks[0].container_present);
    assert!(probes.offsite_configured);
    assert!(!probes.offsite_token_valid, "expired token must show");
    // And the diagnosis flags both problems — a fresh `last_backup` must
    // not paper over the stale real snapshot (the live fault, fix-218).
    let checks = homelab_core::doctor::diagnose(&probes);
    assert!(
        checks
            .iter()
            .any(|c| c.health != homelab_core::doctor::Health::Ok)
    );
    let backup_line = checks
        .iter()
        .find(|c| c.name == "stack synctest backup")
        .expect("a stack synctest backup line");
    assert_eq!(backup_line.health, homelab_core::doctor::Health::Warn);
    assert!(
        backup_line.detail.contains("80h ago"),
        "{}",
        backup_line.detail
    );
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the new
/// doctor probes read the machine: file modes, privileged containers
/// (never a no-touch guest's configuration), the host-meta and drill
/// records in state, the password file and Drive's space.
#[tokio::test]
pub(crate) async fn fix_130_the_new_doctor_probes_read_the_machine() {
    use homelab_core::executor::{CmdOutput, MockExecutor};
    let now = 1_800_000_000u64;
    let exec = MockExecutor::new();
    exec.seed_file(
        "/var/lib/homelab/state.json",
        &format!(
            r#"{{"schema_version":1,"stacks":{{}},"last_host_meta":{},
            "last_restore_drill":{},"restore_drills":{{"kyu-config":{{"last_attempt":{},"last_pass":0,"last_error":"the restore itself failed"}}}}}}"#,
            now - 30 * 3600,
            now - 10 * 86_400,
            now - 86_400
        ),
    );
    exec.respond_always(
        "stat -c",
        CmdOutput::ok("644 /etc/homelab/host.toml\n600 /var/lib/homelab/tls-key.pem\n700 /var/lib/homelab/incidents\n"),
    );
    exec.respond_always("unprivileged: 1", CmdOutput::ok("105\n106\n108\n"));
    exec.respond_always("test -s", CmdOutput::ok(""));
    exec.respond_always("listremotes", CmdOutput::ok("gdrive:\n"));
    exec.respond_always(
        "about gdrive:",
        CmdOutput::ok(
            r#"{"total":107374182400,"used":90000000000,"trashed":2147483648,"free":17374182400}"#,
        ),
    );
    exec.respond_always(
        "ls -1A '/opt/traefik-config/routes'",
        CmdOutput::ok("104-app-gateway.yml\nmanual-leftover.yml\n"),
    );
    let pc = ProbeContext {
        listen: "0.0.0.0:8443".into(),
        exec_enabled: false,
        config_path: "/etc/homelab/host.toml".into(),
        password_file: "/var/lib/homelab/secrets/restic.pass".into(),
        privileged_vmids: vec![105, 106],
        no_touch: vec![100, 101, 102, 103],
        drill_interval_s: 90 * 86_400,
        state_dir: "/var/lib/homelab".into(),
        gateway_vmid: 104,
        gateway_routes_dir: "/opt/traefik-config/routes".into(),
    };
    // As `gather_probes` leaves it when the gdrive remote exists.
    let mut probes = homelab_core::doctor::Probes {
        offsite_configured: true,
        ..Default::default()
    };
    gather_security_probes(&exec, &pc, now, &mut probes).await;
    assert_eq!(
        probes.loose_files,
        Some(vec!["/etc/homelab/host.toml (644)".to_string()])
    );
    let pv = probes.privileged.expect("privileged probed");
    assert_eq!(
        (pv.vmids, pv.outside_policy),
        (vec![105, 106, 108], vec![108])
    );
    let listing = exec.calls_containing("unprivileged: 1");
    assert!(
        listing.iter().all(|c| c.contains("100|101|102|103")),
        "no-touch configurations are skipped, not read: {:?}",
        listing
    );
    assert_eq!(probes.host_meta.and_then(|h| h.age_h), Some(30));
    let drill = probes.restore_drill.expect("drill probed");
    assert_eq!(drill.age_h, Some(240));
    assert!(
        drill.failing[0].starts_with("kyu-config"),
        "{:?}",
        drill.failing
    );
    assert_eq!(probes.password_file_ok, Some(true));
    let drive = probes.drive.expect("drive probed");
    assert_eq!((drive.total, drive.trashed), (107374182400, 2147483648));
    assert_eq!(
        probes.exposure.map(|e| e.listen),
        Some("0.0.0.0:8443".into())
    );
    // fix-130 (second half): no stack is recorded in this state, so
    // every file the gateway lists is unowned.
    assert_eq!(
        probes.unowned_route_files,
        Some(vec![
            "104-app-gateway.yml".to_string(),
            "manual-leftover.yml".to_string()
        ])
    );
}

/// fix-131 (expert panel, orchestrator-logs-only-on-pve, 2026-09-27):
/// `homelab incidents` listed names only; reading a bundle took a root
/// shell on pve. `incidents show <name>` brings the error, the versions
/// and the end of the transcript to the workstation, and a name that is
/// not a plain bundle name is refused before it becomes a path.
#[tokio::test]
pub(crate) async fn fix_131_incident_show_reads_one_bundle_and_nothing_else() {
    let dir = std::env::temp_dir().join(format!("homelab-fix131-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("incidents/1800000000-deploy-media");
    std::fs::create_dir_all(&bundle).unwrap();
    std::fs::write(
        bundle.join("report.json"),
        r#"{"op":"deploy-media","steps":[],"ok":false,"error":{"what":"pull images failed","why":"image not found","remedy":"check the tag"}}"#,
    )
    .unwrap();
    std::fs::write(bundle.join("versions.txt"), "host=3.60.0\nproto=1\n").unwrap();
    // Written the way `write_bundle` writes them.
    let line = |m: &str| {
        let ev = PipelineEvent::Line {
            level: homelab_core::sink::Level::Info,
            source: "HOST".into(),
            msg: m.into(),
        };
        format!("{}\n", serde_json::to_string(&ev).unwrap())
    };
    std::fs::write(
        bundle.join("events.jsonl"),
        format!(
            "{}{}",
            line("[run ] docker compose pull"),
            line("image not found")
        ),
    )
    .unwrap();
    let toml = dir.join("host.toml");
    std::fs::write(
        &toml,
        format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        ),
    )
    .unwrap();
    let state = test_state(load_config_from(toml.to_string_lossy().into_owned()));
    let show = |name: &str| RpcRequest {
        id: 3,
        command: Rpc::IncidentShow { name: name.into() },
    };
    let got = handle_rpc(&state, show("1800000000-deploy-media")).await;
    let refused = handle_rpc(&state, show("../../etc")).await;
    let missing = handle_rpc(&state, show("1800000001-deploy-x")).await;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(got.ok, "{}", got.message);
    for want in [
        "pull images failed",
        "image not found",
        "check the tag",
        "host=3.60.0",
        "docker compose pull",
    ] {
        assert!(
            got.message.contains(want),
            "{} missing from:\n{}",
            want,
            got.message
        );
    }
    assert!(
        !refused.ok && refused.message.contains("not a bundle name"),
        "{}",
        refused.message
    );
    assert!(!missing.ok, "{}", missing.message);
}

/// fix-131: the pruner removes what `bundles_to_prune` names, on disk.
#[test]
pub(crate) fn fix_131_old_bundles_are_removed_from_disk() {
    let dir = std::env::temp_dir().join(format!("homelab-fix131p-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let now = 1_800_000_000u64;
    let old = dir.join(format!("incidents/{}-deploy-media", now - 100 * 86_400));
    let young = dir.join(format!("incidents/{}-deploy-kyu", now - 86_400));
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&young).unwrap();
    std::fs::write(old.join("report.json"), "{}").unwrap();
    let removed = prune_incidents(
        &dir.to_string_lossy(),
        now,
        homelab_core::incidents::BUNDLE_MAX_AGE_DAYS,
        homelab_core::incidents::BUNDLE_MAX_COUNT,
    );
    let (old_gone, young_kept) = (!old.exists(), young.exists());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(removed, 1);
    assert!(old_gone && young_kept);
}

/// rule-20: `cleanup_push_staging` removes a `push-staging-*` file it
/// finds on disk (the age judgement itself is
/// `util::stale_push_staging_tests`, pure) and leaves anything else in
/// the state dir alone.
#[test]
pub(crate) fn rule_20_orphaned_push_staging_files_are_removed_from_disk() {
    let dir = std::env::temp_dir().join(format!("homelab-rule20-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let staging = dir.join("push-staging-118-abcd1234");
    let other = dir.join("state.json");
    std::fs::write(&staging, "stale compose content").unwrap();
    std::fs::write(&other, "{}").unwrap();
    // max_age_s is baked into the call via `now`, well past any file's
    // mtime — the threshold itself is covered in core's pure tests.
    let removed = cleanup_push_staging(&dir.to_string_lossy(), u64::MAX / 2);
    let (staging_gone, other_kept) = (!staging.exists(), other.exists());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(removed, 1);
    assert!(staging_gone && other_kept);
}
