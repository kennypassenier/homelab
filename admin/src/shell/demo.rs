//! feat-platform-10: the demo host, only in a build with the `demo-host`
//! feature (Kenny, 2026-09-28: "Only in test builds"). `make release` builds
//! without it.

use std::time::Duration;

use chassis::shell::live::Live;
use homelab_proto::{Command, RpcResponse, ServerMsg};

use super::host_link::{Asked, HostAsks, Shared, now_s};
use crate::core::fleet::fleet_view;

/// The version the demo host says Hello with: the release every gate in
/// this dashboard asks for, so every page works against it.
pub const DEMO_VERSION: &str = "3.63.0";

/// feat-platform-10: a host that lives in this process, for the browser
/// tests (`HOMELAB_ADMIN_DEMO_HOST=1`). The real line is never opened: the
/// fleet is the stacks named in `stacks` as the working copy describes
/// them, reads answer empty, and every command that would change something
/// only prints three step lines and answers "complete". Nothing leaves the
/// machine.
pub async fn run_demo(
    shared: Shared,
    live: Live,
    mut asks: HostAsks,
    repo: std::path::PathBuf,
    stacks: Vec<String>,
) {
    use homelab_proto::{AppView, FleetState, HostView, LogLevel, StackView, StepMark};
    // Rebuilt on every tick: the working copy is cloned beside this start.
    let build = || FleetState {
        host: HostView {
            name: "demo".into(),
            cpu_pct: Some(7),
            ram_pct: 40,
            disk_pct: 31,
            tls_fingerprint: String::new(),
            ram_total_mb: 65536,
            ram_used_mb: 26000,
            ram_committed_mb: 30000,
            cores_total: 16,
            load1_x100: 80,
            home_address: None,
        },
        stacks: stacks
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let m = homelab_client::spec::build_manifest(&repo.join("stacks").join(name)).ok();
                StackView {
                    name: name.clone(),
                    vmid: m.as_ref().map(|m| m.vmid).unwrap_or(900 + i as u16),
                    hostname: m
                        .as_ref()
                        .map(|m| m.hostname.clone())
                        .unwrap_or_else(|| name.clone()),
                    apps: m
                        .map(|m| m.apps)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|a| AppView {
                            name: a,
                            running: true,
                            restarts: 0,
                        })
                        .collect(),
                    drift: false,
                    applied_hash: String::new(),
                    env_sealed: false,
                    online: true,
                    enabled: true,
                    usage: None,
                    applied_source: None,
                }
            })
            .collect(),
        status_measured_at: Some(now_s()),
    };
    {
        let mut s = shared.write().await;
        s.fleet = Some(fleet_view(&build(), now_s()));
        s.host_version = Some(DEMO_VERSION.into());
        s.host_build = Some("demo host in homelab-admin".into());
        s.link_error = None;
    }
    tracing::warn!(
        "HOMELAB_ADMIN_DEMO_HOST is set: a demo host answers; no real host is contacted"
    );
    let _ = live.publish("link", &serde_json::json!({ "up": true, "host_version": DEMO_VERSION, "host_build": "demo host in homelab-admin" }));
    let mut next: u64 = now_s() << 20;
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            Some(Asked { command, reply, sent }) = asks.rx.recv() => {
                let id = next;
                next += 1;
                if let Some(s) = sent {
                    let _ = s.send(id);
                }
                let answer = move |message: String| RpcResponse { id, ok: true, message, deferred: None };
                if command.is_read_only() {
                    let body = match command {
                        Command::History { .. } => serde_json::json!({ "entries": [] }).to_string(),
                        Command::Incidents { .. } => serde_json::json!({ "incidents": [] }).to_string(),
                        Command::Notices { .. } => serde_json::json!({ "notices": [], "last_seq": 0 }).to_string(),
                        Command::CurrentOp => serde_json::to_string(&homelab_proto::CurrentOpView::default()).unwrap_or_default(),
                        // The TUI parity pages' reads, in the shapes the real
                        // host answers (empty, or plainly made up and said so).
                        Command::Today { .. } => serde_json::json!({ "items": [{
                            "level": "Attention", "source": "check",
                            "what": "demo host: one made-up item so the list has a row",
                            "remedy": "nothing; this is the demo host",
                        }], "unread": [] }).to_string(),
                        Command::FleetCheck { .. } => serde_json::json!({ "passes": true, "findings": [] }).to_string(),
                        Command::ListManualChecks { .. } => serde_json::json!({ "now": now_s(), "checks": [] }).to_string(),
                        Command::ListTemplates => "clonable golden templates (fast):\nclone:996  debian-13-homelab-v4\n\nOS templates (full bootstrap):\n  local:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst\n".into(),
                        Command::GetApplied { .. } => "[]".into(),
                        Command::Ping => "pong (demo host)".into(),
                        Command::GetHostConfig => serde_json::to_string(&homelab_proto::HostConfigFile {
                            path: "/etc/homelab/host.toml (demo host)".into(),
                            ..Default::default()
                        }).unwrap_or_default(),
                        _ => "{}".into(),
                    };
                    let _ = reply.send(Ok(answer(body)));
                    continue;
                }
                let events = asks.events.clone();
                let name = command.name().to_string();
                tokio::spawn(async move {
                    let op = format!("{}-demo", name.split('_').next().unwrap_or("op"));
                    for step in ["prepare", "apply", "verify"] {
                        for finished in [false, true] {
                            let _ = events.send(ServerMsg::Log {
                                level: LogLevel::Info,
                                source: "DEMO".into(),
                                msg: format!("[{op}] {step} {}", if finished { "done" } else { "started" }),
                                req: Some(id),
                                ts: Some(now_s()),
                                step: Some(StepMark { op: op.clone(), step: step.into(), finished, changed: finished, skipped: false }),
                                plan: None,
                                by: Some("admin".into()),
                            });
                            tokio::time::sleep(Duration::from_millis(600)).await;
                        }
                    }
                    let _ = reply.send(Ok(answer(format!("{name} complete (demo host: nothing ran)"))));
                });
            }
            _ = tick.tick() => {
                let f = fleet_view(&build(), now_s());
                shared.write().await.fleet = Some(f.clone());
                let _ = live.publish("fleet", &serde_json::json!({ "changed": false, "fleet": f }));
            }
        }
    }
}
