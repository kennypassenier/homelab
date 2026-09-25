//! ask-2 · stack files that belong outside `/opt/<stack>/`.
//!
//! CT 109 carried five files that existed only on the container: two helper
//! scripts on PATH, two units and a timer. A rebuild from the golden template
//! would have dropped them silently. A `rootfs/` directory in the stack now
//! maps onto the container's `/`, restricted to the two places a stack may
//! legitimately add something to.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

#[test]
fn an_ordinary_stack_file_still_lands_under_opt() {
    let (dest, mode) = file_destination("media", "jellyfin/docker-compose.yml").unwrap();
    assert_eq!(dest, "/opt/media/jellyfin/docker-compose.yml");
    assert_eq!(mode, 0o644);
}

#[test]
fn a_rootfs_file_lands_at_its_absolute_path_with_a_mode_that_fits_the_place() {
    let (dest, mode) =
        file_destination("kyu", "rootfs/etc/systemd/system/kyu-backup.timer").unwrap();
    assert_eq!(dest, "/etc/systemd/system/kyu-backup.timer");
    assert_eq!(mode, 0o644, "a unit is read, not run");
    let (dest, mode) = file_destination("kyu", "rootfs/usr/local/bin/kyu-backup").unwrap();
    assert_eq!(dest, "/usr/local/bin/kyu-backup");
    assert_eq!(mode, 0o755, "a script on PATH has to be executable");
}

#[test]
fn a_rootfs_file_anywhere_else_or_climbing_is_refused() {
    for bad in [
        "rootfs/etc/passwd",
        "rootfs/usr/bin/kyu-backup",
        "rootfs/etc/systemd/system/../../passwd",
        "rootfs//usr/local/bin/x",
        "rootfs/",
    ] {
        let why = file_destination("kyu", bad).expect_err(bad);
        assert!(
            why.contains(bad.trim_end_matches('/')) || why.contains("rootfs/"),
            "{}",
            why
        );
    }
}

fn native_kyu_spec() -> DeploySpec {
    DeploySpec {
        native_binaries: Default::default(),
        manifest: StackManifest {
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: true,
            syslog_receivers: vec![],
            natives: vec!["kyu".into()],
            stack_name: "kyu".into(),
            vmid: 109,
            hostname: "109-app-kyu".into(),
            network: NetworkSpec {
                ip: "10.10.10.9/24".into(),
                gateway: "10.10.10.1".into(),
                bridge: "vmbr0".into(),
                vlan: Some(10),
            },
            resources: ResourceSpec {
                cores: 1,
                memory_mb: 256,
                swap_mb: 0,
                disk_gb: 4,
                storage: "local-lvm".into(),
            },
            lxc: LxcSpec {
                template: "clone:998".into(),
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
            storage: vec![],
            apps: vec![],
        },
        files: vec![
            FileBlob {
                path: "kyu/kyu.service".into(),
                content: "[Unit]\nDescription=kyu\n".into(),
                mode: None,
            },
            FileBlob {
                path: "rootfs/etc/systemd/system/kyu-backup.timer".into(),
                content: "[Timer]\nOnCalendar=daily\n".into(),
                mode: None,
            },
            FileBlob {
                path: "rootfs/usr/local/bin/kyu-backup".into(),
                content: "#!/bin/sh\necho backup\n".into(),
                mode: None,
            },
        ],
        env: std::collections::BTreeMap::new(),
        gateway_route: None,
        checks: Default::default(),
    }
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
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

fn native_mocks(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\nprotection: 1\nonboot: 1\nstartup: order=50\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always("systemctl is-active kyu", CmdOutput::ok("active\n"));
}

/// The five files of CT 109, in miniature: a timer and a script travel with
/// the stack, land at their absolute paths, systemd is reloaded, and the
/// timer is enabled — while /opt/kyu/rootfs never exists.
#[tokio::test]
async fn rootfs_files_land_at_their_absolute_paths_and_the_timer_is_enabled() {
    let exec = MockExecutor::new();
    native_mocks(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &native_kyu_spec()).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    assert!(
        !exec
            .calls_containing("/etc/systemd/system/kyu-backup.timer")
            .is_empty(),
        "the timer lands in /etc/systemd/system: {:?}",
        exec.calls()
    );
    let script = exec.calls_containing("/usr/local/bin/kyu-backup");
    assert!(
        !script.is_empty(),
        "the script lands on PATH: {:?}",
        exec.calls()
    );
    assert!(
        script.iter().any(|c| c.contains("755")),
        "a script on PATH is pushed executable: {:?}",
        script
    );
    assert!(
        exec.calls_containing("/opt/kyu/rootfs").is_empty(),
        "rootfs is a mapping, never a directory under /opt: {:?}",
        exec.calls_containing("/opt/kyu/rootfs")
    );
    assert!(
        !exec
            .calls_containing("systemctl enable --now kyu-backup.timer")
            .is_empty(),
        "a timer on disk fires nothing until it is enabled: {:?}",
        exec.calls()
    );
    // "rootfs" is not an app: nothing may try to restart it as a service.
    assert!(
        exec.calls_containing("restart rootfs").is_empty()
            && exec
                .calls_containing("systemctl restart kyu-backup")
                .is_empty(),
        "{:?}",
        exec.calls()
    );
}

/// A rootfs path outside the two allowed places is refused BEFORE anything
/// is pushed or committed — the validate step, not the push step.
#[tokio::test]
async fn a_rootfs_file_outside_the_allowed_places_stops_the_deploy_before_any_push() {
    let exec = MockExecutor::new();
    native_mocks(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = native_kyu_spec();
    sp.files.push(FileBlob {
        path: "rootfs/etc/passwd".into(),
        content: "root:x:0:0\n".into(),
        mode: None,
    });
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(
        !report.ok,
        "a file that would overwrite /etc/passwd must be refused"
    );
    let why = format!("{:?}", report.error);
    assert!(why.contains("rootfs/etc/passwd"), "{}", why);
    assert!(
        exec.calls_containing("pct push").is_empty(),
        "nothing may be pushed once validation refused: {:?}",
        exec.calls()
    );
}
