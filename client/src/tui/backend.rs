//! Backend trait (AR6): the TUI talks to "something that streams ServerMsg and
//! accepts Commands". Production is the real wss+pinned connection; tests use a
//! scripted TestBackend. The TUI cannot tell them apart.

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use homelab_proto::{Command, LogLevel, RpcRequest, ServerMsg};

/// What the TUI receives from whatever it's connected to.
#[derive(Debug, Clone)]
pub enum BackendEvent {
    Connected {
        version: String,
        fingerprint: Option<String>,
    },
    Server(ServerMsg),
    Disconnected(String),
}

pub struct Channels {
    pub cmd_tx: mpsc::Sender<Command>,
    pub evt_rx: mpsc::Receiver<BackendEvent>,
}

pub trait Backend {
    /// Spawn the backend's own task and return channels to talk to it.
    fn start(self: Box<Self>) -> Channels;
}

// ── Remote (real wss + pinning) ─────────────────────────────────────────────

pub struct RemoteBackend {
    pub host: String,
    pub token: String,
    /// The fingerprint `config/client.toml` names, when there is one.
    pub repo_pin: Option<String>,
    /// fix-149: the fingerprint this client was built with
    /// (`repo_config::built_in_pin`); `None` only in tests with a
    /// certificate of their own.
    pub built_in_pin: Option<String>,
}

impl Backend for RemoteBackend {
    fn start(self: Box<Self>) -> Channels {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<BackendEvent>(256);

        tokio::spawn(async move {
            // fix-67 (tui-connection-skips-guards, 2026-09-27): the same
            // connect as the command line — the repository's pin, the fix-30
            // frame ceiling — instead of a copy that had drifted from it.
            let link = match crate::link::connect(
                &self.host,
                &self.token,
                self.repo_pin.as_deref(),
                self.built_in_pin.as_deref(),
            )
            .await
            {
                Ok(l) => l,
                Err(e) => {
                    let _ = evt_tx.send(BackendEvent::Disconnected(e)).await;
                    return;
                }
            };
            let fingerprint = link.fingerprint.clone();
            let (mut tx, mut rx) = link.ws.split();

            // The version gate needs the host's version before any command
            // goes out, so nothing is sent until the Hello has been read; the
            // command line waits for it the same way.
            let host_version = loop {
                match rx.next().await {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(sm) = serde_json::from_str::<ServerMsg>(&t) {
                            if let ServerMsg::Hello { version, .. } = &sm {
                                let version = version.clone();
                                let _ = evt_tx
                                    .send(BackendEvent::Connected {
                                        version: version.clone(),
                                        fingerprint,
                                    })
                                    .await;
                                let _ = evt_tx.send(BackendEvent::Server(sm)).await;
                                break version;
                            }
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        let _ = evt_tx
                            .send(BackendEvent::Disconnected(format!("ws: {}", e)))
                            .await;
                        return;
                    }
                    None => {
                        let _ = evt_tx
                            .send(BackendEvent::Disconnected(
                                "closed before the host said hello".into(),
                            ))
                            .await;
                        return;
                    }
                }
            };

            let mut req_id = 1u64;
            // fix-66: requests that were answers. The host now reads an answer
            // while the operation that asked is still running, so its reply
            // arrives first; the model pairs replies with requests by order,
            // and would take it for the operation's own end.
            let mut answers: std::collections::HashSet<u64> = Default::default();
            loop {
                tokio::select! {
                    Some(cmd) = cmd_rx.recv() => {
                        req_id += 1;
                        // The refusal comes back the way a failed operation
                        // does, so it lands where the operator is looking.
                        // fix-105: and the other way round, a client older
                        // than the host.
                        if let Some(why) = crate::link::refuse_older_host(&cmd, &host_version)
                            .or_else(|| crate::link::refuse_older_client(&cmd, &host_version))
                        {
                            let refused = ServerMsg::RpcDone(homelab_proto::RpcResponse {
                                id: req_id,
                                ok: false,
                                message: why,
                                deferred: None,
                            });
                            if evt_tx.send(BackendEvent::Server(refused)).await.is_err() { break; }
                            continue;
                        }
                        if matches!(cmd, Command::Answer { .. }) {
                            answers.insert(req_id);
                        }
                        let req = RpcRequest { id: req_id, command: cmd };
                        if tx.send(Message::Text(serde_json::to_string(&req).unwrap().into())).await.is_err() {
                            let _ = evt_tx.send(BackendEvent::Disconnected("send failed".into())).await;
                            break;
                        }
                    }
                    msg = rx.next() => {
                        match msg {
                            Some(Ok(Message::Text(t))) => {
                                if let Ok(sm) = serde_json::from_str::<ServerMsg>(&t) {
                                    let sm = match sm {
                                        ServerMsg::RpcDone(r) if answers.remove(&r.id) => ServerMsg::Log {
                                            level: if r.ok { LogLevel::Info } else { LogLevel::Warn },
                                            source: "HOST".into(),
                                            msg: r.message,
                                        },
                                        other => other,
                                    };
                                    if evt_tx.send(BackendEvent::Server(sm)).await.is_err() { break; }
                                }
                            }
                            Some(Ok(Message::Close(_))) | None => {
                                let _ = evt_tx.send(BackendEvent::Disconnected("closed".into())).await;
                                break;
                            }
                            Some(Err(e)) => {
                                let _ = evt_tx.send(BackendEvent::Disconnected(format!("ws: {}", e))).await;
                                break;
                            }
                            _ => {}
                        }
                    }
                }
            }
        });

        Channels { cmd_tx, evt_rx }
    }
}

// ── Test backend (scripted, no network) ─────────────────────────────────────

/// Emits a scripted sequence of events; used by snapshot tests and the
/// `--offline` demo path. Every command it receives is acknowledged.
pub struct TestBackend {
    pub script: Vec<BackendEvent>,
}

impl Backend for TestBackend {
    fn start(self: Box<Self>) -> Channels {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<BackendEvent>(256);
        tokio::spawn(async move {
            for ev in self.script {
                if evt_tx.send(ev).await.is_err() {
                    return;
                }
            }
            // Keep draining commands so senders don't error.
            while cmd_rx.recv().await.is_some() {}
        });
        Channels { cmd_tx, evt_rx }
    }
}

// ── Demo backend (offline showcase) ─────────────────────────────────────────

/// A self-contained fake host for `homelab tui --offline`: serves a plausible
/// fleet on GetState, replies to Doctor, and plays a scripted deploy (with
/// transfers) when DeployStack arrives. No network, no real infra — lets Kenny
/// experience the full TUI before the host is live.
pub struct DemoBackend;

impl Backend for DemoBackend {
    fn start(self: Box<Self>) -> Channels {
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<Command>(32);
        let (evt_tx, evt_rx) = mpsc::channel::<BackendEvent>(256);

        tokio::spawn(async move {
            let _ = evt_tx
                .send(BackendEvent::Connected {
                    version: "2.0.0-demo".into(),
                    fingerprint: Some("9F:2A:DE:M0:…".into()),
                })
                .await;
            let _ = evt_tx.send(BackendEvent::Server(demo_fleet())).await;

            // Ambient log chatter + command handling.
            let mut ticker = tokio::time::interval(std::time::Duration::from_millis(900));
            let mut n = 0u64;
            loop {
                tokio::select! {
                    _ = ticker.tick() => {
                        n += 1;
                        let (src, msg) = demo_log(n);
                        let _ = evt_tx.send(BackendEvent::Server(ServerMsg::Log {
                            level: homelab_proto::LogLevel::Debug,
                            source: src.into(),
                            msg: msg.into(),
                        })).await;
                    }
                    cmd = cmd_rx.recv() => {
                        match cmd {
                            None => break,
                            Some(Command::GetState) => {
                                let _ = evt_tx.send(BackendEvent::Server(demo_fleet())).await;
                            }
                            Some(Command::Doctor) => {
                                let _ = evt_tx.send(BackendEvent::Server(ServerMsg::RpcDone(
                                    homelab_proto::RpcResponse {
                                        id: 0,
                                        ok: true,
                                        message: DEMO_DOCTOR.into(),
                                        deferred: None,
                                    },
                                ))).await;
                            }
                            Some(Command::DeployStack(spec)) => {
                                play_demo_deploy(&evt_tx, &spec.manifest.stack_name).await;
                            }
                            Some(Command::Today { .. }) => {
                                let today = homelab_core::ops::today::Today::default();
                                let _ = evt_tx.send(BackendEvent::Server(ServerMsg::RpcDone(
                                    homelab_proto::RpcResponse {
                                        id: 0,
                                        ok: true,
                                        message: serde_json::to_string(&today).unwrap_or_default(),
                                        deferred: None,
                                    },
                                ))).await;
                            }
                            Some(Command::GetConfig) => {
                                let _ = evt_tx
                                    .send(BackendEvent::Server(ServerMsg::Config(Box::new(
                                        homelab_proto::HostConfigView {
                                            backup_hour: Some(4),
                                            notify_webhook: None,
                                            retention:
                                                homelab_core::retention::default_tiers(),
                                        },
                                    ))))
                                    .await;
                            }
                            Some(_) => {
                                let _ = evt_tx.send(BackendEvent::Server(ServerMsg::RpcDone(
                                    homelab_proto::RpcResponse {
                                        id: 0,
                                        ok: true,
                                        message: "ok (demo)".into(),
                                        deferred: None,
                                    },
                                ))).await;
                            }
                        }
                    }
                }
            }
        });

        Channels { cmd_tx, evt_rx }
    }
}

fn demo_fleet() -> ServerMsg {
    use homelab_proto::{AppView, FleetState, HostView, StackView};
    let app = |n: &str, run: bool| AppView {
        name: n.into(),
        running: run,
        restarts: 0,
    };
    ServerMsg::State(Box::new(FleetState {
        host: HostView {
            name: "pve-01".into(),
            cpu_pct: 18,
            ram_pct: 68,
            disk_pct: 42,
            tls_fingerprint: "9F:2A:C4:1E:AB:CD:EF:01".into(),
            ram_total_mb: 31744,
            ram_used_mb: 12680,      // actual usage — the real headroom
            ram_committed_mb: 38400, // sum of ceilings > total (normal for LXC)
            cores_total: 12,
            load1_x100: 285, // load average 2.85
        },
        stacks: vec![
            StackView {
                name: "platform".into(),
                vmid: 104,
                hostname: "104-app-platform".into(),
                apps: vec![
                    app("traefik", true),
                    app("loki", true),
                    app("grafana", true),
                    app("crowdsec", true),
                ],
                drift: false,
                applied_hash: String::new(),
                env_sealed: true,
                enabled: true,
                online: true,
            },
            StackView {
                name: "media".into(),
                vmid: 106,
                hostname: "106-app-media".into(),
                apps: vec![
                    app("jellyfin", true),
                    app("sonarr", true),
                    app("radarr", false),
                ],
                drift: true,
                applied_hash: String::new(),
                env_sealed: true,
                enabled: true,
                online: true,
            },
            StackView {
                name: "syncthing".into(),
                vmid: 110,
                hostname: "110-app-syncthing".into(),
                apps: vec![app("syncthing", true)],
                drift: false,
                applied_hash: String::new(),
                env_sealed: true,
                enabled: true,
                online: true,
            },
        ],
    }))
}

const DEMO_DOCTOR: &str = "doctor: Warn\n  [Ok] host disk — 42% free\n  [Ok] state file — parses\n  [Warn] stack media backup — last backup 51h ago\n        ↳ run a backup; the scheduler may be stalled\n  [Ok] offsite (Drive) — token valid\n  [Ok] github mirror — up to date";

fn demo_log(n: u64) -> (&'static str, &'static str) {
    const LINES: &[(&str, &str)] = &[
        ("platform", "traefik :: 200 GET jellyfin.kp-soft.dev 12ms"),
        ("media", "sonarr :: rss sync complete, 0 new"),
        (
            "syncthing",
            "syncthing :: folder \"obsidian-vault\" in sync",
        ),
        (
            "HOST",
            "heartbeat :: CLIENT fresh — failsafe window skipped",
        ),
        (
            "platform",
            "crowdsec :: ip 185.42.1.9 banned (http-probing)",
        ),
        ("media", "jellyfin :: transcode session started (vaapi)"),
    ];
    LINES[(n as usize) % LINES.len()]
}

async fn play_demo_deploy(evt_tx: &mpsc::Sender<BackendEvent>, stack: &str) {
    let steps = [
        "validate",
        "safety gates",
        "provision container",
        "bootstrap docker",
        "runaway guards",
        "push files",
        "start apps",
        "verify health",
    ];
    let log = |src: &str, msg: String| {
        BackendEvent::Server(ServerMsg::Log {
            level: homelab_proto::LogLevel::Info,
            source: src.to_string(),
            msg,
        })
    };
    for step in steps.iter() {
        let _ = evt_tx
            .send(log(
                "HOST",
                format!("[sync][run ] deploy-{} :: {}", stack, step),
            ))
            .await;
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        if *step == "push files" {
            for b in [4096u64, 12288, 20480, 24576] {
                let _ = evt_tx
                    .send(BackendEvent::Server(ServerMsg::Transfer {
                        op: format!("deploy-{}", stack),
                        label: format!("{}/docker-compose.yml", stack),
                        done: b,
                        total: Some(24576),
                    }))
                    .await;
                tokio::time::sleep(std::time::Duration::from_millis(180)).await;
            }
        }
        let _ = evt_tx
            .send(log("HOST", format!("[sync][exit] {} :: ok", step)))
            .await;
    }
    let _ = evt_tx
        .send(log(
            "HOST",
            "[sync] Sync complete — verified healthy".into(),
        ))
        .await;
    let _ = evt_tx
        .send(BackendEvent::Server(ServerMsg::RpcDone(
            homelab_proto::RpcResponse {
                id: 0,
                ok: true,
                message: format!("Sync complete — {} deployed (demo)", stack),
                deferred: None,
            },
        )))
        .await;
}
