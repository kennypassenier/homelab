//! Homelab CLIENT — thin CLI over the CLIENT↔HOST protocol.
//! (The cyberpunk TUI plugs into the same protocol; this is the scriptable
//! interface and the pilot-phase driver.)
//!
//! Usage:
//!   homelab ping
//!   homelab status
//!   homelab deploy stacks/<name>
//!
//! Config via env: HOMELAB_HOST (host:port), HOMELAB_TOKEN.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use homelab_proto::{
    Command, DeploySpec, FileBlob, GatewayRoute, LogLevel, RpcRequest, ServerMsg, StackManifest,
};

const C_RESET: &str = "\x1b[0m";
const C_CYAN: &str = "\x1b[36m";
const C_GREEN: &str = "\x1b[32m";
const C_YELLOW: &str = "\x1b[33m";
const C_RED: &str = "\x1b[31m";
const C_DIM: &str = "\x1b[2m";

#[derive(Deserialize)]
struct StackFile {
    #[serde(flatten)]
    manifest: StackManifest,
    #[serde(default)]
    gateway_route: Option<GatewayRouteFile>,
}

#[derive(Deserialize)]
struct GatewayRouteFile {
    filename: String,
    #[serde(default = "default_gw")]
    gateway_vmid: u16,
}

fn default_gw() -> u16 {
    104
}

fn die(msg: &str) -> ! {
    eprintln!("{}error:{} {}", C_RED, C_RESET, msg);
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let host = std::env::var("HOMELAB_HOST").unwrap_or_else(|_| "10.10.5.250:8443".into());
    let token = std::env::var("HOMELAB_TOKEN").unwrap_or_default();
    if token.is_empty() && cmd != "help" {
        die("HOMELAB_TOKEN is not set");
    }

    match cmd {
        "ping" => rpc(&host, &token, Command::Ping).await,
        "status" => rpc(&host, &token, Command::Status).await,
        "deploy" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab deploy stacks/<name>"));
            let spec = build_spec(Path::new(dir));
            println!(
                "{}▶ deploy {} :: vmid {} :: {} file(s), {} env(s){}",
                C_CYAN,
                spec.manifest.stack_name,
                spec.manifest.vmid,
                spec.files.len(),
                spec.env.len(),
                C_RESET
            );
            rpc(&host, &token, Command::DeployStack(Box::new(spec))).await;
        }
        _ => {
            println!("homelab v{} — usage:", env!("CARGO_PKG_VERSION"));
            println!("  homelab ping|status");
            println!("  homelab deploy stacks/<name>");
            println!("env: HOMELAB_HOST (default 10.10.5.250:8443), HOMELAB_TOKEN");
        }
    }
}

fn build_spec(dir: &Path) -> DeploySpec {
    let manifest_path = dir.join("lxc-compose.yml");
    let raw = std::fs::read_to_string(&manifest_path)
        .unwrap_or_else(|e| die(&format!("cannot read {}: {}", manifest_path.display(), e)));
    let stack_file: StackFile =
        serde_yaml::from_str(&raw).unwrap_or_else(|e| die(&format!("manifest parse: {}", e)));

    let mut files: Vec<FileBlob> = Vec::new();
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    collect(dir, dir, &mut files, &mut env);

    let gateway_route = stack_file.gateway_route.as_ref().map(|g| {
        let route_path = dir.join("traefik-routes.yml");
        let content = std::fs::read_to_string(&route_path).unwrap_or_else(|e| {
            die(&format!(
                "gateway_route set but {}: {}",
                route_path.display(),
                e
            ))
        });
        GatewayRoute {
            gateway_vmid: g.gateway_vmid,
            filename: g.filename.clone(),
            content,
        }
    });

    DeploySpec {
        manifest: stack_file.manifest,
        files,
        env,
        gateway_route,
    }
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<FileBlob>, env: &mut BTreeMap<String, String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            collect(root, &path, files, env);
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .to_string();
        // Top-level control files are not payload.
        if rel == "lxc-compose.yml" || rel == "traefik-routes.yml" {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            die(&format!("non-utf8 file not supported: {}", rel))
        };
        if name == ".env" {
            // stacks/<name>/<app>/.env → secrets channel, never a repo file.
            let app = path
                .parent()
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            env.insert(app, content);
        } else {
            files.push(FileBlob {
                path: rel,
                content,
                mode: None,
            });
        }
    }
}

async fn rpc(host: &str, token: &str, command: Command) {
    let url = format!("ws://{}/api/ws", host);
    let mut request = url
        .clone()
        .into_client_request()
        .unwrap_or_else(|e| die(&format!("bad url {}: {}", url, e)));
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );

    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .unwrap_or_else(|e| die(&format!("connect {}: {}", url, e)));
    let (mut tx, mut rx) = ws.split();

    let req = RpcRequest { id: 1, command };
    tx.send(Message::Text(serde_json::to_string(&req).unwrap()))
        .await
        .unwrap_or_else(|e| die(&format!("send: {}", e)));

    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(server_msg) = serde_json::from_str::<ServerMsg>(&text) else {
            continue;
        };
        match server_msg {
            ServerMsg::Hello { version, proto } => {
                println!(
                    "{}● HOST v{} (proto {}) — link up{}",
                    C_GREEN, version, proto, C_RESET
                );
            }
            ServerMsg::Log { level, source, msg } => {
                let color = match level {
                    LogLevel::Debug => C_DIM,
                    LogLevel::Info => C_CYAN,
                    LogLevel::Warn => C_YELLOW,
                    LogLevel::Error => C_RED,
                };
                println!("{}{:<5}{} {}", color, source, C_RESET, msg);
            }
            ServerMsg::Transfer {
                label, done, total, ..
            } => {
                let total_str = total.map(|t| format!("/{}", t)).unwrap_or_default();
                println!(
                    "{}⇅ {} {}{} bytes{}",
                    C_DIM, label, done, total_str, C_RESET
                );
            }
            ServerMsg::RpcDone(resp) => {
                if resp.ok {
                    println!("{}✓ {}{}", C_GREEN, resp.message, C_RESET);
                    std::process::exit(0);
                } else {
                    println!("{}✗ {}{}", C_RED, resp.message, C_RESET);
                    std::process::exit(1);
                }
            }
        }
    }
    die("connection closed before RPC completed");
}
