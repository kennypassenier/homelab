//! Homelab HOST daemon (v2, merged architecture).
//!
//! Runs on the Proxmox host. One authenticated WebSocket line to CLIENT
//! carries RPC + a live log stream. All container work happens through
//! `pct` — LXCs run zero homelab code.
//!
//! Safety model (whitelist-only):
//! - Only VMIDs named in an incoming manifest are ever managed.
//! - A hard NO_TOUCH list refuses the pre-existing guests outright.
//! - An existing container is only reused when its hostname matches the
//!   canonical `{vmid}-app-{stack}` — anything else is a SAFETY ABORT.
//! - The only cross-stack write allowed is a single traefik route fragment
//!   into the gateway's watched routes directory.
//! - This build has NO destroy path at all.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::process::Command;
use tokio::sync::{broadcast, Mutex};

use homelab_proto::{
    Command as Rpc, DeploySpec, GatewayRoute, LogLevel, RpcRequest, RpcResponse, ServerMsg,
    StackManifest,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Guests that must never be managed, no matter what a manifest claims.
/// VM 101 = Home Assistant, 102/103 = infra, 104-107/111 = legacy ansible
/// stacks (104-106 migrate later, explicitly), 100 + 201-203 = OPNsense/k3s.
const NO_TOUCH: &[u16] = &[100, 101, 102, 103, 104, 105, 106, 107, 111, 201, 202, 203];

/// The one exception: the gateway LXC may receive traefik route fragments
/// in exactly this directory, nothing else.
const GATEWAY_VMID: u16 = 104;
const GATEWAY_ROUTES_DIR: &str = "/opt/traefik-config/routes";

const STATE_DIR: &str = "/var/lib/homelab";

#[derive(Clone)]
struct AppState {
    token: String,
    log_tx: broadcast::Sender<ServerMsg>,
    /// Serializes deploys — one operation at a time.
    op_lock: Arc<Mutex<()>>,
}

struct Ctx {
    log_tx: broadcast::Sender<ServerMsg>,
}

impl Ctx {
    fn log(&self, level: LogLevel, source: &str, msg: impl Into<String>) {
        let msg = msg.into();
        let line = match level {
            LogLevel::Error => format!("[ERR] {}", msg),
            LogLevel::Warn => format!("[WRN] {}", msg),
            _ => msg.clone(),
        };
        eprintln!("{} :: {}", source, line);
        let _ = self.log_tx.send(ServerMsg::Log {
            level,
            source: source.to_string(),
            msg,
        });
    }

    /// Run a host command, streaming a transcript into the log channel.
    async fn run(&self, program: &str, args: &[&str], timeout_s: u64) -> Result<String, String> {
        let rendered = format!("{} {}", program, args.join(" "));
        self.log(LogLevel::Debug, "HOST", format!("[run ] {}", rendered));
        let fut = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .output();
        let out = tokio::time::timeout(Duration::from_secs(timeout_s), fut)
            .await
            .map_err(|_| format!("timeout after {}s: {}", timeout_s, rendered))?
            .map_err(|e| format!("spawn failed: {}: {}", rendered, e))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        for l in stdout.lines().chain(stderr.lines()).take(40) {
            if !l.trim().is_empty() {
                self.log(LogLevel::Debug, "HOST", format!("  {}", l));
            }
        }
        if out.status.success() {
            self.log(LogLevel::Debug, "HOST", format!("[exit] ok :: {}", rendered));
            Ok(stdout)
        } else {
            Err(format!(
                "[exit] rc={} :: {} :: {}",
                out.status.code().unwrap_or(-1),
                rendered,
                stderr.trim()
            ))
        }
    }

    /// Run a shell script inside the container via pct exec.
    async fn pct_sh(&self, vmid: u16, script: &str, timeout_s: u64) -> Result<String, String> {
        let vm = vmid.to_string();
        self.run("pct", &["exec", &vm, "--", "sh", "-c", script], timeout_s)
            .await
    }

    /// Push literal file content into the container. Returns true when the
    /// destination changed (used to trigger conditional service restarts).
    async fn push_content(
        &self,
        vmid: u16,
        dest: &str,
        content: &str,
        perms: &str,
    ) -> Result<bool, String> {
        let current = self
            .pct_sh(vmid, &format!("cat '{}' 2>/dev/null || true", dest), 30)
            .await
            .unwrap_or_default();
        if current == content {
            return Ok(false);
        }
        if let Some(parent) = std::path::Path::new(dest).parent() {
            self.pct_sh(vmid, &format!("mkdir -p '{}'", parent.display()), 30)
                .await?;
        }
        let tmp = format!("/tmp/homelab-content-{}", std::process::id());
        tokio::fs::write(&tmp, content).await.map_err(|e| e.to_string())?;
        let res = self
            .run(
                "pct",
                &["push", &vmid.to_string(), &tmp, dest, "--perms", perms],
                60,
            )
            .await;
        let _ = tokio::fs::remove_file(&tmp).await;
        res.map(|_| true)
    }
}

// ── Runaway guards ───────────────────────────────────────────────────────────
// Every managed container gets hard caps on everything that grows unattended:
// Docker json logs, the systemd journal, syslog, stale Docker images, and the
// apt cache. Idempotent — re-applied (and only then acted upon) each deploy.

const DOCKER_DAEMON_JSON: &str = r#"{
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3"
  }
}
"#;

const JOURNALD_LIMITS: &str = "[Journal]\nSystemMaxUse=100M\nRuntimeMaxUse=50M\nMaxRetentionSec=1month\n";

const LOGROTATE_POLICY: &str = r#"/var/log/syslog /var/log/messages /var/log/auth.log {
    daily
    rotate 7
    maxsize 50M
    missingok
    notifempty
    compress
    delaycompress
    sharedscripts
    postrotate
        /usr/lib/rsyslog/rsyslog-rotate 2>/dev/null || true
    endscript
}
"#;

const APT_AUTOCLEAN: &str =
    "APT::Periodic::AutocleanInterval \"7\";\nAPT::Periodic::CleanInterval \"7\";\n";

const PRUNE_SERVICE: &str = "[Unit]\nDescription=Prune stale Docker data (homelab runaway guard)\n\n[Service]\nType=oneshot\nExecStart=/usr/bin/docker system prune -f --filter until=168h\n";

const PRUNE_TIMER: &str = "[Unit]\nDescription=Weekly Docker prune (homelab runaway guard)\n\n[Timer]\nOnCalendar=weekly\nRandomizedDelaySec=1h\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n";

async fn apply_runaway_guards(ctx: &Ctx, vmid: u16) -> Result<(), String> {
    ctx.log(LogLevel::Info, "HOST", "[sync][run ] runaway guards (logs, journal, prune, apt)");

    // 1. Docker container logs: bounded json-file driver. Must land before
    //    app containers are (re)created so they inherit the caps.
    let docker_changed = ctx
        .push_content(vmid, "/etc/docker/daemon.json", DOCKER_DAEMON_JSON, "644")
        .await?;
    if docker_changed {
        ctx.pct_sh(vmid, "systemctl restart docker", 120).await?;
        ctx.log(LogLevel::Info, "HOST", "[guard] docker log caps applied (10m x 3)");
    }

    // 2. systemd journal: hard size + retention limits.
    let journal_changed = ctx
        .push_content(
            vmid,
            "/etc/systemd/journald.conf.d/homelab-limits.conf",
            JOURNALD_LIMITS,
            "644",
        )
        .await?;
    if journal_changed {
        ctx.pct_sh(vmid, "systemctl restart systemd-journald", 60).await?;
        ctx.log(LogLevel::Info, "HOST", "[guard] journald capped at 100M / 1 month");
    }

    // 3. Classic syslog files: logrotate policy (logrotate ships with Debian;
    //    install is a no-op when present).
    ctx.pct_sh(
        vmid,
        "command -v logrotate >/dev/null || (export DEBIAN_FRONTEND=noninteractive; apt-get install -y -qq logrotate)",
        300,
    )
    .await?;
    ctx.push_content(vmid, "/etc/logrotate.d/homelab", LOGROTATE_POLICY, "644")
        .await?;

    // 4. Stale Docker data: weekly prune timer (watchtower-style :latest
    //    updates otherwise accumulate old image layers forever).
    ctx.push_content(vmid, "/etc/systemd/system/docker-prune.service", PRUNE_SERVICE, "644")
        .await?;
    let timer_changed = ctx
        .push_content(vmid, "/etc/systemd/system/docker-prune.timer", PRUNE_TIMER, "644")
        .await?;
    if timer_changed {
        ctx.pct_sh(vmid, "systemctl daemon-reload && systemctl enable --now docker-prune.timer", 60)
            .await?;
        ctx.log(LogLevel::Info, "HOST", "[guard] weekly docker prune timer armed");
    }

    // 5. apt cache: periodic autoclean + clean up after our own bootstrap.
    ctx.push_content(vmid, "/etc/apt/apt.conf.d/60homelab-clean", APT_AUTOCLEAN, "644")
        .await?;
    ctx.pct_sh(vmid, "apt-get clean", 60).await?;

    ctx.log(LogLevel::Info, "HOST", "[guard] runaway guards in place ✓");
    Ok(())
}

#[tokio::main]
async fn main() {
    let token = std::env::var("HOMELAB_TOKEN").unwrap_or_default();
    if token.len() < 16 {
        eprintln!("FATAL: HOMELAB_TOKEN must be set (>=16 chars). Refusing to start unauthenticated.");
        std::process::exit(1);
    }
    let listen: SocketAddr = std::env::var("HOMELAB_LISTEN")
        .unwrap_or_else(|_| "0.0.0.0:8443".into())
        .parse()
        .expect("HOMELAB_LISTEN must be host:port");

    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState {
        token,
        log_tx,
        op_lock: Arc::new(Mutex::new(())),
    };

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/version", get(|| async { VERSION }))
        .route("/api/ws", get(ws_upgrade))
        .with_state(state);

    eprintln!("homelab-host v{} listening on {}", VERSION, listen);
    let listener = tokio::net::TcpListener::bind(listen).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn ws_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let authed = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == format!("Bearer {}", state.token))
        .unwrap_or(false);
    if !authed {
        return (StatusCode::UNAUTHORIZED, "missing or invalid bearer token").into_response();
    }
    ws.on_upgrade(move |socket| ws_session(socket, state))
        .into_response()
}

async fn ws_session(socket: WebSocket, state: AppState) {
    let (mut tx, mut rx) = socket.split();
    let hello = ServerMsg::Hello {
        version: VERSION.into(),
        proto: homelab_proto::PROTO_VERSION,
    };
    let _ = tx
        .send(Message::Text(serde_json::to_string(&hello).unwrap()))
        .await;

    // Fan the broadcast log stream into this socket.
    let mut log_rx = state.log_tx.subscribe();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMsg>(256);
    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(msg) = log_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.is_err() {
                        break;
                    }
                }
                Some(msg) = out_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.is_err() {
                        break;
                    }
                }
                else => break,
            }
        }
    });

    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(req) = serde_json::from_str::<RpcRequest>(&text) else {
            continue;
        };
        let ctx = Ctx {
            log_tx: state.log_tx.clone(),
        };
        let resp = match req.command {
            Rpc::Ping => RpcResponse {
                id: req.id,
                ok: true,
                message: "pong".into(),
            },
            Rpc::Status => {
                let msg = status(&ctx).await;
                RpcResponse {
                    id: req.id,
                    ok: true,
                    message: msg,
                }
            }
            Rpc::DeployStack(spec) => {
                let _guard = state.op_lock.lock().await;
                match deploy(&ctx, &spec).await {
                    Ok(summary) => RpcResponse {
                        id: req.id,
                        ok: true,
                        message: summary,
                    },
                    Err(e) => {
                        ctx.log(LogLevel::Error, "HOST", format!("deploy failed: {}", e));
                        RpcResponse {
                            id: req.id,
                            ok: false,
                            message: e,
                        }
                    }
                }
            }
        };
        let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
    }
    forward.abort();
}

async fn status(ctx: &Ctx) -> String {
    let pct = ctx.run("pct", &["list"], 30).await.unwrap_or_default();
    let state = tokio::fs::read_to_string(format!("{}/state.json", STATE_DIR))
        .await
        .unwrap_or_else(|_| "{}".into());
    format!("pct list:\n{}\nmanaged state:\n{}", pct, state)
}

// ── Deploy pipeline ──────────────────────────────────────────────────────────

async fn deploy(ctx: &Ctx, spec: &DeploySpec) -> Result<String, String> {
    let m = &spec.manifest;
    ctx.log(
        LogLevel::Info,
        "HOST",
        format!("[sync][run ] deploy {} (vmid {})", m.stack_name, m.vmid),
    );

    safety_check(ctx, m).await?;
    ensure_storage(ctx, m).await?;
    let created = ensure_container(ctx, m).await?;
    wait_ready(ctx, m.vmid).await?;
    bootstrap_docker(ctx, m.vmid).await?;
    apply_runaway_guards(ctx, m.vmid).await?;
    commit_to_repo(ctx, spec).await?;
    push_files(ctx, spec).await?;
    start_apps(ctx, m).await?;
    verify(ctx, m).await?;
    if let Some(route) = &spec.gateway_route {
        push_gateway_route(ctx, route).await?;
    }
    write_state(ctx, m).await?;

    let summary = format!(
        "Sync complete — {} {} · {} app(s) running, verified",
        if created { "provisioned" } else { "updated" },
        m.hostname,
        m.apps.len()
    );
    ctx.log(LogLevel::Info, "HOST", format!("[sync] {}", summary));
    Ok(summary)
}

async fn safety_check(ctx: &Ctx, m: &StackManifest) -> Result<(), String> {
    ctx.log(LogLevel::Info, "HOST", "[gate] safety checks");
    if NO_TOUCH.contains(&m.vmid) {
        return Err(format!(
            "SAFETY ABORT: vmid {} is on the no-touch list",
            m.vmid
        ));
    }
    let expected = format!("{}-app-{}", m.vmid, m.stack_name);
    if m.hostname != expected {
        return Err(format!(
            "SAFETY ABORT: hostname {} does not match canonical {}",
            m.hostname, expected
        ));
    }
    if !m
        .stack_name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("SAFETY ABORT: stack name must be lowercase [a-z0-9-]".into());
    }
    // A QEMU VM with this id is always fatal.
    if ctx
        .run("qm", &["status", &m.vmid.to_string()], 30)
        .await
        .is_ok()
    {
        return Err(format!("SAFETY ABORT: vmid {} is a QEMU VM", m.vmid));
    }
    // An existing container is only reusable if its hostname matches.
    if let Ok(cfg) = ctx.run("pct", &["config", &m.vmid.to_string()], 30).await {
        let host_line = cfg
            .lines()
            .find(|l| l.starts_with("hostname:"))
            .map(|l| l.trim_start_matches("hostname:").trim().to_string())
            .unwrap_or_default();
        if host_line != expected {
            return Err(format!(
                "SAFETY ABORT: vmid {} exists with hostname '{}', expected '{}' — refusing",
                m.vmid, host_line, expected
            ));
        }
        ctx.log(
            LogLevel::Info,
            "HOST",
            format!("[gate] vmid {} already ours ({}) — reuse", m.vmid, expected),
        );
    }
    ctx.log(LogLevel::Info, "HOST", "[gate] PASS");
    Ok(())
}

async fn ensure_storage(ctx: &Ctx, m: &StackManifest) -> Result<(), String> {
    for mount in &m.storage {
        if !mount.host_path.starts_with("/appdata/") {
            return Err(format!(
                "SAFETY ABORT: host_path {} must live under /appdata/",
                mount.host_path
            ));
        }
        ctx.run("mkdir", &["-p", &mount.host_path], 30).await?;
        if let Some(uid) = mount.host_owner_uid {
            let owner = format!("{}:{}", uid, uid);
            ctx.run("chown", &["-R", &owner, &mount.host_path], 60).await?;
        }
    }
    Ok(())
}

async fn ensure_container(ctx: &Ctx, m: &StackManifest) -> Result<bool, String> {
    let vm = m.vmid.to_string();
    let exists = ctx.run("pct", &["status", &vm], 30).await.is_ok();
    if !exists {
        ctx.log(
            LogLevel::Info,
            "HOST",
            format!("[sync][run ] pct create {} ({})", vm, m.hostname),
        );
        let rootfs = format!("{}:{}", m.resources.storage, m.resources.disk_gb);
        let mut net = format!(
            "name=eth0,bridge={},firewall=0,ip={},gw={}",
            m.network.bridge, m.network.ip, m.network.gateway
        );
        if let Some(tag) = m.network.vlan {
            net.push_str(&format!(",tag={}", tag));
        }
        let mem = m.resources.memory_mb.to_string();
        let swap = m.resources.swap_mb.to_string();
        let cores = m.resources.cores.to_string();
        let unpriv = if m.lxc.unprivileged { "1" } else { "0" };
        let onboot = if m.boot.onboot { "1" } else { "0" };
        let mut args: Vec<String> = vec![
            "create".into(),
            vm.clone(),
            m.lxc.template.clone(),
            "--hostname".into(),
            m.hostname.clone(),
            "--rootfs".into(),
            rootfs,
            "--net0".into(),
            net,
            "--memory".into(),
            mem,
            "--swap".into(),
            swap,
            "--cores".into(),
            cores,
            "--unprivileged".into(),
            unpriv.into(),
            "--features".into(),
            m.lxc.features.clone(),
            "--onboot".into(),
            onboot.into(),
        ];
        if let Some(order) = m.boot.order {
            args.push("--startup".into());
            args.push(format!("order={}", order));
        }
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        ctx.run("pct", &arg_refs, 300).await?;

        for (i, mount) in m.storage.iter().enumerate() {
            let mp = format!("-mp{}", i);
            let val = format!("{},mp={}", mount.host_path, mount.mount_point);
            ctx.run("pct", &["set", &vm, &mp, &val], 60).await?;
        }
    }

    let status = ctx.run("pct", &["status", &vm], 30).await?;
    if !status.contains("running") {
        ctx.run("pct", &["start", &vm], 120).await?;
    }
    Ok(!exists)
}

async fn wait_ready(ctx: &Ctx, vmid: u16) -> Result<(), String> {
    ctx.log(LogLevel::Info, "HOST", "[sync][run ] wait for systemd");
    for _ in 0..30 {
        if let Ok(out) = ctx
            .pct_sh(vmid, "systemctl is-system-running 2>/dev/null || true", 20)
            .await
        {
            let s = out.trim();
            if s.contains("running") || s.contains("degraded") {
                return Ok(());
            }
        }
        tokio::time::sleep(Duration::from_secs(4)).await;
    }
    Err("container never reached running/degraded".into())
}

async fn bootstrap_docker(ctx: &Ctx, vmid: u16) -> Result<(), String> {
    if ctx.pct_sh(vmid, "docker --version", 30).await.is_ok() {
        ctx.log(LogLevel::Info, "HOST", "[boot] docker already present");
        return Ok(());
    }
    ctx.log(LogLevel::Info, "HOST", "[sync][run ] bootstrap docker engine");
    ctx.pct_sh(
        vmid,
        "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq curl ca-certificates",
        600,
    )
    .await?;
    ctx.pct_sh(vmid, "curl -fsSL https://get.docker.com | sh", 900).await?;
    ctx.pct_sh(vmid, "systemctl enable --now docker", 120).await?;
    Ok(())
}

/// Non-secret files land in HOST's local git repo — history and rollback
/// without GitHub in the critical path.
async fn commit_to_repo(ctx: &Ctx, spec: &DeploySpec) -> Result<(), String> {
    let repo = format!("{}/repo", STATE_DIR);
    let stack_dir = format!("{}/stacks/{}", repo, spec.manifest.stack_name);
    tokio::fs::create_dir_all(&stack_dir)
        .await
        .map_err(|e| e.to_string())?;
    if ctx
        .run("git", &["-C", &repo, "rev-parse", "--git-dir"], 20)
        .await
        .is_err()
    {
        ctx.run("git", &["-C", &repo, "init", "-q"], 30).await?;
        ctx.run(
            "git",
            &["-C", &repo, "config", "user.email", "host@homelab.local"],
            20,
        )
        .await?;
        ctx.run("git", &["-C", &repo, "config", "user.name", "homelab-host"], 20)
            .await?;
    }
    for f in &spec.files {
        if f.path.contains("..") {
            return Err(format!("SAFETY ABORT: path traversal in {}", f.path));
        }
        let dest = format!("{}/{}", stack_dir, f.path);
        if let Some(parent) = std::path::Path::new(&dest).parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&dest, &f.content)
            .await
            .map_err(|e| e.to_string())?;
    }
    ctx.run("git", &["-C", &repo, "add", "-A"], 30).await?;
    let msg = format!("deploy {}", spec.manifest.stack_name);
    // Commit is a no-op when nothing changed; that's fine.
    let _ = ctx
        .run("git", &["-C", &repo, "commit", "-q", "-m", &msg], 30)
        .await;
    ctx.log(LogLevel::Info, "HOST", "[git] intent committed to local repo");
    Ok(())
}

async fn push_files(ctx: &Ctx, spec: &DeploySpec) -> Result<(), String> {
    let m = &spec.manifest;
    let vm = m.vmid.to_string();
    ctx.log(LogLevel::Info, "HOST", "[sync][run ] pct push files");
    for f in &spec.files {
        if f.path.contains("..") || f.path.starts_with('/') {
            return Err(format!("SAFETY ABORT: bad file path {}", f.path));
        }
        let dest = format!("/opt/{}/{}", m.stack_name, f.path);
        let dir = std::path::Path::new(&dest)
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        ctx.pct_sh(m.vmid, &format!("mkdir -p '{}'", dir), 30).await?;
        let tmp = format!("/tmp/homelab-push-{}", std::process::id());
        tokio::fs::write(&tmp, &f.content).await.map_err(|e| e.to_string())?;
        let perms = format!("{:o}", f.mode.unwrap_or(0o644));
        ctx.run("pct", &["push", &vm, &tmp, &dest, "--perms", &perms], 60)
            .await?;
        let _ = tokio::fs::remove_file(&tmp).await;
        ctx.log(LogLevel::Debug, "HOST", format!("  pushed {}", dest));
    }
    // Secrets: pushed with tight perms, never written into the git repo.
    for (app, env) in &spec.env {
        let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
        let tmp = format!("/tmp/homelab-env-{}", std::process::id());
        tokio::fs::write(&tmp, env).await.map_err(|e| e.to_string())?;
        ctx.run("pct", &["push", &vm, &tmp, &dest, "--perms", "600"], 60)
            .await?;
        let _ = tokio::fs::remove_file(&tmp).await;
        // Also keep a HOST-side vault copy (0600) for redeploys.
        let vault_dir = format!("{}/secrets/{}", STATE_DIR, m.stack_name);
        tokio::fs::create_dir_all(&vault_dir).await.map_err(|e| e.to_string())?;
        let vault_file = format!("{}/{}.env", vault_dir, app);
        tokio::fs::write(&vault_file, env).await.map_err(|e| e.to_string())?;
        let _ = ctx.run("chmod", &["600", &vault_file], 20).await;
        ctx.log(
            LogLevel::Info,
            "HOST",
            format!("[vault] {} sealed (values not logged)", dest),
        );
    }
    Ok(())
}

async fn start_apps(ctx: &Ctx, m: &StackManifest) -> Result<(), String> {
    let net = format!("{}_net", m.stack_name);
    let _ = ctx
        .pct_sh(m.vmid, &format!("docker network create {} 2>/dev/null || true", net), 60)
        .await;
    for app in &m.apps {
        ctx.log(
            LogLevel::Info,
            "HOST",
            format!("[sync][run ] compose pull+up :: {}", app),
        );
        let dir = format!("/opt/{}/{}", m.stack_name, app);
        ctx.pct_sh(m.vmid, &format!("cd '{}' && docker compose pull -q", dir), 900)
            .await?;
        ctx.pct_sh(m.vmid, &format!("cd '{}' && docker compose up -d --remove-orphans", dir), 300)
            .await?;
    }
    Ok(())
}

async fn verify(ctx: &Ctx, m: &StackManifest) -> Result<(), String> {
    ctx.log(LogLevel::Info, "HOST", "[sync][run ] verify health gates");
    // Give containers a moment to settle before judging them.
    tokio::time::sleep(Duration::from_secs(5)).await;
    for app in &m.apps {
        let dir = format!("/opt/{}/{}", m.stack_name, app);
        let out = ctx
            .pct_sh(
                m.vmid,
                &format!("cd '{}' && docker compose ps --status running --services", dir),
                60,
            )
            .await?;
        if out.trim().is_empty() {
            let diag = ctx
                .pct_sh(m.vmid, &format!("cd '{}' && docker compose ps -a && docker compose logs --tail 20", dir), 60)
                .await
                .unwrap_or_default();
            return Err(format!("verify FAILED: {} has no running services\n{}", app, diag));
        }
        ctx.log(
            LogLevel::Info,
            "HOST",
            format!("[gate] {} :: running ✓", app),
        );
    }
    Ok(())
}

async fn push_gateway_route(ctx: &Ctx, route: &GatewayRoute) -> Result<(), String> {
    if route.gateway_vmid != GATEWAY_VMID {
        return Err(format!(
            "SAFETY ABORT: gateway routes may only target vmid {}",
            GATEWAY_VMID
        ));
    }
    let name = &route.filename;
    if name.contains('/') || name.contains("..") || !name.ends_with(".yml") {
        return Err(format!("SAFETY ABORT: bad route filename {}", name));
    }
    let dest = format!("{}/{}", GATEWAY_ROUTES_DIR, name);
    ctx.log(
        LogLevel::Info,
        "HOST",
        format!("[sync][run ] gateway route → {} (file-provider watch reloads)", dest),
    );
    let tmp = format!("/tmp/homelab-route-{}", std::process::id());
    tokio::fs::write(&tmp, &route.content).await.map_err(|e| e.to_string())?;
    ctx.run(
        "pct",
        &["push", &GATEWAY_VMID.to_string(), &tmp, &dest, "--perms", "644"],
        60,
    )
    .await?;
    let _ = tokio::fs::remove_file(&tmp).await;
    Ok(())
}

async fn write_state(ctx: &Ctx, m: &StackManifest) -> Result<(), String> {
    tokio::fs::create_dir_all(STATE_DIR).await.map_err(|e| e.to_string())?;
    let path = format!("{}/state.json", STATE_DIR);
    let mut state: BTreeMap<String, serde_json::Value> =
        match tokio::fs::read_to_string(&path).await {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => BTreeMap::new(),
        };
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state.insert(
        m.stack_name.clone(),
        serde_json::json!({
            "vmid": m.vmid,
            "hostname": m.hostname,
            "apps": m.apps,
            "applied_at": ts,
        }),
    );
    tokio::fs::write(&path, serde_json::to_string_pretty(&state).unwrap())
        .await
        .map_err(|e| e.to_string())?;
    ctx.log(LogLevel::Info, "HOST", "[state] state.json updated");
    Ok(())
}
