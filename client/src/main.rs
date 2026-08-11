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

use std::path::Path;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::Connector;

use homelab_proto::{Command, LogLevel, RpcRequest, ServerMsg};

use homelab_client::{load_pin, save_pin, spec, tui};

const C_RESET: &str = "\x1b[0m";
const C_CYAN: &str = "\x1b[36m";
const C_GREEN: &str = "\x1b[32m";
const C_YELLOW: &str = "\x1b[33m";
const C_RED: &str = "\x1b[31m";
const C_DIM: &str = "\x1b[2m";

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
    let offline = args.iter().any(|a| a == "--offline" || a == "--demo");
    // The offline demo TUI needs neither host nor token.
    if token.is_empty() && cmd != "help" && !(cmd == "tui" && offline) {
        die("HOMELAB_TOKEN is not set");
    }

    match cmd {
        // `homelab tui` launches the control deck; `--offline` uses a fake host.
        "tui" => {
            let backend: Box<dyn tui::backend::Backend> = if offline {
                Box::new(tui::backend::DemoBackend)
            } else {
                Box::new(tui::backend::RemoteBackend {
                    host: host.clone(),
                    token: token.clone(),
                })
            };
            if let Err(e) = tui::run(backend).await {
                die(&format!("tui: {}", e));
            }
        }
        "ping" => rpc(&host, &token, Command::Ping).await,
        "status" => rpc(&host, &token, Command::Status).await,
        "doctor" => rpc(&host, &token, Command::Doctor).await,
        "incidents" => rpc(&host, &token, Command::Incidents).await,
        "plan" => {
            // D6/D10: validate locally and show what would be sent — no network.
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab plan stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            match homelab_core::manifest::validate(&spec) {
                Ok(()) => println!(
                    "{}✓ valid{} — {} would deploy vmid {}: {} file(s), {} env(s)",
                    C_GREEN,
                    C_RESET,
                    spec.manifest.stack_name,
                    spec.manifest.vmid,
                    spec.files.len(),
                    spec.env.len()
                ),
                Err(e) => die(&format!("validation failed: {}", e)),
            }
        }
        "deploy" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab deploy stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            // D10: fail fast client-side before opening a connection.
            if let Err(e) = homelab_core::manifest::validate(&spec) {
                die(&format!("validation failed: {}", e));
            }
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
            println!("  homelab ping|status|doctor|incidents");
            println!("  homelab plan stacks/<name>     validate locally (no network)");
            println!("  homelab deploy stacks/<name>");
            println!("env: HOMELAB_HOST (default 10.10.5.250:8443), HOMELAB_TOKEN");
            println!("cert pin: ~/.config/homelab/pin (auto on first connect)");
        }
    }
}

async fn rpc(host: &str, token: &str, command: Command) {
    let url = format!("wss://{}/api/ws", host);
    let mut request = url
        .clone()
        .into_client_request()
        .unwrap_or_else(|e| die(&format!("bad url {}: {}", url, e)));
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );

    // A4: pin the host certificate (TOFU on first connect).
    let pin = load_pin();
    let first_connect = pin.is_none();
    let verifier = homelab_client::tls::PinnedVerifier::new(pin);
    let tls_config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    let connector = Connector::Rustls(Arc::new(tls_config));

    let (ws, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .unwrap_or_else(|e| die(&format!("connect {}: {}", url, e)));

    if first_connect {
        if let Some(fp) = verifier.observed() {
            save_pin(&fp);
            eprintln!(
                "{}● pinned host certificate SHA256:{}{}",
                C_YELLOW, fp, C_RESET
            );
            eprintln!(
                "{}  verify this matches the fingerprint the host printed at boot{}",
                C_DIM, C_RESET
            );
        }
    }
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
            ServerMsg::State(fleet) => {
                println!(
                    "{}fleet: {} stack(s) managed{}",
                    C_DIM,
                    fleet.stacks.len(),
                    C_RESET
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
