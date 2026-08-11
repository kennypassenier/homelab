//! Homelab HOST daemon — thin shell around homelab-core (AR1).
//!
//! Provides: the real Executor (processes + files), config (TOML + env,
//! AR11), tracing (AR15), the journal file (B5), the WS server with required
//! bearer token, and the broadcast sink feeding connected clients (F2).

use std::io::Write;
use std::net::SocketAddr;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::process::Command;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use homelab_core::error::CoreError;
use homelab_core::executor::{Cmd, CmdOutput, Executor};
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::Journal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::{PipelineEvent, Sink};

use homelab_proto::{Command as Rpc, RpcRequest, RpcResponse, ServerMsg};

mod tls;

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Config (AR11) ────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, Default)]
struct FileConfig {
    token: Option<String>,
    listen: Option<String>,
    state_dir: Option<String>,
    backup_hour: Option<u8>,
    notify_webhook: Option<String>,
    retention: Option<Vec<homelab_proto::RetentionTier>>,
    exec_enabled: Option<bool>,
    mirror_remote: Option<String>,
}

#[derive(Clone)]
struct Config {
    token: String,
    listen: SocketAddr,
    state_dir: String,
    /// Path of the toml we loaded — SetConfig persists back to it.
    config_path: String,
    /// A6: remote exec endpoint switch. Deny-by-default; ssh-edited only
    /// (deliberately NOT in the G8 settings tab).
    exec_enabled: bool,
    /// D5: git remote URL for the offsite intent mirror; None = off.
    mirror_remote: Option<String>,
    /// Initial mutable settings (live copy lives in AppState.settings).
    initial_settings: homelab_proto::HostConfigView,
}

fn load_config() -> Config {
    let path = std::env::var("HOMELAB_CONFIG").unwrap_or_else(|_| "/etc/homelab/host.toml".into());
    let file: FileConfig = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| toml::from_str(&raw).ok())
        .unwrap_or_default();

    let token = std::env::var("HOMELAB_TOKEN")
        .ok()
        .or(file.token)
        .unwrap_or_default();
    if token.len() < 16 {
        eprintln!(
            "FATAL: token must be set (>=16 chars) via {} or HOMELAB_TOKEN",
            path
        );
        std::process::exit(1);
    }
    let listen = std::env::var("HOMELAB_LISTEN")
        .ok()
        .or(file.listen)
        .unwrap_or_else(|| "0.0.0.0:8443".into())
        .parse()
        .expect("listen must be host:port");
    let state_dir = std::env::var("HOMELAB_STATE_DIR")
        .ok()
        .or(file.state_dir)
        .unwrap_or_else(|| "/var/lib/homelab".into());
    Config {
        token,
        listen,
        state_dir,
        config_path: path,
        exec_enabled: file.exec_enabled.unwrap_or(false),
        mirror_remote: file.mirror_remote,
        initial_settings: homelab_proto::HostConfigView {
            backup_hour: file.backup_hour,
            notify_webhook: file.notify_webhook,
            retention: file
                .retention
                .unwrap_or_else(homelab_core::retention::default_tiers),
        },
    }
}

/// G8: persist the mutable settings back to host.toml, atomically, keeping
/// the immutable fields (token/listen/state_dir) intact.
fn persist_settings(
    config: &Config,
    settings: &homelab_proto::HostConfigView,
) -> Result<(), String> {
    #[derive(serde::Serialize)]
    struct Out<'a> {
        token: &'a str,
        listen: String,
        state_dir: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        backup_hour: Option<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        notify_webhook: Option<&'a String>,
        retention: &'a [homelab_proto::RetentionTier],
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        exec_enabled: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        mirror_remote: Option<&'a String>,
    }
    let out = Out {
        token: &config.token,
        listen: config.listen.to_string(),
        state_dir: &config.state_dir,
        backup_hour: settings.backup_hour,
        notify_webhook: settings.notify_webhook.as_ref(),
        retention: &settings.retention,
        exec_enabled: config.exec_enabled,
        mirror_remote: config.mirror_remote.as_ref(),
    };
    let raw = toml::to_string_pretty(&out).map_err(|e| e.to_string())?;
    let tmp = format!("{}.tmp", config.config_path);
    std::fs::write(&tmp, &raw).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &config.config_path).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Real executor (AR2) ─────────────────────────────────────────────────────

struct RealExecutor;

#[async_trait]
impl Executor for RealExecutor {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        // Transcript emission is handled by core's TracingExecutor inside the
        // pipeline; here we only trace at the log level for non-pipeline calls.
        let rendered = cmd.rendered();
        tracing::trace!("run {}", rendered);
        let fut = Command::new(&cmd.program)
            .args(&cmd.args)
            .stdin(Stdio::null())
            .output();
        let out = tokio::time::timeout(Duration::from_secs(cmd.timeout_s), fut)
            .await
            .map_err(|_| CoreError::Timeout {
                rendered: rendered.clone(),
                seconds: cmd.timeout_s,
            })?
            .map_err(|e| CoreError::Other(format!("spawn {}: {}", rendered, e)))?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        Ok(CmdOutput {
            stdout,
            stderr,
            code: out.status.code().unwrap_or(-1),
        })
    }

    /// Atomic by contract (AR4): write a temp file, fsync, rename over.
    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
        use std::os::unix::fs::PermissionsExt;
        let path = path.to_string();
        let content = content.to_string();
        tokio::task::spawn_blocking(move || -> Result<(), CoreError> {
            let p = std::path::Path::new(&path);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).map_err(|e| CoreError::State(e.to_string()))?;
            }
            let tmp = format!("{}.tmp", path);
            {
                let mut f =
                    std::fs::File::create(&tmp).map_err(|e| CoreError::State(e.to_string()))?;
                f.write_all(content.as_bytes())
                    .map_err(|e| CoreError::State(e.to_string()))?;
                f.sync_all().map_err(|e| CoreError::State(e.to_string()))?;
            }
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(mode))
                .map_err(|e| CoreError::State(e.to_string()))?;
            std::fs::rename(&tmp, &path).map_err(|e| CoreError::State(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| CoreError::Other(e.to_string()))?
    }

    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| CoreError::State(format!("{}: {}", path, e)))
    }

    async fn sleep_ms(&self, ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }
}

// ── Sink + journal adapters ─────────────────────────────────────────────────

struct BroadcastSink {
    log_tx: broadcast::Sender<ServerMsg>,
}

impl Sink for BroadcastSink {
    fn emit(&self, event: PipelineEvent) {
        let msg = match event {
            PipelineEvent::Line { level, source, msg } => {
                tracing::info!(source = %source, "{}", msg);
                ServerMsg::Log {
                    level: level.into(),
                    source,
                    msg,
                }
            }
            PipelineEvent::StepStarted { op, step } => ServerMsg::Log {
                level: homelab_proto::LogLevel::Info,
                source: "HOST".into(),
                msg: format!("[sync][run ] {} :: {}", op, step),
            },
            PipelineEvent::StepFinished { op, step, changed } => ServerMsg::Log {
                level: homelab_proto::LogLevel::Info,
                source: "HOST".into(),
                msg: format!(
                    "[sync][exit] {} :: {} :: {}",
                    op,
                    step,
                    if changed { "changed" } else { "ok (no change)" }
                ),
            },
            PipelineEvent::Bytes {
                op,
                label,
                done,
                total,
            } => ServerMsg::Transfer {
                op,
                label,
                done,
                total,
            },
        };
        let _ = self.log_tx.send(msg);
    }
}

/// B5/AR13: append-only JSONL journal; "running" records land before a step
/// executes, so an interrupted operation is visible after restart.
struct FileJournal {
    path: String,
}

impl Journal for FileJournal {
    fn record(&self, op: &str, step: &str, status: &str) {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let line = serde_json::json!({"ts": ts, "op": op, "step": step, "status": status});
        if let Some(parent) = std::path::Path::new(&self.path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ── Server ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    config: Config,
    log_tx: broadcast::Sender<ServerMsg>,
    op_lock: Arc<Mutex<()>>, // AR12: mutations strictly serial
    /// G8: live mutable settings (scheduler hour, webhook, retention).
    settings: Arc<std::sync::RwLock<homelab_proto::HostConfigView>>,
}

/// B7: minimal sd_notify — tell systemd we're alive without pulling in a
/// crate. No-op when NOTIFY_SOCKET is unset (dev runs).
fn sd_notify(msg: &str) {
    if let Ok(sock) = std::env::var("NOTIFY_SOCKET") {
        let addr = if let Some(stripped) = sock.strip_prefix('@') {
            format!("\0{}", stripped)
        } else {
            sock
        };
        if let Ok(s) = std::os::unix::net::UnixDatagram::unbound() {
            let _ = s.send_to(msg.as_bytes(), addr);
        }
    }
}

#[tokio::main]
async fn main() {
    // H5: the self-update gate runs `staged --selfcheck` before installing.
    // Prove we can execute at all and report our version, then exit.
    if std::env::args().any(|a| a == "--selfcheck") {
        println!("{}", VERSION);
        std::process::exit(0);
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let config = load_config();
    let (log_tx, _) = broadcast::channel(4096);
    let state = AppState {
        config: config.clone(),
        log_tx,
        op_lock: Arc::new(Mutex::new(())),
        settings: Arc::new(std::sync::RwLock::new(config.initial_settings.clone())),
    };

    // AR13: surface any operation the previous run left mid-flight.
    let mut interrupted: Vec<String> = Vec::new();
    if let Ok(journal) = std::fs::read_to_string(format!("{}/journal.jsonl", config.state_dir)) {
        for (op, step) in homelab_core::incidents::interrupted_ops(&journal) {
            tracing::warn!(
                "interrupted operation '{}' at step '{}' — re-running it is safe (idempotent)",
                op,
                step
            );
            interrupted.push(format!("{} @ {}", op, step));
        }
    }

    // F3: boot notification — after a power cut or crash-restart, Home
    // Assistant hears that the daemon is back, which version runs, and
    // whether anything was left mid-flight. Delayed so the network is up.
    {
        let boot_state = state.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let payload = serde_json::json!({
                "source": "homelab-host",
                "op": "host-online",
                "label": "boot",
                "ok": interrupted.is_empty(),
                "error": if interrupted.is_empty() { None } else {
                    Some(format!("interrupted: {}", interrupted.join("; ")))
                },
                "version": VERSION,
            })
            .to_string();
            notify_raw(&boot_state, &RealExecutor, payload).await;
        });
    }

    // E4: nightly scheduler — backups for every managed stack + auto-policy
    // updates, driven from state.json manifests (no client needed). Reads the
    // live settings each tick, so G8 edits apply without a restart.
    {
        let sched_state = state.clone();
        tokio::spawn(async move { scheduler_loop(sched_state).await });
        match config.initial_settings.backup_hour {
            Some(hour) => info!(
                "scheduler armed: daily backup + auto-updates at {:02}:00",
                hour
            ),
            None => info!("scheduler idle (backup_hour not set)"),
        }
    }

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/version", get(|| async { VERSION }))
        .route("/api/ws", get(ws_upgrade))
        .with_state(state);

    // A4: TLS with a self-signed cert; the client pins this fingerprint.
    let (certs, fingerprint) =
        tls::ensure_cert(&config.state_dir, "homelab-host").expect("tls cert");
    info!(
        "homelab-host v{} listening on {} (TLS)",
        VERSION, config.listen
    );
    info!("TLS fingerprint SHA256:{}", fingerprint);
    let tls_config =
        axum_server::tls_rustls::RustlsConfig::from_pem_file(&certs.cert_pem, &certs.key_pem)
            .await
            .expect("load tls");

    // H5: accept the self-update only after surviving 5s of real serving —
    // a binary that binds and then dies must leave the marker for OnFailure.
    let marker = format!("{}/selfupdate.pending", config.state_dir);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        if std::path::Path::new(&marker).exists() {
            let _ = std::fs::remove_file(&marker);
            info!("self-update accepted — now running v{}", VERSION);
        }
    });

    // B7: tell systemd we're ready, then feed its watchdog. If this loop
    // ever stops (deadlock/hang), systemd kills and restarts the daemon.
    sd_notify("READY=1");
    tokio::spawn(async {
        loop {
            tokio::time::sleep(Duration::from_secs(10)).await;
            sd_notify("WATCHDOG=1");
        }
    });

    axum_server::bind_rustls(config.listen, tls_config)
        .serve(app.into_make_service())
        .await
        .expect("serve");
}

/// E4: check every 20 minutes; when the local hour matches `hour` and a
/// stack's last backup is >20h old, run backup (E1) then auto-updates (D9)
/// for that stack. Uses the same op machinery as RPCs (op-lock, incidents,
/// notifications), so a client-triggered deploy never overlaps.
async fn scheduler_loop(state: AppState) {
    let exec = RealExecutor;
    loop {
        tokio::time::sleep(Duration::from_secs(20 * 60)).await;
        spawn_mirror_push(&state); // D5 retry queue: try again every tick
        let (hour, tiers) = {
            let s = state.settings.read().unwrap();
            match s.backup_hour {
                Some(h) => (h, s.retention.clone()),
                None => continue, // scheduler disabled
            }
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Host-local hour without pulling in chrono: read from `date`.
        let local_hour = match exec.run(&Cmd::new("date", &["+%H"], 10)).await {
            Ok(out) => out.stdout.trim().parse::<u8>().unwrap_or(255),
            Err(_) => 255,
        };
        if local_hour != hour {
            continue;
        }
        let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
        let snapshot = store.load().await;
        for (name, st) in snapshot.stacks {
            if now.saturating_sub(st.last_backup) < 20 * 3600 {
                continue; // already done today
            }
            let Some(manifest) = st.manifest else {
                tracing::warn!("scheduler: stack {} has no stored manifest — skipped", name);
                continue;
            };
            info!("scheduler: nightly run for {}", name);
            let m1 = manifest.clone();
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: tiers.clone(),
                ..Default::default()
            };
            let backup_report = run_mutating_op(&state, &exec, 0, "scheduled-backup", |ctx| {
                Box::pin(async move { homelab_core::ops::backup::backup(ctx, &m1, &cfg).await })
            })
            .await;
            if backup_report.ok {
                // Record last_backup so tomorrow's check is accurate.
                let mut s = store.load().await;
                if let Some(rec) = s.stacks.get_mut(&name) {
                    rec.last_backup = now;
                }
                let _ = store.save(s).await;
            }
            let m2 = manifest.clone();
            let _ = run_mutating_op(&state, &exec, 0, "scheduled-update", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::update::update(ctx, &m2, None, true).await },
                )
            })
            .await;
        }
    }
}

async fn ws_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let authed = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .map(|v| v == format!("Bearer {}", state.config.token))
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

    let mut log_rx = state.log_tx.subscribe();
    let (out_tx, mut out_rx) = tokio::sync::mpsc::channel::<ServerMsg>(256);
    let forward = tokio::spawn(async move {
        loop {
            tokio::select! {
                Ok(msg) = log_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.is_err() { break; }
                }
                Some(msg) = out_rx.recv() => {
                    if tx.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.is_err() { break; }
                }
                else => break,
            }
        }
    });

    while let Some(Ok(Message::Text(text))) = rx.next().await {
        let Ok(req) = serde_json::from_str::<RpcRequest>(&text) else {
            continue;
        };
        let resp = handle_rpc(&state, req).await;
        let _ = out_tx.send(ServerMsg::RpcDone(resp)).await;
    }
    forward.abort();
}

/// D5: push the intent repo to the offsite mirror, detached — a failing
/// push logs and is retried after the next operation (and by the periodic
/// tick), never blocking the operation that triggered it.
fn spawn_mirror_push(state: &AppState) {
    let Some(remote) = state.config.mirror_remote.clone() else {
        return;
    };
    let repo = format!("{}/repo", state.config.state_dir);
    tokio::spawn(async move {
        if let Err(e) = homelab_core::ops::mirror::mirror_push(&RealExecutor, &repo, &remote).await
        {
            tracing::warn!("mirror push failed (will retry): {}", e);
        }
    });
}

/// F3: best-effort webhook to Home Assistant after every mutating operation.
/// Runs through the executor (curl) so it is visible in traces and never
/// blocks or fails the operation itself.
async fn notify(
    state: &AppState,
    exec: &RealExecutor,
    label: &str,
    report: &homelab_core::runner::OperationReport,
) {
    let payload = serde_json::json!({
        "source": "homelab-host",
        "op": report.op,
        "label": label,
        "ok": report.ok,
        "error": report.error.as_ref().map(|e| format!("{} :: {}", e.what, e.why)),
    })
    .to_string();
    notify_raw(state, exec, payload).await;
}

/// Lower-level webhook POST used by notify() and the boot notification.
async fn notify_raw(state: &AppState, exec: &RealExecutor, payload: String) {
    let url = match state.settings.read().unwrap().notify_webhook.clone() {
        Some(u) => u,
        None => return,
    };
    let _ = exec
        .run(&Cmd::new(
            "curl",
            &[
                "-m",
                "5",
                "-s",
                "-o",
                "/dev/null",
                "-X",
                "POST",
                "-H",
                "Content-Type: application/json",
                "-d",
                &payload,
                &url,
            ],
            10,
        ))
        .await;
}

/// Run any mutating operation under the op-lock (AR12) with uniform incident
/// bundling on failure (AR14). The closure receives the OpCtx and returns the
/// OperationReport.
async fn run_mutating_op<F>(
    state: &AppState,
    exec: &RealExecutor,
    req_id: u64,
    label: &str,
    op: F,
) -> RpcResponse
where
    F: for<'a> FnOnce(
        &'a OpCtx<'a>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = homelab_core::runner::OperationReport> + Send + 'a>,
    >,
{
    let _guard = state.op_lock.lock().await;
    let broadcast = BroadcastSink {
        log_tx: state.log_tx.clone(),
    };
    let sink = homelab_core::incidents::RecordingSink::new(&broadcast);
    let journal = FileJournal {
        path: format!("{}/journal.jsonl", state.config.state_dir),
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let ctx = OpCtx {
        exec,
        sink: &sink,
        journal: &journal,
        safety: SafetyConfig::default(),
        state_dir: state.config.state_dir.clone(),
        now_unix: now,
    };
    let report = op(&ctx).await;
    notify(state, exec, label, &report).await; // F3, best-effort
    if report.ok {
        spawn_mirror_push(state); // D5, best-effort + detached
    }
    if report.ok {
        RpcResponse {
            id: req_id,
            ok: true,
            message: format!(
                "{} complete — {} step(s), {} changed",
                label,
                report.steps.len(),
                report.steps.iter().filter(|s| s.changed).count()
            ),
        }
    } else {
        let err = report
            .error
            .clone()
            .unwrap_or(homelab_core::error::OperatorError {
                what: format!("{} failed", label),
                why: "unknown".into(),
                remedy: "see transcript".into(),
            });
        error!("{} failed: {} — {}", label, err.what, err.why);
        let versions = format!("host={}\nproto={}\n", VERSION, homelab_proto::PROTO_VERSION);
        let bundle = homelab_core::incidents::write_bundle(
            exec,
            &state.config.state_dir,
            now,
            &report,
            &sink.events(),
            &versions,
        )
        .await;
        let bundle_note = match bundle {
            Ok(dir) => format!(" :: incident bundle {}", dir),
            Err(e) => format!(" :: (bundle write failed: {})", e),
        };
        RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "{} :: {} :: remedy: {}{}",
                err.what, err.why, err.remedy, bundle_note
            ),
        }
    }
}

async fn handle_rpc(state: &AppState, req: RpcRequest) -> RpcResponse {
    let exec = RealExecutor;
    match req.command {
        Rpc::Ping => RpcResponse {
            id: req.id,
            ok: true,
            message: "pong".into(),
        },
        Rpc::Status => {
            let out = exec.run(&Cmd::new("pct", &["list"], 30)).await;
            let listing = out.map(|o| o.stdout).unwrap_or_else(|e| e.to_string());
            let managed = exec
                .read_file(&format!("{}/state.json", state.config.state_dir))
                .await
                .unwrap_or_else(|_| "{}".into());
            RpcResponse {
                id: req.id,
                ok: true,
                message: format!("pct list:\n{}\nmanaged state:\n{}", listing, managed),
            }
        }
        Rpc::DeployStack(spec) => {
            run_mutating_op(state, &exec, req.id, "deploy", |ctx| {
                Box::pin(async move { deploy(ctx, &spec).await })
            })
            .await
        }
        Rpc::DestroyStack { manifest, confirm } => {
            run_mutating_op(state, &exec, req.id, "destroy", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::destroy::destroy(
                        ctx,
                        &manifest.stack_name,
                        manifest.vmid,
                        &confirm,
                    )
                    .await
                })
            })
            .await
        }
        Rpc::BackupStack(manifest) => {
            let cfg = homelab_core::ops::backup::BackupCfg {
                tiers: state.settings.read().unwrap().retention.clone(),
                ..Default::default()
            };
            run_mutating_op(state, &exec, req.id, "backup", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::backup::backup(ctx, &manifest, &cfg).await },
                )
            })
            .await
        }
        Rpc::RestoreStack { manifest, snapshot } => {
            run_mutating_op(state, &exec, req.id, "restore", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::backup::restore(
                        ctx,
                        &manifest,
                        &homelab_core::ops::backup::BackupCfg::default(),
                        &snapshot,
                    )
                    .await
                })
            })
            .await
        }
        Rpc::UpdateStack { manifest, app } => {
            run_mutating_op(state, &exec, req.id, "update", |ctx| {
                Box::pin(async move {
                    homelab_core::ops::update::update(ctx, &manifest, app.as_deref(), false).await
                })
            })
            .await
        }
        Rpc::PatchFleet => {
            // Targets come from state.json — only stacks we deployed.
            let store =
                homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
            let snapshot = store.load().await;
            let targets: Vec<(String, u16)> = snapshot
                .stacks
                .iter()
                .map(|(name, st)| (name.clone(), st.vmid))
                .collect();
            run_mutating_op(state, &exec, req.id, "patch", |ctx| {
                Box::pin(async move { homelab_core::ops::patch::patch_fleet(ctx, &targets).await })
            })
            .await
        }
        Rpc::BuildTemplate { temp_vmid, version } => {
            run_mutating_op(state, &exec, req.id, "template-build", |ctx| {
                Box::pin(async move {
                    let cfg = homelab_core::ops::template::TemplateCfg {
                        temp_vmid,
                        version,
                        ..Default::default()
                    };
                    homelab_core::ops::template::build_template(ctx, &cfg).await
                })
            })
            .await
        }
        Rpc::ExecIn { vmid, command } => {
            if let Err(e) = homelab_core::safety::exec_guard(
                state.config.exec_enabled,
                &SafetyConfig::default(),
                vmid,
            ) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                };
            }
            // A6: audit every invocation before running it.
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let audit = format!("{} exec vmid={} cmd={:?}\n", ts, vmid, command);
            let audit_path = format!("{}/audit.log", state.config.state_dir);
            let _ = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&audit_path)
                .and_then(|mut f| {
                    use std::io::Write as _;
                    f.write_all(audit.as_bytes())
                });
            info!("A6 exec vmid={} cmd={:?}", vmid, command);
            match homelab_core::executor::pct_sh(&exec, vmid, &command, 120).await {
                Ok(out) => RpcResponse {
                    id: req.id,
                    ok: out.success(),
                    message: format!(
                        "exit {}\n{}{}",
                        out.code,
                        out.stdout,
                        if out.stderr.is_empty() {
                            String::new()
                        } else {
                            format!("--- stderr ---\n{}", out.stderr)
                        }
                    ),
                },
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{}", e),
                },
            }
        }
        Rpc::GetConfig => {
            let view = state.settings.read().unwrap().clone();
            let _ = state.log_tx.send(ServerMsg::Config(Box::new(view)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "config".into(),
            }
        }
        Rpc::SetConfig(view) => {
            // Validate before persisting.
            if let Some(h) = view.backup_hour {
                if h > 23 {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: "backup_hour must be 0-23".into(),
                    };
                }
            }
            if view.retention.is_empty() {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: "retention needs at least one tier".into(),
                };
            }
            match persist_settings(&state.config, &view) {
                Ok(()) => {
                    *state.settings.write().unwrap() = *view;
                    info!("settings updated via G8");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: "settings saved and applied".into(),
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("persist settings: {}", e),
                },
            }
        }
        Rpc::SelfUpdateHost { binary_b64 } => {
            use base64::Engine as _;
            let bytes = match base64::engine::general_purpose::STANDARD.decode(&binary_b64) {
                Ok(b) => b,
                Err(e) => {
                    return RpcResponse {
                        id: req.id,
                        ok: false,
                        message: format!("bad binary payload: {}", e),
                    }
                }
            };
            let cfg = homelab_core::ops::selfupdate::SelfUpdateCfg::default();
            // Stage outside the op so the (large) write is done before the
            // op-lock is taken. Raw bytes, not write_file (which is text).
            if let Err(e) = std::fs::write(&cfg.staged, &bytes).and_then(|()| {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&cfg.staged, std::fs::Permissions::from_mode(0o755))
            }) {
                return RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("stage binary: {}", e),
                };
            }
            info!("self-update requested: staged {} bytes", bytes.len());
            run_mutating_op(state, &exec, req.id, "self-update", |ctx| {
                Box::pin(async move { homelab_core::ops::selfupdate::self_update(ctx, &cfg).await })
            })
            .await
        }
        Rpc::Doctor => {
            let probes = gather_probes(&exec, &state.config.state_dir).await;
            let checks = homelab_core::doctor::diagnose(&probes);
            let overall = homelab_core::doctor::overall(&checks);
            let mut msg = format!("doctor: {:?}\n", overall);
            for c in &checks {
                msg.push_str(&format!("  [{:?}] {} — {}\n", c.health, c.name, c.detail));
                if let Some(r) = &c.remedy {
                    msg.push_str(&format!("        ↳ {}\n", r));
                }
            }
            RpcResponse {
                id: req.id,
                ok: overall != homelab_core::doctor::Health::Fail,
                message: msg,
            }
        }
        Rpc::GetState => {
            // For M2 this reads recorded state; live per-app health arrives
            // when the verify step persists it (wired in M4).
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let hs = store.load().await;
            let df = exec
                .run(&Cmd::new(
                    "df",
                    &["--output=pcent", &state.config.state_dir],
                    20,
                ))
                .await
                .ok()
                .and_then(|o| {
                    o.stdout
                        .lines()
                        .nth(1)
                        .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                })
                .unwrap_or(0);
            let (_, fingerprint) = tls::ensure_cert(&state.config.state_dir, "homelab-host")
                .unwrap_or((
                    tls::CertPaths {
                        cert_pem: String::new(),
                        key_pem: String::new(),
                    },
                    "unknown".into(),
                ));
            let stacks = hs
                .stacks
                .values()
                .map(|s| homelab_proto::StackView {
                    name: s
                        .hostname
                        .rsplit("-app-")
                        .next()
                        .unwrap_or(&s.hostname)
                        .to_string(),
                    vmid: s.vmid,
                    hostname: s.hostname.clone(),
                    apps: s
                        .apps
                        .iter()
                        .map(|a| homelab_proto::AppView {
                            name: a.clone(),
                            running: true,
                            restarts: 0,
                        })
                        .collect(),
                    drift: false, // computed client-side from applied_hash
                    applied_hash: s.applied_hash.clone(),
                    env_sealed: true,
                    online: true,
                })
                .collect();
            let fleet = homelab_proto::FleetState {
                host: homelab_proto::HostView {
                    name: "pve-01".into(),
                    cpu_pct: 0,
                    ram_pct: 0,
                    disk_pct: df,
                    tls_fingerprint: fingerprint,
                    // Capacity totals wired when state carries resources (M4).
                    ram_total_mb: 0,
                    ram_used_mb: 0,
                    ram_committed_mb: 0,
                    cores_total: 0,
                    load1_x100: 0,
                },
                stacks,
            };
            let _ = state.log_tx.send(ServerMsg::State(Box::new(fleet)));
            RpcResponse {
                id: req.id,
                ok: true,
                message: "state".into(),
            }
        }
        Rpc::Incidents => {
            let dir = format!("{}/incidents", state.config.state_dir);
            let list = std::fs::read_dir(&dir)
                .map(|rd| {
                    let mut names: Vec<String> = rd
                        .flatten()
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect();
                    names.sort();
                    names
                })
                .unwrap_or_default();
            RpcResponse {
                id: req.id,
                ok: true,
                message: if list.is_empty() {
                    "no incidents recorded".into()
                } else {
                    format!("incidents:\n  {}", list.join("\n  "))
                },
            }
        }
    }
}

/// Gather doctor probes (F6). I/O stays here; the verdict logic is in core.
async fn gather_probes(exec: &RealExecutor, state_dir: &str) -> homelab_core::doctor::Probes {
    use homelab_core::doctor::Probes;
    let state_raw = exec.read_file(&format!("{}/state.json", state_dir)).await;
    let state_parses = state_raw
        .as_ref()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).is_ok())
        .unwrap_or(true);
    let interrupted = std::fs::read_to_string(format!("{}/journal.jsonl", state_dir))
        .map(|j| {
            homelab_core::incidents::interrupted_ops(&j)
                .into_iter()
                .map(|(op, _)| op)
                .collect()
        })
        .unwrap_or_default();
    // Host disk free % via df on the state dir.
    let disk = exec
        .run(&Cmd::new("df", &["--output=pcent", state_dir], 20))
        .await
        .ok()
        .and_then(|o| {
            o.stdout
                .lines()
                .nth(1)
                .and_then(|l| l.trim().trim_end_matches('%').parse::<u64>().ok())
                .map(|used| 100u64.saturating_sub(used))
        });
    Probes {
        host_disk_free_pct: disk,
        state_parses,
        managed_stacks: Vec::new(), // populated once state carries backup ages
        offsite_configured: false,
        offsite_token_valid: false,
        mirror_behind: None,
        interrupted_ops: interrupted,
    }
}
