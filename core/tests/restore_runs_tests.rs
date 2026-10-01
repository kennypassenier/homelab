//! fix-112 (restore-stale-files-mixed-nights, 2026-09-27): a restore wrote
//! over a non-empty directory (files the snapshot does not have stayed
//! behind), could only restore a whole stack, and restored each repository's
//! own `latest` — a newer database beside older media when one repository's
//! backup failed that night.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::OpCtx;
use homelab_core::ops::backup::{BackupCfg, backup, restore_app, restore_with};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const NOW: u64 = 1_760_000_000;
const PAPERLESS: &str = "RESTIC_REPOSITORY=rclone:gdrive:homelab-backups/paperless-config";
const ACTUAL: &str = "RESTIC_REPOSITORY=rclone:gdrive:homelab-backups/actual-config";
const LIST: &str = "RESTIC_PASSWORD_FILE=/var/lib/homelab/secrets/restic.pw \
                    RESTIC_CACHE_DIR=/var/lib/homelab/restic-cache restic snapshots --json";

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        default_log_rotation: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

fn mount(path: &str, app: &str) -> MountSpec {
    MountSpec {
        host_path: path.into(),
        mount_point: path.into(),
        no_data: false,
        no_backup: None,
        host_owner_uid: None,
        app: Some(app.into()),
        postgres_check_image: None,
    }
}

fn paperwork() -> StackManifest {
    StackManifest {
        home_address_whitelist: None,
        tiles: Default::default(),
        log_files: Vec::new(),
        firewall: None,
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        on_demand: false,
        syslog_receivers: vec![],
        natives: Vec::new(),
        stack_name: "paperwork".into(),
        vmid: 114,
        hostname: "114-app-paperwork".into(),
        network: NetworkSpec {
            ip: "10.10.10.14/24".into(),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: ResourceSpec {
            cores: 1,
            memory_mb: 512,
            swap_mb: 256,
            disk_gb: 4,
            storage: "local-lvm".into(),
        },
        lxc: LxcSpec {
            timezone: "host".into(),
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
            protection: true,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![
            mount("/appdata/paperwork/paperless-config", "paperless"),
            mount("/appdata/paperwork/actual-config", "actual"),
        ],
        apps: vec!["paperless".into(), "actual".into()],
    }
}

fn harness() -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 114",
        CmdOutput::ok("hostname: 114-app-paperwork\n"),
    );
    exec.respond_always("test -d", CmdOutput::ok("yes\n"));
    exec.respond_always("du -sbc", CmdOutput::ok("1000\n50000000000\n"));
    exec.respond_always("--status running --services", CmdOutput::ok("paperless\n"));
    // paperless was backed up on both nights; actual's backup failed on the
    // second (newest) night.
    exec.respond_always(
        &format!("{} {}", PAPERLESS, LIST),
        CmdOutput::ok(
            r#"[{"id":"p1full","short_id":"p1111111","time":"2026-09-25T02:00:00Z","tags":["run-100"]},
                {"id":"p2full","short_id":"p2222222","time":"2026-09-26T02:00:00Z","tags":["run-200"]}]"#,
        ),
    );
    exec.respond_always(
        &format!("{} {}", ACTUAL, LIST),
        CmdOutput::ok(
            r#"[{"id":"a1full","short_id":"a1111111","time":"2026-09-25T02:01:00Z","tags":["run-100"]}]"#,
        ),
    );
    // The plain listing a single repository's restore validates against.
    exec.respond_always(
        "restic snapshots",
        CmdOutput::ok("ID        Time\np2222222  2026-09-26 02:00:00\n"),
    );
    exec
}

fn restores(exec: &MockExecutor) -> Vec<String> {
    exec.calls_containing("restic restore")
}

#[tokio::test]
async fn a_night_is_tagged_so_its_repositories_can_be_restored_together() {
    let exec = harness();
    exec.respond_always("restic backup", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = backup(&ctx(&exec, &sink, &j), &paperwork(), &BackupCfg::default()).await;
    assert!(r.ok, "{:?}", r.error);
    let snaps = exec.calls_containing("restic backup");
    assert_eq!(snaps.len(), 2);
    assert!(
        snaps
            .iter()
            .all(|c| c.contains(&format!("--tag run-{}", NOW))),
        "{:?}",
        snaps
    );
}

#[tokio::test]
async fn latest_is_the_newest_night_every_repository_has() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_with(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "latest",
        true,
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let done = restores(&exec);
    assert_eq!(done.len(), 2, "{:?}", done);
    assert!(
        done.iter()
            .any(|c| c.contains(PAPERLESS) && c.contains("restore p1full")),
        "paperless goes back to the night actual also has, not its own newest: {:?}",
        done
    );
    assert!(
        done.iter()
            .any(|c| c.contains(ACTUAL) && c.contains("restore a1full"))
    );
}

#[tokio::test]
async fn an_explicit_id_brings_the_other_repositories_of_its_night_along() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_with(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "a1111111",
        true,
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let done = restores(&exec);
    assert!(
        done.iter().any(|c| c.contains("restore p1full")),
        "{:?}",
        done
    );
    assert!(
        done.iter().any(|c| c.contains("restore a1full")),
        "{:?}",
        done
    );
}

#[tokio::test]
async fn an_id_from_a_night_one_repository_lacks_is_refused_before_anything_stops() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_with(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "p2222222",
        true,
    )
    .await;
    assert!(!r.ok);
    assert!(
        exec.calls_containing("docker compose down").is_empty(),
        "{:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn one_app_is_restored_without_touching_its_neighbours() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_app(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "latest",
        true,
        Some("paperless"),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let done = restores(&exec);
    assert_eq!(done.len(), 1, "{:?}", done);
    assert!(
        done[0].contains(PAPERLESS) && done[0].contains("restore latest"),
        "one repository is its own night: {}",
        done[0]
    );
    let downs = exec.calls_containing("docker compose down");
    assert_eq!(downs.len(), 1, "{:?}", downs);
    assert!(downs[0].contains("/opt/paperwork/paperless"));
}

#[tokio::test]
async fn an_app_the_stack_does_not_have_is_refused() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_app(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "latest",
        true,
        Some("sonarr"),
    )
    .await;
    assert!(!r.ok);
    assert!(restores(&exec).is_empty());
}

#[tokio::test]
async fn with_a_safety_copy_the_target_is_emptied_so_no_stale_file_stays() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_app(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "latest",
        true,
        Some("paperless"),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    let emptied = calls
        .iter()
        .position(|c| c.contains("find '/appdata/paperwork/paperless-config' -mindepth 1 -delete"))
        .expect("the target is emptied");
    let copied = calls
        .iter()
        .position(|c| c.contains("cp -a --parents"))
        .unwrap();
    let restored = calls
        .iter()
        .position(|c| c.contains("restic restore"))
        .unwrap();
    assert!(copied < emptied && emptied < restored, "{:?}", calls);
}

#[tokio::test]
async fn without_a_safety_copy_nothing_is_deleted_first() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_app(
        &ctx(&exec, &sink, &j),
        &paperwork(),
        &BackupCfg::default(),
        "latest",
        false,
        Some("paperless"),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    assert!(exec.calls_containing("-delete").is_empty());
}
