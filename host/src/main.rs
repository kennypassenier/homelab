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

const VERSION: &str = env!("CARGO_PKG_VERSION");

// ── Config (AR11) ────────────────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize, Default)]
struct FileConfig {
    token: Option<String>,
    listen: Option<String>,
    state_dir: Option<String>,
}

#[derive(Clone)]
struct Config {
    token: String,
    listen: SocketAddr,
    state_dir: String,
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
    }
}

// ── Real executor (AR2) ─────────────────────────────────────────────────────

struct RealExecutor {
    log_tx: broadcast::Sender<ServerMsg>,
}

impl RealExecutor {
    fn debug(&self, msg: String) {
        tracing::debug!("{}", msg);
        let _ = self.log_tx.send(ServerMsg::Log {
            level: homelab_proto::LogLevel::Debug,
            source: "HOST".into(),
            msg,
        });
    }
}

#[async_trait]
impl Executor for RealExecutor {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        let rendered = cmd.rendered();
        self.debug(format!("[run ] {}", rendered));
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
        for line in stdout.lines().chain(stderr.lines()).take(40) {
            if !line.trim().is_empty() {
                self.debug(format!("  {}", line));
            }
        }
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
}

#[tokio::main]
async fn main() {
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
    };

    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/version", get(|| async { VERSION }))
        .route("/api/ws", get(ws_upgrade))
        .with_state(state);

    info!("homelab-host v{} listening on {}", VERSION, config.listen);
    let listener = tokio::net::TcpListener::bind(config.listen)
        .await
        .expect("bind");
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

async fn handle_rpc(state: &AppState, req: RpcRequest) -> RpcResponse {
    let exec = RealExecutor {
        log_tx: state.log_tx.clone(),
    };
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
            let _guard = state.op_lock.lock().await; // AR12
            let sink = BroadcastSink {
                log_tx: state.log_tx.clone(),
            };
            let journal = FileJournal {
                path: format!("{}/journal.jsonl", state.config.state_dir),
            };
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let ctx = OpCtx {
                exec: &exec,
                sink: &sink,
                journal: &journal,
                safety: SafetyConfig::default(),
                state_dir: state.config.state_dir.clone(),
                now_unix: now,
            };
            let report = deploy(&ctx, &spec).await;
            if report.ok {
                RpcResponse {
                    id: req.id,
                    ok: true,
                    message: format!(
                        "Sync complete — {} step(s), {} changed",
                        report.steps.len(),
                        report.steps.iter().filter(|s| s.changed).count()
                    ),
                }
            } else {
                let err = report.error.unwrap_or(homelab_core::error::OperatorError {
                    what: "deploy failed".into(),
                    why: "unknown".into(),
                    remedy: "see transcript".into(),
                });
                error!("deploy failed: {} — {}", err.what, err.why);
                RpcResponse {
                    id: req.id,
                    ok: false,
                    message: format!("{} :: {} :: remedy: {}", err.what, err.why, err.remedy),
                }
            }
        }
    }
}
