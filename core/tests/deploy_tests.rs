//! The M0 safety and idempotency suite — every scenario here maps to a
//! FEATURES.md test scenario (A1, A2, A3/D10, B1, D1, A5).

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

fn manifest(vmid: u16, stack: &str) -> StackManifest {
    StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: stack.into(),
        vmid,
        hostname: format!("{}-app-{}", vmid, stack),
        network: NetworkSpec {
            ip: format!("10.10.10.{}/24", vmid - 100),
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
            template: "local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst".into(),
            unprivileged: true,
            features: "nesting=1,keyctl=1".into(),
            protection: false,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![MountSpec {
            host_path: "/appdata/syncthing/syncthing-config".into(),
            mount_point: "/appdata/syncthing/syncthing-config".into(),
            host_owner_uid: Some(101000),
            app: Some("syncthing".into()),
        }],
        apps: vec!["syncthing".into()],
    }
}

fn spec(vmid: u16, stack: &str) -> DeploySpec {
    DeploySpec {
        manifest: manifest(vmid, stack),
        files: vec![FileBlob {
            path: "syncthing/docker-compose.yml".into(),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: std::collections::BTreeMap::new(),
        gateway_route: Some(GatewayRoute {
            gateway_vmid: 104,
            filename: "110-app-syncthing.yml".into(),
            content: "http: {}\n".into(),
        }),
    }
}

fn sha_hex(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(content.as_bytes());
    let out = h
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    format!("{}\n", out)
}

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        kea: None,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        backup: Default::default(),
        registry_cache: None,
    }
}

/// Script the mock as "fresh host, container does not exist yet".
fn script_fresh(exec: &MockExecutor) {
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
}

/// O7: a config path must be named `<app>-config`. Kenny chose the literal
/// rule at the mini-round — "uniformiteit en een duidelijke regel hebben hier
/// voorrang" — over the weaker shape that would have tolerated the live
/// `prometheus-data`. The restore looks paths up by name, so a directory that
/// is renamed loses track of its own snapshots; a rule nothing enforces is
/// how that drifted into three different shapes in the first place.
#[test]
fn o7_config_paths_must_be_named_after_their_app() {
    use homelab_core::manifest::validate;
    let mut s = spec(108, "test");
    s.manifest.apps = vec!["syncthing".into()];
    s.manifest.storage[0].host_path = "/appdata/test/syncthing-data".into();
    s.manifest.storage[0].mount_point = "/appdata/test/syncthing-data".into();
    s.manifest.storage[0].app = Some("syncthing".into());
    let err = validate(&s).expect_err("a path not ending in -config must be refused");
    assert!(
        format!("{}", err).contains("syncthing-config"),
        "the error must name what it should have been: {}",
        err
    );

    // The owner and the directory have to agree, or the name says one thing
    // and the backup does another.
    let mut s = spec(108, "test");
    s.manifest.apps = vec!["syncthing".into(), "other".into()];
    s.manifest.storage[0].host_path = "/appdata/test/other-config".into();
    s.manifest.storage[0].mount_point = "/appdata/test/other-config".into();
    s.manifest.storage[0].app = Some("syncthing".into());
    let err = validate(&s).expect_err("owner and directory name must agree");
    assert!(format!("{}", err).contains("syncthing"), "{}", err);

    // The shape that is right stays right.
    validate(&spec(108, "test")).expect("syncthing-config owned by syncthing is valid");
}

/// O5: on an unprivileged container uid 1000 inside is uid 101000 on the
/// host, so the two privilege levels need host_owner_uid values 100000 apart.
/// Nothing checked, and the wrong number produces a directory the service
/// cannot use while the deploy reports success — the app just does not start.
#[test]
fn o5_host_owner_uid_must_match_the_privilege_level() {
    use homelab_core::manifest::validate;
    // Unprivileged stack given a raw uid: that is the privileged number.
    let mut s = spec(108, "test");
    s.manifest.lxc.unprivileged = true;
    s.manifest.storage[0].host_owner_uid = Some(1000);
    let err = validate(&s).expect_err("raw uid on an unprivileged stack must be refused");
    assert!(
        format!("{}", err).contains("101000"),
        "the error must name the number that would work: {}",
        err
    );

    // Privileged stack given a mapped uid: the other way round.
    let mut s = spec(108, "test");
    s.manifest.lxc.unprivileged = false;
    s.manifest.storage[0].host_owner_uid = Some(101000);
    let err = validate(&s).expect_err("mapped uid on a privileged stack must be refused");
    assert!(format!("{}", err).contains("1000"), "{}", err);

    // The combinations that are right stay right.
    let mut ok = spec(108, "test");
    ok.manifest.lxc.unprivileged = true;
    ok.manifest.storage[0].host_owner_uid = Some(101000);
    validate(&ok).expect("mapped uid on an unprivileged stack is correct");
    let mut ok = spec(108, "test");
    ok.manifest.lxc.unprivileged = false;
    ok.manifest.storage[0].host_owner_uid = Some(1000);
    validate(&ok).expect("raw uid on a privileged stack is correct");
}

// ── A1: no-touch list refuses before ANY command runs ───────────────────────

#[tokio::test]
async fn a1_no_touch_vmid_is_refused_with_zero_commands() {
    for vmid in [100u16, 101, 102, 103] {
        let exec = MockExecutor::new();
        let sink = VecSink::new();
        let journal = NullJournal;
        // Craft a "valid-looking" spec pointing at a protected guest.
        let stack = "evil";
        let report = deploy(&ctx(&exec, &sink, &journal), &spec(vmid, stack)).await;
        assert!(!report.ok, "vmid {} must be refused", vmid);
        let err = report.error.unwrap();
        assert!(
            err.why.contains("no-touch"),
            "unexpected error: {}",
            err.why
        );
        assert!(
            exec.calls().is_empty(),
            "vmid {}: commands ran despite refusal: {:?}",
            vmid,
            exec.calls()
        );
    }
}

// ── A2: hostname guard refuses a mismatched existing container ──────────────

#[tokio::test]
async fn a2_hostname_mismatch_refuses_reuse() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: something-else\ncores: 1\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("refusing"));
    // Probes are allowed; mutations are not.
    for forbidden in [
        "pct create",
        "pct push",
        "pct set",
        "pct start",
        "compose up",
    ] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "mutating call {} happened after refusal",
            forbidden
        );
    }
}

// ── A3/D10: invalid input aborts with zero commands ─────────────────────────

#[tokio::test]
async fn d10_path_traversal_is_refused_before_any_command() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing");
    s.files.push(FileBlob {
        path: "../escape/evil.yml".into(),
        content: "x".into(),
        mode: None,
    });
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(!report.ok);
    assert!(report.error.unwrap().why.contains("escapes the stack root"));
    assert!(exec.calls().is_empty());
}

#[tokio::test]
async fn d10_validator_collects_all_problems() {
    let mut s = spec(110, "syncthing");
    s.manifest.stack_name = "Bad Name".into();
    s.manifest.resources.memory_mb = 64;
    let err = homelab_core::manifest::validate(&s).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("stack_name"), "{}", msg);
    assert!(msg.contains("memory_mb"), "{}", msg);
    assert!(msg.contains("hostname"), "{}", msg); // canonical name changed too
}

/// A bind INSIDE a declared mount is covered by it. The check exists so data
/// cannot land on the container's own disk; a subdirectory of a host mount
/// lands on the host exactly as intended. Requiring an exact match refused
/// the pull-through cache, which keeps one directory per upstream registry
/// under a single mount — and O7's naming rule would have forced four apps
/// into existence to express that.
#[test]
fn a_bind_inside_a_declared_mount_is_covered_by_it() {
    use homelab_core::manifest::{validate, FileBlob};
    let mut s = spec(110, "syncthing");
    s.files.push(FileBlob {
        path: "syncthing/docker-compose.yml".into(),
        content:
            "services:\n  a:\n    volumes:\n      - /appdata/syncthing/syncthing-config/sub:/data\n"
                .into(),
        mode: None,
    });
    validate(&s).expect("a subdirectory of a declared mount is fine");

    // A sibling that merely shares a prefix is NOT covered — that would land
    // on the rootfs, which is the whole point of the check.
    let mut s = spec(110, "syncthing");
    s.files.push(FileBlob {
        path: "syncthing/docker-compose.yml".into(),
        content: "services:\n  a:\n    volumes:\n      - /appdata/syncthing/syncthing-config-other:/data\n"
            .into(),
        mode: None,
    });
    let err = validate(&s).expect_err("a sibling path must still be refused");
    assert!(
        format!("{}", err).contains("not declared under storage"),
        "{}",
        err
    );
}

// ── Native units get the same cycle a docker app gets ──────────────────────

/// Kenny's N1 (2026-08-31): the unit file lives in the repository and a
/// deploy puts it there. Before this, the four files that make CT 109's
/// services exist were only inside CT 109 — losing the container would have
/// lost the only copy.
#[tokio::test]
async fn a_deploy_installs_the_unit_file_and_starts_a_service_that_is_down() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\nprotection: 1\nonboot: 1\nstartup: order=50\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    // The unit is down before, up after.
    exec.enqueue("systemctl is-active kyu", CmdOutput::ok("inactive\n"));
    exec.respond_always("systemctl is-active kyu", CmdOutput::ok("active\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(109, "kyu");
    sp.manifest.hostname = "109-app-kyu".into();
    sp.manifest.apps = vec![];
    sp.manifest.storage = vec![];
    sp.manifest.native_only = true;
    sp.manifest.natives = vec!["kyu".into()];
    sp.gateway_route = None;
    sp.files = vec![homelab_core::manifest::FileBlob {
        path: "kyu/kyu.service".into(),
        content: "[Unit]\nDescription=kyu\n".into(),
        mode: None,
    }];
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    assert!(
        !exec
            .calls_containing("/etc/systemd/system/kyu.service")
            .is_empty(),
        "the unit file is written"
    );
    // And ONLY there. Pushing it to /opt/<stack>/<unit>/ as well cost
    // almanac its binary: that path was the binary, the garbage collector
    // removed it, and the push made a directory of the same name. The
    // service survived only because the kernel holds a deleted file open.
    assert!(
        exec.calls_containing("/opt/kyu/kyu/kyu.service").is_empty(),
        "a unit file must never be pushed into /opt as well: {:?}",
        exec.calls_containing("/opt/kyu/kyu")
    );
    assert!(!exec.calls_containing("systemctl daemon-reload").is_empty());
    assert!(!exec
        .calls_containing("systemctl enable --now kyu")
        .is_empty());
}

/// A service that is already running is left running. Adoption's rule holds:
/// a deploy does not restart a production service to take ownership of it.
#[tokio::test]
async fn a_running_native_service_is_never_restarted_by_a_deploy() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\nprotection: 1\nonboot: 1\nstartup: order=50\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always("systemctl is-active kyu", CmdOutput::ok("active\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(109, "kyu");
    sp.manifest.hostname = "109-app-kyu".into();
    sp.manifest.apps = vec![];
    sp.manifest.storage = vec![];
    sp.manifest.native_only = true;
    sp.manifest.natives = vec!["kyu".into()];
    sp.gateway_route = None;
    sp.files = vec![homelab_core::manifest::FileBlob {
        path: "kyu/kyu.service".into(),
        content: "[Unit]\nDescription=kyu\n".into(),
        mode: None,
    }];
    assert!(deploy(&ctx(&exec, &sink, &journal), &sp).await.ok);
    // The unit itself, not the container's own housekeeping: the runaway
    // guards restart journald, which is not this service.
    for forbidden in [
        "systemctl restart kyu",
        "systemctl stop kyu",
        "systemctl enable --now kyu",
    ] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "a running service must not see '{}'",
            forbidden
        );
    }
}

/// A declared unit with no unit file is refused, loudly. The whole point of
/// bringing the file into the repository is that a rebuild does not depend on
/// somebody remembering what was in it.
#[tokio::test]
async fn a_declared_unit_without_its_file_is_refused() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\nprotection: 1\nonboot: 1\nstartup: order=50\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(109, "kyu");
    sp.manifest.hostname = "109-app-kyu".into();
    sp.manifest.apps = vec![];
    sp.manifest.storage = vec![];
    sp.manifest.native_only = true;
    sp.manifest.natives = vec!["kyu".into()];
    sp.gateway_route = None;
    sp.files = vec![];
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok);
    let why = report.error.unwrap().why;
    assert!(
        why.contains("kyu/kyu.service"),
        "name the missing file: {}",
        why
    );
}

// ── A container that runs no docker at all ─────────────────────────────────

/// Kenny asked the right question on 2026-08-31: the four native services
/// could be backed up and updated, but nothing in the repository said how to
/// rebuild the container they run on. A stack that can only be repaired by
/// the person who remembers how it was built is not managed.
#[test]
fn native_only_lets_a_container_declare_that_it_runs_no_docker() {
    use homelab_core::manifest::validate;
    let mut s = spec(109, "kyu");
    s.manifest.hostname = "109-app-kyu".into();
    s.manifest.apps = vec![];
    s.manifest.storage = vec![];
    s.files = vec![];
    s.manifest.native_only = true;
    s.manifest.natives = vec!["kyu".into()];
    validate(&s).expect("a native-only container may declare no apps");

    // An empty list WITHOUT the flag is still refused: that is what a docker
    // stack looks like when somebody forgot to fill it in.
    let mut s2 = s.clone();
    s2.manifest.native_only = false;
    let err = validate(&s2).expect_err("an empty app list must not pass by accident");
    assert!(
        format!("{}", err).contains("native_only"),
        "the error must name the way to say it on purpose: {}",
        err
    );

    // And the two must agree.
    let mut s3 = s.clone();
    s3.manifest.apps = vec!["promtail".into()];
    let err = validate(&s3).expect_err("native_only with apps is a contradiction");
    assert!(
        format!("{}", err).contains("one of the two is wrong"),
        "{}",
        err
    );
}

/// Such a container must never be given docker: installing it would change
/// the very thing the manifest exists to reproduce.
#[tokio::test]
async fn a_native_only_container_is_never_given_docker() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\nprotection: 1\nonboot: 1\nstartup: order=50\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(109, "kyu");
    sp.manifest.hostname = "109-app-kyu".into();
    sp.manifest.apps = vec![];
    sp.manifest.storage = vec![];
    sp.files = vec![];
    sp.manifest.native_only = true;
    sp.manifest.natives = vec!["kyu".into()];
    sp.files = vec![homelab_core::manifest::FileBlob {
        path: "kyu/kyu.service".into(),
        content: "[Unit]\nDescription=kyu\n".into(),
        mode: None,
    }];
    sp.gateway_route = None;
    exec.respond_always("systemctl is-active kyu", CmdOutput::ok("active\n"));
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    for forbidden in [
        "get.docker.com",
        "docker --version",
        "docker compose",
        "docker network create",
        "rm -rf",
    ] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "a native-only container must never see '{}': {:?}",
            forbidden,
            exec.calls_containing(forbidden)
        );
    }
    // And no weekly prune timer for a docker that is not there. CT 109 and
    // CT 112 have been failing that unit every week.
    assert!(exec.calls_containing("docker-prune").is_empty());
    assert!(exec.calls_containing("/etc/docker/daemon.json").is_empty());
}

// ── M1: directories this stack borrows rather than owns ────────────────────

/// The media libraries are two ZFS datasets that Proxmox hands to the
/// fileserver and to the media container at the same time. They are not
/// config, they are not backed up, and they are terabytes large — so they
/// are declared apart from the directories the orchestrator owns, and the
/// strict rules that make `storage:` worth having stay strict.
#[tokio::test]
async fn m1_a_borrowed_directory_is_mounted_but_never_created() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always("if [ -d ", CmdOutput::ok("/HDD18TB/subvol-103-disk-0 OK\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.data_mounts = vec![homelab_core::manifest::DataMount {
        host_path: "/HDD18TB/subvol-103-disk-0".into(),
        mount_point: "/mnt/data/18TB".into(),
        note: Some("the fileserver's dataset".into()),
    }];
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    // Mounted, with its number continuing after the storage entries.
    let mp = exec.calls_containing("/HDD18TB/subvol-103-disk-0,mp=/mnt/data/18TB");
    assert_eq!(mp.len(), 1, "{:?}", mp);
    assert!(
        mp[0].contains("-mp1"),
        "storage took mp0, this is mp1: {}",
        mp[0]
    );

    // Never created, never chowned — those are the owner's business.
    assert!(
        exec.calls_containing("mkdir -p /HDD18TB").is_empty(),
        "a borrowed directory is not ours to create"
    );
    assert!(
        exec.calls_containing("chown")
            .iter()
            .all(|c| !c.contains("/HDD18TB")),
        "nor ours to chown"
    );
}

/// A missing one is refused. `pct set` would happily make an empty directory,
/// the container would start, and Jellyfin would come up with no films while
/// every *arr root folder reported itself missing — a rebuild that silently
/// loses the libraries.
#[tokio::test]
async fn m1_a_missing_borrowed_directory_stops_the_deploy() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always(
        "if [ -d ",
        CmdOutput::ok("/HDD18TB/subvol-103-disk-0 MISSING\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.data_mounts = vec![homelab_core::manifest::DataMount {
        host_path: "/HDD18TB/subvol-103-disk-0".into(),
        mount_point: "/mnt/data/18TB".into(),
        note: None,
    }];
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok, "a missing library path must stop the deploy");
    let why = report.error.unwrap().why;
    assert!(why.contains("/HDD18TB/subvol-103-disk-0"), "{}", why);
    assert!(
        why.contains("libraries are empty"),
        "say what would happen: {}",
        why
    );
    assert!(exec.calls_containing("pct create").is_empty());
    assert!(exec.calls_containing("pct clone").is_empty());
}

/// The two lists stay distinct, in both directions. Blurring them is how the
/// strict rule on `storage:` quietly stops meaning anything.
#[test]
fn m1_the_two_kinds_of_directory_cannot_be_confused() {
    use homelab_core::manifest::{validate, DataMount};
    // A borrowed directory under /appdata is a config directory in disguise.
    let mut s = spec(110, "syncthing");
    s.manifest.data_mounts = vec![DataMount {
        host_path: "/appdata/syncthing/other-config".into(),
        mount_point: "/mnt/other".into(),
        note: None,
    }];
    let err = validate(&s).expect_err("must be refused");
    assert!(
        format!("{}", err).contains("declare it under storage:"),
        "the error must say where it belongs: {}",
        err
    );

    // Two lists claiming the same place inside the container.
    let mut s = spec(110, "syncthing");
    s.manifest.data_mounts = vec![DataMount {
        host_path: "/HDD18TB/media".into(),
        mount_point: "/appdata/syncthing/syncthing-config".into(),
        note: None,
    }];
    let err = validate(&s).expect_err("a mount point cannot be claimed twice");
    assert!(format!("{}", err).contains("claimed by both"), "{}", err);

    // And a relative path is refused outright.
    let mut s = spec(110, "syncthing");
    s.manifest.data_mounts = vec![DataMount {
        host_path: "HDD18TB/media".into(),
        mount_point: "/mnt/media".into(),
        note: None,
    }];
    assert!(validate(&s).is_err());

    // The real shape stays valid.
    let mut ok = spec(110, "syncthing");
    ok.manifest.data_mounts = vec![DataMount {
        host_path: "/HDD18TB/subvol-103-disk-0".into(),
        mount_point: "/mnt/data/18TB".into(),
        note: Some("CT 103's dataset, mounted twice on purpose".into()),
    }];
    validate(&ok).expect("a borrowed media directory is valid");
}

/// A stack with no borrowed directories must not be asked about any.
#[tokio::test]
async fn m1_a_stack_without_borrowed_directories_is_never_probed() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);
    assert!(exec.calls_containing("if [ -d ").is_empty());
}

// ── Mount drift on a container that already exists ─────────────────────────

/// A mount that is missing from a live container is put back by a deploy.
///
/// Found the hard way: the downloader was provisioned without its two data
/// disks because the host silently dropped a field it did not know (F118).
/// The mounts were re-attached by hand, and a redeploy would have reported
/// success while leaving that hand-made fix as the only thing holding them
/// there. A repair that lives only outside the repo is not a repair.
#[tokio::test]
async fn a_missing_mount_is_reattached_on_an_existing_container() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    // Exists, protected, and carrying only its config mount.
    exec.respond_always(
        "pct config",
        CmdOutput::ok(
            "hostname: 110-app-syncthing\nprotection: 1\n\
             mp0: /appdata/syncthing/syncthing-config,mp=/appdata/syncthing/syncthing-config\n\
             onboot: 1\nstartup: order=50\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always("if [ -d ", CmdOutput::ok("/HDD18TB/media OK\n"));
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.data_mounts = vec![homelab_core::manifest::DataMount {
        host_path: "/HDD18TB/media".into(),
        mount_point: "/mnt/data/18TB".into(),
        note: None,
    }];
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n));
    let add = pos("-mp1 /HDD18TB/media,mp=/mnt/data/18TB").expect("the missing mount is attached");
    let off = pos("--protection 0").expect("protection must come off first");
    let on = calls
        .iter()
        .rposition(|c| c.contains("--protection 1"))
        .expect("and go straight back on");
    assert!(
        off < add && add < on,
        "off, attach, on: {} {} {}",
        off,
        add,
        on
    );

    // The mount it already has is left alone — no pointless writes.
    assert_eq!(
        exec.calls_containing("-mp0 ").len(),
        0,
        "an mp that already matches must not be rewritten"
    );
}

/// The protection flag is intent like any other, and it was only ever set on
/// the run that created the container. Found one minute after shipping the
/// mount reconciliation: that step lifts the flag to do its work, and the
/// deploy that followed left the downloader unprotected without a word.
#[tokio::test]
async fn protection_is_put_back_when_the_stack_file_asks_for_it() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(
            "hostname: 110-app-syncthing\nprotection: 0\n\
             mp0: /appdata/syncthing/syncthing-config,mp=/appdata/syncthing/syncthing-config\n\
             onboot: 1\nstartup: order=50\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.lxc.protection = true;
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);
    assert_eq!(
        exec.calls_containing("--protection 1").len(),
        1,
        "turned back on exactly once"
    );

    // And a stack that does not ask for protection never touches the flag.
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(
            "hostname: 110-app-syncthing\nprotection: 0\n\
             mp0: /appdata/syncthing/syncthing-config,mp=/appdata/syncthing/syncthing-config\n\
             onboot: 1\nstartup: order=50\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.lxc.protection = false;
    assert!(deploy(&ctx(&exec, &sink, &journal), &sp).await.ok);
    assert!(exec.calls_containing("--protection").is_empty());
}

/// A container whose mounts already match is not written to at all, and its
/// protection flag is never touched.
#[tokio::test]
async fn matching_mounts_are_left_completely_alone() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(
            "hostname: 110-app-syncthing\nprotection: 1\n\
             mp0: /appdata/syncthing/syncthing-config,mp=/appdata/syncthing/syncthing-config\n\
             onboot: 1\nstartup: order=50\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);
    assert!(
        exec.calls_containing("--protection").is_empty(),
        "nothing to change means the flag is never touched"
    );
    assert!(exec.calls_containing("-mp").is_empty());
}

// ── W3: a container that already exists is put back in line ────────────────

/// The W3 acceptance criterion's second half: a boot order moved by hand is
/// corrected by a deploy. Set at creation and never checked again means the
/// fleet boots in whatever order somebody typed years ago.
#[tokio::test]
async fn w3_a_deploy_puts_a_drifted_boot_policy_back() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    // The container exists, and disagrees with the stack file on both counts.
    exec.respond_always(
        "pct config",
        CmdOutput::ok(
            "arch: amd64\nhostname: 110-app-syncthing\nonboot: 0\nstartup: order=1\nmemory: 512\ncores: 1\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let set = exec.calls_containing("--onboot");
    assert_eq!(set.len(), 1, "one correction: {:?}", set);
    assert!(set[0].contains("--onboot 1"), "{}", set[0]);
    assert!(set[0].contains("--startup order=50"), "{}", set[0]);
    // And it stays out of the resources: raising them is `homelab resize`,
    // lowering them is a rebuild, and neither belongs in a deploy.
    assert!(!set[0].contains("--memory"), "{}", set[0]);
    assert!(!set[0].contains("--cores"), "{}", set[0]);
}

/// A container that already agrees is left alone — the check must not
/// produce a write on every deploy.
#[tokio::test]
async fn w3_a_container_that_agrees_is_not_written_to() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, "does not exist"));
    exec.respond_always(
        "pct config",
        CmdOutput::ok(
            "arch: amd64\nhostname: 110-app-syncthing\nonboot: 1\nstartup: order=50\nmemory: 512\ncores: 1\n",
        ),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    exec.respond_always("git -C /var/lib/homelab/repo commit", CmdOutput::ok(""));
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);
    assert!(
        exec.calls_containing("--onboot").is_empty(),
        "agreement means no write"
    );
}

// ── W1: the host has to actually have what the stack asks for ──────────────

/// F54, the most self-concealing failure left in the fleet: a stack with
/// `gpu: true` on a host with no card comes up perfectly and transcodes on
/// the CPU. Nothing looks wrong until a film stutters in the evening.
#[tokio::test]
async fn w1_a_gpu_stack_is_refused_when_the_host_has_no_card() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always(
        "stat -c %g",
        CmdOutput::ok("/dev/dri/card0 MISSING\n/dev/dri/renderD128 MISSING\ndri:\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.lxc.gpu = true;
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;

    assert!(
        !report.ok,
        "a GPU stack on a host without one must be refused"
    );
    let why = report.error.unwrap().why;
    assert!(why.contains("/dev/dri/card0"), "name the device: {}", why);
    assert!(why.contains("syncthing"), "name the stack: {}", why);
    assert!(
        why.contains("transcodes on the CPU"),
        "say what would happen instead of failing: {}",
        why
    );
    // And it refuses before it builds anything.
    assert!(exec.calls_containing("pct create").is_empty());
    assert!(exec.calls_containing("pct clone").is_empty());
    assert!(exec.calls_containing("mkdir -p /appdata").is_empty());
}

/// The other half of F54: the group ids were the literals 44 and 104, right
/// on this host and silently wrong on any other. A gid that does not match
/// hands over a device node the container cannot open — which looks exactly
/// like the device not being there.
#[tokio::test]
async fn w1_the_device_group_ids_are_read_from_the_host() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always(
        "stat -c %g",
        CmdOutput::ok("/dev/dri/card0 993\n/dev/dri/renderD128 994\ndri: card0 renderD128 \n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.lxc.gpu = true;
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let dev = exec.calls_containing("--dev0");
    assert_eq!(dev.len(), 1, "exactly one passthrough call: {:?}", dev);
    assert!(dev[0].contains("/dev/dri/card0,gid=993"), "{}", dev[0]);
    assert!(dev[0].contains("/dev/dri/renderD128,gid=994"), "{}", dev[0]);
    assert!(
        !dev[0].contains("gid=44") && !dev[0].contains("gid=104"),
        "the hardcoded numbers must be gone: {}",
        dev[0]
    );
}

/// The same shape for the VPN flag: without /dev/net/tun the container
/// starts and only the tunnel inside it fails, where nothing is looking.
#[tokio::test]
async fn w1_a_vpn_stack_is_refused_when_the_host_has_no_tun() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always("stat -c %g", CmdOutput::ok("/dev/net/tun MISSING\ndri:\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.lxc.vpn = true;
    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok);
    let why = report.error.unwrap().why;
    assert!(why.contains("/dev/net/tun"), "{}", why);
}

/// A stack that asks for no hardware must not be probed at all — a check
/// that runs where it has no business is a check that can refuse a deploy
/// for a reason that does not apply to it.
#[tokio::test]
async fn w1_a_stack_without_hardware_is_never_probed() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);
    assert!(
        exec.calls_containing("stat -c %g").is_empty(),
        "no hardware asked for, no hardware probed"
    );
}

/// Read back what the deploy recorded for the stack under test.
async fn recorded_backup_time(exec: &MockExecutor) -> u64 {
    homelab_core::state::StateStore::new(exec, "/var/lib/homelab")
        .load()
        .await
        .expect("state written")
        .stacks
        .get("syncthing")
        .expect("stack recorded")
        .last_backup
}

// ── M7: what a container replacement must not lose ─────────────────────────

/// A C4 replacement destroys the container and its state record together, so
/// "preserve the previous value" preserves nothing. In the M7 drill CT 115
/// was backed up twelve minutes before it was replaced and came back saying
/// it had never been backed up; the fleet check then called it broken while
/// the snapshot sat in the repository, untouched and perfectly restorable.
///
/// The state file is a cache of what the repository knows. When the cache is
/// gone, rebuild it from the source instead of inventing a zero.
#[tokio::test]
async fn m7_a_replaced_stack_recovers_its_backup_time_from_the_repository() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    // The config directory survived the destroy (that is the whole point of
    // /appdata), so E3 has nothing to restore and only the state recovery
    // reaches restic.
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    exec.respond_always(
        "restic snapshots",
        CmdOutput::ok(r#"[{"short_id":"b768d68b","time":"2026-08-31T18:17:33.981879581+02:00"}]"#),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    assert_eq!(
        recorded_backup_time(&exec).await,
        1_788_193_053,
        "the snapshot time must survive the replacement"
    );
}

/// The native services registered on a stack are not the deploy's to forget.
///
/// Giving CT 109 and CT 112 a container manifest wrote an empty list over
/// their registrations, so the nightly backup of kyu, kyu-runner,
/// http-switchboard and almanac would simply have stopped — quietly, with
/// nothing to see. Found by reading `homelab status` after the deploy rather
/// than the deploy's own output.
#[tokio::test]
async fn a_deploy_never_unregisters_the_native_services_on_its_stack() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"syncthing":{"vmid":110,
           "hostname":"110-app-syncthing","apps":["syncthing"],"applied_at":1,
           "last_backup":4242,"applied_hash":"","manifest":null,
           "enabled":true,"native":null,"natives":[
             {"stack_name":"syncthing","vmid":110,"hostname":"110-app-syncthing",
              "unit":"kyu","binary":"/usr/local/bin/kyu","env_file":null,
              "data_dirs":["/var/lib/kyu"],"update_cmd":null,"stateless":false}
           ]}}}"#,
    );
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let state = homelab_core::state::StateStore::new(&exec, "/var/lib/homelab")
        .load()
        .await
        .expect("state written");
    let st = state.stacks.get("syncthing").expect("stack recorded");
    assert_eq!(st.natives.len(), 1, "the registration survives a deploy");
    assert_eq!(st.natives[0].unit, "kyu");
}

/// The other direction, which is what makes the test above mean anything: a
/// stack whose repository has nothing to say still records a zero, and the
/// deploy still succeeds. A backup target that is unreachable must never be
/// able to block a deploy.
#[tokio::test]
async fn m7_a_stack_with_no_snapshots_records_zero_and_still_deploys() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    exec.respond_always(
        "restic snapshots",
        CmdOutput::failed(1, "repository does not exist"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(
        report.ok,
        "an unreachable repository must not block a deploy"
    );
    assert_eq!(recorded_backup_time(&exec).await, 0);
}

/// A redeploy over a container that still exists keeps the value it already
/// had, and must not spend a restic round trip finding that out.
#[tokio::test]
async fn m7_a_plain_redeploy_keeps_its_record_without_asking_restic() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"syncthing":{"vmid":110,
           "hostname":"110-app-syncthing","apps":["syncthing"],"applied_at":1,
           "last_backup":4242,"applied_hash":"","manifest":null,
           "enabled":true,"native":null,"natives":[]}}}"#,
    );
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok("config.xml\n"),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);
    assert_eq!(recorded_backup_time(&exec).await, 4242);
    assert!(
        exec.calls_containing("restic snapshots").is_empty(),
        "an existing record answers the question on its own"
    );
}

/// The repository that deploy reads must be the one the host is configured
/// with. Both of deploy's restic callers built their own
/// `BackupCfg::default()`, so a changed `restic_base` in settings.toml would
/// have pointed E3's auto-restore at a repository that does not exist — and
/// its answer to that is "no snapshot — fresh", after which the deploy
/// carries on and starts the app on an empty config directory.
#[tokio::test]
async fn e3_auto_restore_reads_the_configured_repository() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    // Empty config directory: E3 goes looking for a snapshot.
    exec.respond_always(
        "ls -A '/appdata/syncthing/syncthing-config'",
        CmdOutput::ok(""),
    );
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut c = ctx(&exec, &sink, &journal);
    c.backup = homelab_core::ops::backup::BackupCfg {
        restic_base: "rclone:hdd:homelab-backups".into(),
        password_file: "/etc/homelab/restic.pw".into(),
        ..Default::default()
    };
    let report = deploy(&c, &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let probes = exec.calls_containing("restic snapshots");
    assert!(!probes.is_empty(), "E3 must ask before it decides");
    for call in &probes {
        assert!(
            call.contains("rclone:hdd:homelab-backups/syncthing-config"),
            "the configured repository is the only one it may read: {}",
            call
        );
        assert!(
            call.contains("/etc/homelab/restic.pw"),
            "and the configured password file: {}",
            call
        );
    }
}

// ── D1: fresh deploy runs the full ordered sequence ─────────────────────────

#[tokio::test]
async fn d1_fresh_deploy_command_sequence() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &spec(110, "syncthing")).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let calls = exec.calls();
    let pos = |needle: &str| {
        calls
            .iter()
            .position(|c| c.contains(needle))
            .unwrap_or_else(|| panic!("missing call containing '{}' in {:#?}", needle, calls))
    };
    // Ordered invariants of the pipeline:
    let create = pos("pct create 110");
    let start = pos("pct start 110");
    let push = pos("pct push 110");
    let up = pos("compose up -d");
    let verify = pos("ps --status running --services");
    let route = pos("pct push 104");
    assert!(create < start, "create before start");
    assert!(start < push, "start before file push");
    assert!(push < up, "files pushed before compose up");
    assert!(up < verify, "verify runs after compose up");
    assert!(verify < route, "gateway route only after health is proven");
    // Boot policy (C3) is part of creation.
    assert!(calls[create].contains("--onboot 1"));
    assert!(calls[create].contains("--startup order=50"));
    // State recorded (AR4).
    let state = exec
        .file("/var/lib/homelab/state.json")
        .expect("state written");
    assert!(state.contains("\"syncthing\""));
    assert!(state.contains("110-app-syncthing"));
}

// ── B1: second run is quiet (no create, no restarts, no re-enable) ──────────

/// T52: pushing a config file is not the same as the service reading it.
///
/// `docker compose up -d` sees an unchanged compose definition and leaves the
/// container running, so an edit to a bind-mounted config takes effect at the
/// next unrelated restart — or never. On 2026-08-31 that cost a wrong
/// conclusion: promtail ran four more minutes on the old pipeline while I
/// concluded the fix had not worked.
#[tokio::test]
async fn t52_a_changed_config_file_restarts_its_app() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 110-app-syncthing\ncores: 1\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    let mut spec = spec(110, "syncthing");
    // The compose file is already in place — only the config beside it moved.
    exec.respond_always(
        "sha256sum '/opt/syncthing/syncthing/docker-compose.yml'",
        CmdOutput::ok(&sha_hex(&spec.files[0].content)),
    );
    spec.files.push(FileBlob {
        path: "syncthing/config.yml".into(),
        content: "changed\n".into(),
        mode: None,
    });
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        exec.calls_containing("docker compose restart")
            .iter()
            .any(|c| c.contains("/opt/syncthing/syncthing")),
        "the app whose config changed must be restarted: {:?}",
        exec.calls_containing("docker compose")
    );
}

/// The other half: when only the compose file changed, `up -d` recreates the
/// container by itself and a restart on top of that is pure churn.
#[tokio::test]
async fn t52_a_changed_compose_file_does_not_restart_twice() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 110-app-syncthing\ncores: 1\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    // Every sha lookup misses, so the compose file counts as changed.
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        exec.calls_containing("docker compose restart").is_empty(),
        "compose up already recreated it: {:?}",
        exec.calls_containing("docker compose")
    );
}

/// T56: an image behind a login must not need a step nobody wrote down.
///
/// kp-soft was the first stack with a private image, and on 2026-08-31 the
/// deploy failed at the pull until a `docker login` was run on the container
/// by hand. That login lived in no manifest, so a container rebuilt from
/// scratch would have failed the same way with nothing to say why.
#[tokio::test]
async fn t56_a_private_registry_is_signed_into_before_the_pull() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 110-app-syncthing\ncores: 1\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    let mut spec = spec(110, "syncthing");
    spec.manifest.registry_login = Some(homelab_core::manifest::RegistryLogin {
        registry: "ghcr.io".into(),
        app: "syncthing".into(),
    });
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec).await;
    assert!(report.ok, "{:?}", report.error);

    let calls = exec.calls();
    let pos = |n: &str| calls.iter().position(|c| c.contains(n)).unwrap();
    assert!(
        calls.iter().any(|c| c.contains("docker login ghcr.io")),
        "the deploy must sign in: {:?}",
        exec.calls_containing("docker")
    );
    assert!(
        pos("docker login ghcr.io") < pos("docker compose pull"),
        "signing in after the pull is signing in too late"
    );
    // The token must never be handed to docker as an argument, where it would
    // sit in the process list of a machine other people can read.
    assert!(
        exec.calls_containing("docker login")
            .iter()
            .all(|c| c.contains("--password-stdin")),
        "the token must arrive on stdin"
    );
}

/// A stack with no private registry must not gain a login step it never
/// asked for.
#[tokio::test]
async fn t56_a_stack_without_a_registry_never_logs_in() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 110-app-syncthing\ncores: 1\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &j), &spec(110, "syncthing")).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(exec.calls_containing("docker login").is_empty());
}

#[tokio::test]
async fn b1_second_run_is_quiet() {
    use homelab_core::ops::guards;
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 110-app-syncthing\ncores: 1\n"),
    );
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always(
        "ps --status running --services",
        CmdOutput::ok("syncthing\n"),
    );
    // Guards find their exact content already in place → no restarts.
    exec.respond_always(
        "sha256sum '/etc/docker/daemon.json'",
        CmdOutput::ok(&sha_hex(guards::DOCKER_DAEMON_JSON)),
    );
    exec.respond_always(
        "sha256sum '/etc/systemd/journald.conf.d/homelab-limits.conf'",
        CmdOutput::ok(&sha_hex(guards::JOURNALD_LIMITS)),
    );
    exec.respond_always(
        "sha256sum '/etc/logrotate.d/homelab'",
        CmdOutput::ok(&sha_hex(guards::LOGROTATE_POLICY)),
    );
    exec.respond_always(
        "sha256sum '/etc/systemd/system/docker-prune.service'",
        CmdOutput::ok(&sha_hex(guards::PRUNE_SERVICE)),
    );
    exec.respond_always(
        "sha256sum '/etc/systemd/system/docker-prune.timer'",
        CmdOutput::ok(&sha_hex(guards::PRUNE_TIMER)),
    );
    exec.respond_always(
        "sha256sum '/etc/apt/apt.conf.d/60homelab-clean'",
        CmdOutput::ok(&sha_hex(guards::APT_AUTOCLEAN)),
    );
    exec.respond_always(
        "sha256sum '/etc/apt/apt.conf.d/50unattended-upgrades'",
        CmdOutput::ok(&sha_hex(guards::UNATTENDED_UPGRADES)),
    );
    exec.respond_always(
        "sha256sum '/opt/cadvisor/docker-compose.yml'",
        CmdOutput::ok(&sha_hex(guards::CADVISOR_COMPOSE)),
    );
    // Files already at destination content → pushes skipped too.
    let s = spec(110, "syncthing");
    exec.respond_always(
        "sha256sum '/opt/syncthing/syncthing/docker-compose.yml'",
        CmdOutput::ok(&sha_hex(&s.files[0].content)),
    );
    exec.respond_always(
        "sha256sum '/opt/traefik-config/routes/110-app-syncthing.yml'",
        CmdOutput::ok(&sha_hex(s.gateway_route.as_ref().unwrap().content.as_str())),
    );
    // Intent repo already committed → git commit exits non-zero (nothing to do).
    exec.respond_always(
        "git -C /var/lib/homelab/repo commit",
        CmdOutput::failed(1, "nothing to commit, working tree clean"),
    );

    let sink = VecSink::new();
    let journal = NullJournal;
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(report.ok, "second run failed: {:?}", report.error);

    for forbidden in [
        "pct create",
        "pct start",
        "pct push",
        "systemctl restart docker",
        "systemctl restart systemd-journald",
        "enable --now docker-prune.timer",
    ] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "second run was not quiet: found {}",
            forbidden
        );
    }
}

// ── A5: secrets never land in the intent repo; vault copy is 0600 ───────────

#[tokio::test]
async fn a5_env_goes_to_vault_never_to_repo() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing");
    s.env
        .insert("syncthing".into(), "SECRET_TOKEN=supersecret\n".into());
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(report.ok, "{:?}", report.error);

    let vault_path = "/var/lib/homelab/secrets/syncthing/syncthing.env";
    assert_eq!(
        exec.file(vault_path).as_deref(),
        Some("SECRET_TOKEN=supersecret\n")
    );
    assert_eq!(exec.file_mode(vault_path), Some(0o600));
    // Nothing under the repo tree may contain the secret.
    assert!(
        exec.file("/var/lib/homelab/repo/stacks/syncthing/syncthing/.env")
            .is_none(),
        "env file leaked into the intent repo"
    );
    // And the transcript never logs the value.
    for line in sink.lines() {
        assert!(
            !line.contains("supersecret"),
            "secret leaked into transcript: {}",
            line
        );
    }
}

// ── Gateway route constraint (H1) ───────────────────────────────────────────

#[tokio::test]
async fn h1_gateway_route_only_to_the_gateway() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut s = spec(110, "syncthing");
    s.gateway_route = Some(GatewayRoute {
        gateway_vmid: 106, // not the gateway!
        filename: "x.yml".into(),
        content: "http: {}".into(),
    });
    let report = deploy(&ctx(&exec, &sink, &journal), &s).await;
    assert!(!report.ok);
    assert!(report
        .error
        .unwrap()
        .why
        .contains("gateway routes may only target"));
    assert!(exec.calls_containing("pct push 106").is_empty());
}

/// F129 · the fallback half of C1: a cache that answers its probe but cannot
/// serve a blob must not be able to hold the deploy hostage.
///
/// The real case, 2026-09-01: the ghcr.io proxy answered `/v2/` in under a
/// millisecond, then streamed 157 MB of a layer, hit an HTTP/2 PROTOCOL_ERROR
/// against GitHub, returned 500 after 10m12s, and docker started over. The
/// deploy sat there until the 900 s step ceiling caught it — while the same
/// container pulled the same image directly at 4.1 MB/s. Seerr and
/// flaresolverr had to be started by hand.
#[tokio::test]
async fn f129_a_cache_that_cannot_serve_falls_back_to_the_real_registry() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    // The probe passes — that is the whole point, a broken cache looks alive.
    exec.respond_always("http://10.10.10.17:5001/v2/", CmdOutput::ok("UP\n"));
    // The pull through the cache fails; the retry afterwards succeeds.
    exec.enqueue("docker compose pull", CmdOutput::failed(1, "500 Internal"));
    exec.respond_always("docker compose pull", CmdOutput::ok(""));

    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.files[0].content = "services:\n  app:\n    image: ghcr.io/o/app:latest\n".into();
    // No gateway route: its push happens after the pull step and would
    // otherwise be the last thing staged, hiding what the fallback wrote.
    sp.gateway_route = None;

    let mut c = ctx(&exec, &sink, &journal);
    let cache = homelab_core::ops::registry_cache::CacheCfg {
        host: "10.10.10.17".into(),
        upstreams: vec![homelab_core::ops::registry_cache::CacheUpstream {
            registry: "ghcr.io".into(),
            port: 5001,
        }],
        pull_timeout_secs: 180,
    };
    c.registry_cache = Some(cache);

    let report = deploy(&c, &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    // Pushed twice: once pointed at the cache, once put back. push_content
    // asks the container for the file's hash before every write, so the
    // number of those asks is the number of push attempts.
    // Only the form push_content uses to decide whether to write; the S2
    // verification asks the same question with a different command, and
    // counting both would measure the check instead of the pushes.
    let pushes: Vec<String> = exec
        .calls_containing("sha256sum '/opt/syncthing/syncthing/docker-compose.yml'")
        .into_iter()
        .filter(|c| c.contains("cut -d"))
        .collect();
    assert_eq!(
        pushes.len(),
        2,
        "compose pushed once for the cache and once for the fallback: {:?}",
        pushes
    );
    // And what went back is the file naming the real registry — otherwise
    // `up -d` starts an image the cache still cannot serve.
    let staged = exec
        .file("/var/lib/homelab/push-staging")
        .expect("something was staged");
    assert!(
        staged.contains("ghcr.io/o/app:latest") && !staged.contains("10.10.10.17:5001"),
        "the last staged compose must name the real registry: {}",
        staged
    );

    // Bounded first attempt, full budget for the retry.
    let budgets = exec.timeouts_for("docker compose pull");
    assert_eq!(budgets, vec![180, 900], "first attempt bounded, retry not");

    // The truncated blob is cleared, or the direct pull dies on a digest
    // mismatch that reads like a broken image.
    assert_eq!(
        exec.calls_containing("docker system prune -f").len(),
        1,
        "the abandoned layer must be pruned before the retry"
    );

    assert!(
        sink.lines().iter().any(|l| l.contains("falling back")),
        "the fallback must be visible in the transcript, not silent"
    );
}

/// S2a · a deploy that stops half-way must still leave a record.
///
/// The case, 2026-09-01: the media stack failed at "start apps", so the
/// `record state` step — the last one — never ran, and the orchestrator did
/// not know the stack existed. Nine containers were running and nothing was
/// backing up 12 GB of their configuration. A stack that is plainly broken is
/// recoverable; one that is invisible is not, because nobody goes looking.
#[tokio::test]
async fn s2_a_failed_deploy_still_records_the_stack() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_always("docker compose pull", CmdOutput::failed(1, "boom"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let sp = spec(110, "syncthing");

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok, "this deploy is meant to fail");

    let state = exec
        .file("/var/lib/homelab/state.json")
        .expect("state was written even though the deploy failed");
    assert!(
        state.contains("\"syncthing\"") && state.contains("incomplete_step"),
        "the stack must exist in state and say where it stopped: {}",
        state
    );
    assert!(
        sink.lines()
            .iter()
            .any(|l| l.contains("recorded as incomplete")),
        "and must say so out loud"
    );
}

/// S2a-bis · but a refusal is not a half-deploy. A1 promises that a no-touch
/// target runs zero commands, and writing a state record is a command.
#[tokio::test]
async fn s2_a_refused_target_records_nothing_at_all() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let sp = spec(100, "syncthing");

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok, "vmid 100 is on the no-touch list");
    assert!(
        exec.file("/var/lib/homelab/state.json").is_none(),
        "a refusal leaves nothing behind, state included"
    );
}

/// S2b · the reconciliation pass rejects a deploy whose steps all succeeded
/// but whose container does not match the stack file.
///
/// The divergence used here is the one no single step can catch: the app was
/// started, `docker compose up -d` returned zero, and the container is not
/// running. A crash loop exits zero on the way in.
#[tokio::test]
async fn s2_reconcile_catches_a_container_that_does_not_match() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_first("docker ps", CmdOutput::ok(""));
    let sink = VecSink::new();
    let journal = NullJournal;
    let sp = spec(110, "syncthing");

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok, "an app that is not running is not a success");
    let err = format!("{:?}", report.error);
    assert!(
        err.contains("does not match") && err.contains("syncthing' is not running"),
        "the error must name what is wrong: {}",
        err
    );
}

/// S2c · a push that reports success and did not land fails its own step,
/// not three steps later.
///
/// F124 is the case this is shaped after: a unit file was pushed onto the
/// path that held a running program's own binary. The push reported success —
/// it had, after all, written a file — and the service survived only because
/// the kernel keeps a deleted file open for a process still running it.
/// Reading the file back says immediately that what is there is not what was
/// sent.
#[tokio::test]
async fn s2_a_push_that_did_not_land_fails_its_own_step() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    // The container answers every hash query with something else.
    exec.respond_first("sha256sum", CmdOutput::ok("0000000000000000 /opt/x\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let sp = spec(110, "syncthing");

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(!report.ok, "a push that did not land is not a success");
    let err = format!("{:?}", report.error);
    assert!(
        err.contains("push files") && err.contains("the change is not there"),
        "the step must name itself and say what is wrong: {}",
        err
    );
}

/// The declared owner of a data directory must be the uid the app actually
/// runs as — a fact only the image knows.
///
/// Four failures in two days had this root: a render gid assumed to be 104
/// when the machine said 993, and a blanket chown to 100000 that handed
/// Loki's database to root while Loki runs as 10001. It crash-looped with
/// `permission denied`, the front door stayed down, and the cause had to be
/// read out of a Go stack trace. Validation could not have caught any of it:
/// it checks the +100000 container mapping, which was correct every time.
#[tokio::test]
async fn storage_owned_by_the_wrong_uid_fails_the_deploy() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    // The app runs as 10001 inside the container; unprivileged, so the host
    // owner must be 110001. The directory says 100000 — the shape of the
    // mistake, a plausible number that is simply not this app's.
    exec.respond_first(
        "--format '{{.Config.User}}",
        CmdOutput::ok("10001|PATH=/usr/bin \n"),
    );
    exec.respond_first("stat -c %u", CmdOutput::ok("100000\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.storage[0].host_owner_uid = Some(100_000);

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(
        !report.ok,
        "a directory the app cannot write is not a success"
    );
    let err = format!("{:?}", report.error);
    assert!(
        err.contains("110001") && err.contains("chown"),
        "the error must give the exact remedy, not a diagnosis to work out: {}",
        err
    );
}

/// And the same check must stay quiet when the ownership is right, or it
/// becomes a step everyone learns to ignore.
#[tokio::test]
async fn storage_owned_correctly_says_nothing() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    exec.respond_first(
        "--format '{{.Config.User}}",
        CmdOutput::ok("10001|PATH=/usr/bin \n"),
    );
    exec.respond_first("stat -c %u", CmdOutput::ok("110001\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.storage[0].host_owner_uid = Some(110_001);

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "correct ownership must pass: {:?}", report.error);
}

/// H4 · cAdvisor is installed on every managed docker host, not declared per
/// stack.
///
/// It was a per-stack app directory in seven of thirteen stacks while
/// `deploy.rs` wrote a Prometheus scrape target for every stack with apps.
/// The metrics and syncthing stacks were therefore scraped and answered
/// nothing — permanently down, permanently silent, because the HostDown rule
/// watches the node job and not this one. An empty container panel and a
/// working one look identical.
#[tokio::test]
async fn h4_cadvisor_is_installed_by_the_guards_on_every_docker_host() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    // A stack that does NOT declare cadvisor as one of its apps.
    let sp = spec(110, "syncthing");
    assert!(
        !sp.manifest.apps.iter().any(|a| a == "cadvisor"),
        "this test is only meaningful for a stack that does not declare it"
    );

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(report.ok, "deploy failed: {:?}", report.error);

    let staged = exec.file("/var/lib/homelab/push-staging");
    assert!(
        exec.calls_containing("/opt/cadvisor/docker-compose.yml")
            .iter()
            .any(|c| c.contains("sha256sum")),
        "the guards must push a cadvisor compose to this host: {:?}",
        staged
    );
    assert!(
        exec.calls_containing("cd /opt/cadvisor && docker compose up -d")
            .len()
            == 1,
        "and bring it up exactly once"
    );
}

/// And never on a container that runs no docker: a weekly prune timer on a
/// native-only host has been failing every week since it was installed, which
/// is worse than useless — a guard that fails on schedule teaches you to
/// ignore failures.
#[tokio::test]
async fn h4_cadvisor_is_not_installed_where_there_is_no_docker() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.native_only = true;
    sp.manifest.apps = Vec::new();
    sp.manifest.natives = vec!["thing".into()];
    sp.files = vec![homelab_core::manifest::FileBlob {
        path: "thing/thing.service".into(),
        content: "[Unit]\n".into(),
        mode: None,
    }];
    sp.manifest.storage = Vec::new();
    sp.gateway_route = None;

    let _ = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(
        exec.calls_containing("/opt/cadvisor").is_empty(),
        "a native-only host has no docker to report on"
    );
}

/// The uid that writes the data is not always the uid docker was asked for.
///
/// The linuxserver.io images — the *arr suite, syncthing, most of this fleet
/// — start as root and drop to PUID themselves. `Config.User` is empty and
/// `docker exec id` says root, while every file they write belongs to 1000.
/// Reading only Config.User produced a confident wrong answer on the first
/// stack it met: it told us to chown syncthing's configuration to root, which
/// would have broken it. A check that is confidently wrong is worse than no
/// check, because its output looks like an instruction.
#[tokio::test]
async fn ownership_reads_puid_before_the_docker_user() {
    let exec = MockExecutor::new();
    script_fresh(&exec);
    // Config.User empty, PUID=1000 — the linuxserver shape.
    exec.respond_first(
        "--format '{{.Config.User}}",
        CmdOutput::ok("|PATH=/usr/bin PUID=1000 PGID=1000 TZ=Europe/Brussels \n"),
    );
    exec.respond_first("stat -c %u", CmdOutput::ok("101000\n"));
    let sink = VecSink::new();
    let journal = NullJournal;
    let mut sp = spec(110, "syncthing");
    sp.manifest.storage[0].host_owner_uid = Some(101_000);

    let report = deploy(&ctx(&exec, &sink, &journal), &sp).await;
    assert!(
        report.ok,
        "1000 inside an unprivileged container IS 101000 on the host — this \
         must pass, not demand a chown to root: {:?}",
        report.error
    );
}
