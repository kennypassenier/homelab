//! feat-overview-10 (backup calendar): every snapshot time across a stack's
//! per-app repositories, read for the dashboard's own calendar query.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::backup::{BackupCfg, snapshot_nights_unix};

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

fn two_app_stack() -> StackManifest {
    StackManifest {
        home_address_whitelist: None,
        tiles: Default::default(),
        log_files: Vec::new(),
        firewall: None,
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        no_apps_yet: false,
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

const SNAPS_A: &str = r#"[
  {"short_id":"aaa1","time":"2026-09-01T04:00:12.000+02:00"},
  {"short_id":"aaa2","time":"2026-09-02T04:00:12.000+02:00"}
]"#;
const SNAPS_B: &str = r#"[
  {"short_id":"bbb1","time":"2026-09-02T04:05:00.000+02:00"}
]"#;

#[tokio::test]
async fn unions_snapshot_times_across_every_app_repository() {
    let exec = MockExecutor::new();
    exec.respond_always("paperless-config", CmdOutput::ok(SNAPS_A));
    exec.respond_always("actual-config", CmdOutput::ok(SNAPS_B));
    let m = two_app_stack();
    let times = snapshot_nights_unix(&exec, &m, &BackupCfg::default()).await;
    assert_eq!(times.len(), 3, "{times:?}");
}

#[tokio::test]
async fn a_repository_that_cannot_answer_is_left_out_not_a_failure() {
    let exec = MockExecutor::new();
    exec.respond_always("paperless-config", CmdOutput::ok(SNAPS_A));
    exec.respond_always(
        "actual-config",
        CmdOutput::failed(1, "repository does not exist"),
    );
    let m = two_app_stack();
    let times = snapshot_nights_unix(&exec, &m, &BackupCfg::default()).await;
    assert_eq!(times.len(), 2, "{times:?}");
}

#[tokio::test]
async fn malformed_json_from_one_repository_is_left_out() {
    let exec = MockExecutor::new();
    exec.respond_always("paperless-config", CmdOutput::ok("not json"));
    exec.respond_always("actual-config", CmdOutput::ok(SNAPS_B));
    let m = two_app_stack();
    let times = snapshot_nights_unix(&exec, &m, &BackupCfg::default()).await;
    assert_eq!(times.len(), 1, "{times:?}");
}
