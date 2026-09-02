//! Standing rule 9 (hardening H15): E2E against REAL dependencies where
//! possible. The intent-history git flow runs against a real temporary git
//! repo — the exact class of failure the mock can't see (dubious ownership,
//! index locks, revert semantics) lives here.

use async_trait::async_trait;
use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor};
use homelab_core::manifest::{DeploySpec, FileBlob};
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

/// git + filesystem = REAL; hypervisor/docker commands = canned success.
struct HybridExec {
    root: std::path::PathBuf,
    pushed: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl HybridExec {
    /// Real fs for paths inside the tempdir (the git repo/state); everything
    /// else (guards writing /etc/...) is redirected into a shadow dir so the
    /// test never touches the real machine.
    fn sandbox(&self, path: &str) -> std::path::PathBuf {
        if path.starts_with(self.root.to_str().unwrap()) {
            std::path::PathBuf::from(path)
        } else {
            self.root.join("shadow").join(path.trim_start_matches('/'))
        }
    }
}

#[async_trait]
impl Executor for HybridExec {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        match cmd.program.as_str() {
            "git" => {
                let out = std::process::Command::new("git")
                    .args(&cmd.args)
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_SYSTEM", "/dev/null")
                    .output()
                    .map_err(|e| CoreError::Other(e.to_string()))?;
                Ok(CmdOutput {
                    stdout: String::from_utf8_lossy(&out.stdout).into(),
                    stderr: String::from_utf8_lossy(&out.stderr).into(),
                    code: out.status.code().unwrap_or(-1),
                })
            }
            "qm" => Ok(CmdOutput::failed(2, "no such vm")),
            "sh" => {
                let script = cmd.args.last().cloned().unwrap_or_default();
                if script.contains("ls -A") {
                    Ok(CmdOutput::ok("config\n")) // dirs non-empty → no E3
                } else {
                    Ok(CmdOutput::ok(""))
                }
            }
            "pct" => {
                let rendered = cmd.rendered();
                if rendered.contains("pct config") {
                    if self.root.join("created").exists() {
                        return Ok(CmdOutput::ok("hostname: 108-app-test\n"));
                    }
                    std::fs::write(self.root.join("created"), "1").ok();
                    return Ok(CmdOutput::failed(2, "does not exist"));
                }
                if rendered.contains("pct status") {
                    return Ok(CmdOutput::ok("status: running"));
                }
                if rendered.contains("is-system-running") {
                    return Ok(CmdOutput::ok("running"));
                }
                if rendered.contains("docker --version") {
                    return Ok(CmdOutput::ok("Docker 27"));
                }
                if rendered.contains("--status running --services") {
                    return Ok(CmdOutput::ok("app\n"));
                }
                // S2: the deploy now reads its own pushes back, so this fake
                // has to model the destination side of `pct push` — the file
                // is staged on disk here, so the hash is real, not invented.
                if rendered.contains("sha256sum") {
                    let mut out = String::new();
                    if let Ok(map) = self.pushed.lock() {
                        for (dest, hash) in map.iter() {
                            if rendered.contains(dest.as_str()) {
                                out.push_str(&format!("{}  {}\n", hash, dest));
                            }
                        }
                    }
                    return Ok(CmdOutput::ok(&out));
                }
                if rendered.contains("docker ps --format") {
                    return Ok(CmdOutput::ok("app\n"));
                }
                if cmd.args.first().map(|a| a.as_str()) == Some("push") {
                    if let (Some(src), Some(dest)) = (cmd.args.get(2), cmd.args.get(3)) {
                        if let Ok(content) = std::fs::read_to_string(self.sandbox(src)) {
                            if let Ok(mut map) = self.pushed.lock() {
                                map.insert(
                                    dest.clone(),
                                    homelab_core::manifest::sha256_hex(content.as_bytes()),
                                );
                            }
                        }
                    }
                }
                Ok(CmdOutput::ok(""))
            }
            _ => Ok(CmdOutput::ok("")),
        }
    }

    async fn write_file(&self, path: &str, content: &str, _mode: u32) -> Result<(), CoreError> {
        let p = self.sandbox(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| CoreError::State(e.to_string()))?;
        }
        std::fs::write(p, content).map_err(|e| CoreError::State(e.to_string()))
    }

    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        std::fs::read_to_string(self.sandbox(path)).map_err(|e| CoreError::State(e.to_string()))
    }

    async fn sleep_ms(&self, _ms: u64) {}
}

fn spec(files_content: &str) -> DeploySpec {
    let mut m = homelab_core::manifest::StackManifest {
        registry_login: None,
        retention: None,
        data_mounts: Vec::new(),
        native_only: false,
        natives: Vec::new(),
        stack_name: "test".into(),
        vmid: 108,
        hostname: "108-app-test".into(),
        network: homelab_core::manifest::NetworkSpec {
            ip: "10.10.10.8/24".into(),
            gateway: "10.10.10.1".into(),
            bridge: "vmbr0".into(),
            vlan: Some(10),
        },
        resources: homelab_core::manifest::ResourceSpec {
            cores: 1,
            memory_mb: 512,
            swap_mb: 256,
            disk_gb: 4,
            storage: "local-lvm".into(),
        },
        lxc: homelab_core::manifest::LxcSpec {
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
            protection: false,
            gpu: false,
            vpn: false,
        },
        boot: homelab_core::manifest::BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage: vec![],
        apps: vec!["app".into()],
    };
    m.hostname = m.canonical_hostname();
    DeploySpec {
        native_binaries: Default::default(),
        manifest: m,
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: files_content.into(),
            mode: None,
        }],
        env: Default::default(),
        gateway_route: None,
        checks: Default::default(),
    }
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[tokio::test]
async fn r9_intent_history_against_real_git_two_deploys_two_commits_revert_works() {
    let tmp = tempfile::tempdir().unwrap();
    let state_dir = tmp.path().to_str().unwrap().to_string();
    let exec = HybridExec {
        root: tmp.path().to_path_buf(),
        pushed: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let ctx = OpCtx {
        exec: &exec,
        sink: &sink,
        journal: &j,
        safety: SafetyConfig::default(),
        state_dir: state_dir.clone(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    };
    // Deploy 1: repo is initialized, one commit lands.
    let r1 = deploy(&ctx, &spec("services: {}\n")).await;
    assert!(r1.ok, "{:?}", r1.error);
    let repo = tmp.path().join("repo");
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]).trim(), "1");
    // Deploy 2, same content: real git says nothing-to-commit → still 1.
    let r2 = deploy(&ctx, &spec("services: {}\n")).await;
    assert!(r2.ok, "unchanged redeploy must stay green: {:?}", r2.error);
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]).trim(), "1");
    // Deploy 3, changed content: second commit.
    let r3 = deploy(&ctx, &spec("services: {changed: true}\n")).await;
    assert!(r3.ok, "{:?}", r3.error);
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]).trim(), "2");
    // D4's promise: revert restores the previous file content.
    git(&repo, &["revert", "--no-edit", "HEAD"]);
    let reverted =
        std::fs::read_to_string(repo.join("stacks/test/app/docker-compose.yml")).unwrap();
    assert_eq!(reverted, "services: {}\n");
}

#[tokio::test]
async fn r9_broken_git_identity_fails_the_deploy_not_silently() {
    // A repo where committing WILL fail for a non-benign reason: make the
    // repo dir read-only so git add/commit cannot write the index.
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::process::Command::new("git")
        .args(["-C", repo.to_str().unwrap(), "init", "-q"])
        .output()
        .unwrap();
    let mut perms = std::fs::metadata(repo.join(".git")).unwrap().permissions();
    use std::os::unix::fs::PermissionsExt;
    perms.set_mode(0o555);
    std::fs::set_permissions(repo.join(".git"), perms).unwrap();

    let exec = HybridExec {
        root: tmp.path().to_path_buf(),
        pushed: Default::default(),
    };
    let sink = VecSink::new();
    let j = NullJournal;
    let ctx = OpCtx {
        exec: &exec,
        sink: &sink,
        journal: &j,
        safety: SafetyConfig::default(),
        state_dir: tmp.path().to_str().unwrap().into(),
        now_unix: 1_760_000_000,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    };
    let r = deploy(&ctx, &spec("services: {}\n")).await;
    // Restore perms so tempdir cleanup works.
    let mut perms = std::fs::metadata(repo.join(".git")).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(repo.join(".git"), perms).unwrap();
    assert!(
        !r.ok,
        "a genuinely failing intent commit must FAIL the deploy (was silently green before H15)"
    );
}
