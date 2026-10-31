//! The nightly round, the scheduler clock and daemon supervision.

use super::support::*;
use crate::*;

#[test]
pub(crate) fn h12_scheduler_clock_logic() {
    // Weird `date` output never silently disables the scheduler.
    assert_eq!(parse_local_hour("04\n"), Some(4));
    assert_eq!(parse_local_hour("garbage"), None);
    assert_eq!(parse_local_hour("99"), None);
    let now = 1_800_000_000u64;
    assert!(backup_due(4, 4, now - 25 * 3600, now));
    assert!(
        !backup_due(4, 7, now - 25 * 3600, now),
        "outside the night window (fix-129)"
    );
    assert!(!backup_due(4, 4, now - 3600, now), "backed up an hour ago");
    assert!(backup_due(4, 4, 0, now), "never backed up");
}

/// fix-129 (expert panel, restart-skips-night, 2026-09-27): a backup was
/// due only while the local hour equalled `backup_hour`, and the first
/// check came twenty minutes after start. A self-update, crash or power
/// cut inside that hour cost the whole night: no backups, updates,
/// host-meta or fleet check. What was not done in the hour is caught up
/// in the next one, and the first check comes a minute after start.
#[test]
pub(crate) fn fix_129_a_night_missed_by_a_restart_is_caught_up_before_morning() {
    let now = 1_800_000_000u64;
    let stale = now - 25 * 3600;
    assert!(
        backup_due(4, 5, stale, now),
        "04:xx lost, caught up at 05:xx"
    );
    assert!(!backup_due(4, 6, stale, now), "not into the day");
    assert!(!backup_due(4, 3, stale, now), "not before the hour");
    assert!(backup_due(23, 0, stale, now), "the window wraps midnight");
    assert!(
        !backup_due(4, 5, now - 3600, now),
        "done in the hour: not again"
    );
    let st = NightlyState {
        last_integrity_check: 0,
        last_second_copy: 0,
        second_copy_configured: Default::default(),
        last_host_meta: stale,
        last_zfs: now,
        last_restore_drill: now,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
    };
    assert_eq!(
        nightly_plan(4, 5, now, &[("a".into(), true, stale)], &st),
        vec![NightlyTask::Stack("a".into()), NightlyTask::HostMeta]
    );
    assert!(
        Duration::from_secs(SCHEDULER_FIRST_CHECK_S) <= Duration::from_secs(60),
        "the first look comes soon after a start, not a tick later"
    );
}

/// fix-nightly-1 (Kenny, 2026-10-09): a backup by hand in the evening
/// moved `last_backup`, the 20 h rule then skipped the stack that night.
/// The nightly reads its own clock; a manual backup does not move it.
#[test]
pub(crate) fn fix_nightly_1_a_manual_backup_does_not_count_as_tonights_nightly() {
    let now = 1_800_000_000u64;
    let mut hs = homelab_core::state::HostState::default();
    let mut st = homelab_core::state::StackState {
        applied_source: None,
        vmid: 108,
        hostname: "108-app-a".into(),
        apps: vec![],
        applied_at: 0,
        // Backed up by hand three hours ago.
        last_backup: now - 3 * 3600,
        applied_hash: String::new(),
        manifest: None,
        enabled: true,
        natives: Vec::new(),
        incomplete_step: None,
        route_file: None,
        extra_route_files: Vec::new(),
        pushed_file_hashes: std::collections::BTreeMap::new(),
        component_digests: Default::default(),
    };
    hs.stacks.insert("a".into(), st.clone());
    st.enabled = false;
    hs.stacks.insert("parked".into(), st);
    // The last night's own backup was 25 h ago.
    hs.last_nightly_backup.insert("a".into(), now - 25 * 3600);
    let inputs = nightly_stack_inputs(&hs);
    let nst = NightlyState {
        last_integrity_check: now,
        last_second_copy: 0,
        second_copy_configured: Default::default(),
        last_host_meta: now,
        last_zfs: now,
        last_restore_drill: now,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
    };
    assert_eq!(
        nightly_plan(4, 4, now, &inputs, &nst),
        vec![NightlyTask::Stack("a".into())],
        "the evening's manual backup must not cost the stack its night"
    );
    // Once the night recorded its own backup, it is not due again.
    hs.last_nightly_backup.insert("a".into(), now - 600);
    assert!(nightly_plan(4, 4, now, &nightly_stack_inputs(&hs), &nst).is_empty());
}

/// fix-stale-refresh-1: only operations that replace a stack's running
/// images re-read them afterwards.
#[test]
pub(crate) fn fix_stale_refresh_1_only_image_changing_operations_refresh() {
    for l in ["deploy", "update", "restart"] {
        assert!(refreshes_running_images(l), "{l}");
    }
    for l in ["backup", "restore", "resize", "verify-restore", "adopt"] {
        assert!(!refreshes_running_images(l), "{l}");
    }
}

/// G14: the drill rides the backup hour and only when it is due.
#[test]
pub(crate) fn g14_the_restore_drill_is_planned_only_in_the_backup_hour_and_only_when_due() {
    let never = NightlyState {
        last_host_meta: 0,
        last_zfs: 0,
        last_restore_drill: 0,
        last_integrity_check: 0,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
        second_copy_configured: false,
        last_second_copy: 0,
    };
    assert!(
        nightly_plan(4, 4, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
        "never drilled, and it is the hour: it must be planned"
    );
    assert!(
        !nightly_plan(4, 7, 1_000_000, &[], &never).contains(&NightlyTask::RestoreDrill),
        "a restore pulls a whole snapshot back — not outside the backup hour"
    );
    let fresh = NightlyState {
        last_restore_drill: 1_000_000 - 3600,
        ..never
    };
    assert!(
        !nightly_plan(4, 4, 1_000_000, &[], &fresh).contains(&NightlyTask::RestoreDrill),
        "drilled an hour ago: not again tonight"
    );
}

/// fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): the
/// second copy runs once a night when configured, the rotating restic
/// check once a night either way, both in the backup hour.
#[test]
pub(crate) fn fix96_the_second_copy_and_the_check_are_planned_once_a_night() {
    let now = 1_000_000;
    let due = NightlyState {
        last_host_meta: now,
        last_zfs: 0,
        last_restore_drill: now,
        last_integrity_check: 0,
        restore_drill_interval_s: 90 * 24 * 3600,
        zfs_configured: false,
        devices_configured: false,
        second_copy_configured: true,
        last_second_copy: 0,
    };
    let plan = nightly_plan(4, 4, now, &[], &due);
    assert!(plan.contains(&NightlyTask::SecondCopy), "{:?}", plan);
    assert!(plan.contains(&NightlyTask::IntegrityCheck), "{:?}", plan);
    assert!(
        // fix-129: the night is backup_hour plus a catch-up hour, so
        // "outside" starts two hours later.
        nightly_plan(4, 7, now, &[], &due).is_empty(),
        "outside the two-hour night nothing runs"
    );
    let unconfigured = NightlyState {
        second_copy_configured: false,
        ..due
    };
    let plan = nightly_plan(4, 4, now, &[], &unconfigured);
    assert!(!plan.contains(&NightlyTask::SecondCopy));
    assert!(
        plan.contains(&NightlyTask::IntegrityCheck),
        "the Google Drive copy is checked with or without a second one"
    );
    let done = NightlyState {
        last_second_copy: now - 3600,
        last_integrity_check: now - 3600,
        ..due
    };
    assert!(nightly_plan(4, 4, now, &[], &done).is_empty());
}

/// F259 · a watcher pointed at a path no writer writes to (see
/// REGISTER.md).
#[test]
pub(crate) fn f259_a_watcher_that_matches_no_writer_is_named() {
    let dev = homelab_core::ops::devicebackup::DeviceBackup {
        name: "opnsense".into(),
        url: "https://10.10.10.1/api/core/backup/download/this".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense-backup.conf".into(),
        filename: "config.xml".into(),
        pin: Some("sha256//abc".into()),
        ca_file: None,
    };
    let base = "rclone:gdrive:homelab-backups";

    for path in [
        // The repo root, spelled the way restic spells it.
        format!("{}/opnsense-config", base),
        // The same repo without restic's remote prefix, which is how
        // rclone itself must be given it.
        "gdrive:homelab-backups/opnsense-config".to_string(),
        // And the shape that actually works: the snapshots directory,
        // because the repo root holds `config`, written once at init, so
        // watching the root reports the creation date forever.
        "gdrive:homelab-backups/opnsense-config/snapshots".to_string(),
    ] {
        let w = WatchedBackup {
            name: "opnsense-config".into(),
            rclone_path: path.clone(),
            max_age_hours: 26,
        };
        assert!(
            orphan_watchers(&[w], std::slice::from_ref(&dev), base).is_empty(),
            "{} is the writer's own repository and must not be called an orphan — \
             a guard that cries wolf on a correct config teaches its reader to \
             skip the line",
            path
        );
    }

    let stray = WatchedBackup {
        name: "opnsense-config".into(),
        rclone_path: format!("{}/OPNSense-backups", base),
        max_age_hours: 26,
    };
    let out = orphan_watchers(&[stray], std::slice::from_ref(&dev), base);
    assert_eq!(out.len(), 1, "the live misconfiguration must be named");
    assert!(out[0].contains("OPNSense-backups"), "{:?}", out);

    // With nothing configured to write, every watcher is an orphan and
    // saying so is the point — that was the state for weeks.
    assert_eq!(
        orphan_watchers(&[stray_clone()], &[], base).len(),
        1,
        "no writers at all means the watcher watches nothing"
    );
}

#[test]
pub(crate) fn h10_nightly_plan_always_includes_host_meta() {
    // The bug this test was written for: the host-meta backup existed as
    // code but no scheduler path ever reached it, so the vault (holding
    // the ONLY copy of the restic password), state.json and the TLS
    // material were never backed up.
    let now = 1_800_000_000u64;
    let fresh = now - 3600; // backed up an hour ago
    let stale = now - 25 * 3600;

    // No stack is due — the host's own crown jewels still get a snapshot.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), true, fresh)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: 0,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);

    // Due stacks come first, host-meta closes the run.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), true, stale), ("b".into(), true, stale)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: 0,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(
        plan,
        vec![
            NightlyTask::Stack("a".into()),
            NightlyTask::Stack("b".into()),
            NightlyTask::HostMeta
        ]
    );

    // Already snapshotted this run — not repeated on the next 20-min tick.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), true, fresh)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: fresh,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert!(plan.is_empty());

    // Outside the night window (fix-129: 04:00-06:00): nothing at all.
    assert!(
        nightly_plan(
            4,
            7,
            now,
            &[("a".into(), true, stale)],
            &NightlyState {
                last_restore_drill: now,
                last_integrity_check: now,
                restore_drill_interval_s: 90 * 24 * 3600,
                last_host_meta: 0,
                last_zfs: now,
                zfs_configured: false,
                devices_configured: false,
                second_copy_configured: false,
                last_second_copy: 0,
            }
        )
        .is_empty()
    );

    // H8: a parked stack sits out, but the host-meta backup does not
    // depend on any stack being active.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[("a".into(), false, stale)],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: 0,
            last_zfs: now,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);
}

#[test]
pub(crate) fn e8_zfs_only_when_configured() {
    let now = 1_800_000_000u64;
    let stale = now - 25 * 3600;
    // No jobs declared → the feature is simply off.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: stale,
            last_zfs: stale,
            zfs_configured: false,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);
    // Declared → runs once a night, after the host-meta snapshot.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: stale,
            last_zfs: stale,
            zfs_configured: true,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta, NightlyTask::Zfs]);
    // Already ran this cycle → not repeated on the next 20-min tick.
    let plan = nightly_plan(
        4,
        4,
        now,
        &[],
        &NightlyState {
            last_restore_drill: now,
            last_integrity_check: now,
            restore_drill_interval_s: 90 * 24 * 3600,
            last_host_meta: stale,
            last_zfs: now - 3600,
            zfs_configured: true,
            devices_configured: false,
            second_copy_configured: false,
            last_second_copy: 0,
        },
    );
    assert_eq!(plan, vec![NightlyTask::HostMeta]);
}

#[test]
pub(crate) fn h16_capacity_numbers_parse_and_sum() {
    let free = "               total        used        free\nMem:           15908        9911        1268\nSwap:           8191         512        7679\n";
    let mut hs = homelab_core::state::HostState::default();
    let mk = |mem: u32| {
        let mut m = homelab_core::manifest::StackManifest {
            home_address_whitelist: None,
            tiles: Default::default(),
            log_files: Vec::new(),
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: false,
            no_apps_yet: false,
            on_demand: false,
            syslog_receivers: vec![],
            firewall: None,
            natives: Vec::new(),
            stack_name: "x".into(),
            vmid: 108,
            hostname: "108-app-x".into(),
            network: homelab_core::manifest::NetworkSpec {
                ip: "10.10.10.8/24".into(),
                gateway: "g".into(),
                bridge: "b".into(),
                vlan: None,
            },
            resources: homelab_core::manifest::ResourceSpec {
                cores: 2,
                memory_mb: mem,
                swap_mb: 0,
                disk_gb: 4,
                storage: "s".into(),
            },
            lxc: homelab_core::manifest::LxcSpec {
                timezone: "host".into(),
                template: "t".into(),
                unprivileged: true,
                features: String::new(),
                protection: false,
                gpu: false,
                vpn: false,
            },
            boot: homelab_core::manifest::BootSpec {
                onboot: true,
                order: None,
            },
            storage: vec![],
            apps: vec![],
        };
        m.hostname = m.canonical_hostname();
        m
    };
    hs.stacks.insert(
        "a".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 108,
            hostname: "108-app-a".into(),
            apps: vec![],
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(mk(1024)),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    );
    hs.stacks.insert(
        "b".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 109,
            hostname: "109-app-b".into(),
            apps: vec![],
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(mk(4096)),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    );
    let (total, used, committed, cores, load1) =
        capacity_numbers(free, "12\n", "2.53 1.80 1.20 2/500 12345", &hs);
    assert_eq!(total, 15908);
    assert_eq!(used, 9911);
    assert_eq!(committed, 5120, "sum of manifest RAM ceilings");
    assert_eq!(cores, 12);
    assert_eq!(load1, 253);
}
