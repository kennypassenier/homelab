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
    // Commands that never touch the network need no token: help, offline TUI,
    // and `plan` (local validation only, D10).
    let needs_token = !matches!(
        cmd,
        "help" | "plan" | "runbook" | "presets" | "export" | "import"
    ) && !(cmd == "tui" && offline);
    if token.is_empty() && needs_token {
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
        "patch" => rpc(&host, &token, Command::PatchFleet).await,
        "config" => rpc(&host, &token, Command::GetConfig).await,
        // E8: ZFS snapshots + replication of the declared jobs.
        "zfs-replicate" => rpc(&host, &token, Command::ZfsReplicate).await,
        // C7: adopt a hand-built native-service container, and drive an
        // adopted one. The stack file is stacks/<name>/service.yml.
        "adopt" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab adopt stacks/<name>"));
            let path = Path::new(dir).join("service.yml");
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| die(&format!("cannot read {}: {}", path.display(), e)));
            let m: homelab_proto::NativeServiceManifest = serde_yaml::from_str(&raw)
                .unwrap_or_else(|e| die(&format!("service.yml parse: {}", e)));
            if let Err(problems) = homelab_core::native::validate_native(&m) {
                die(&format!("service.yml invalid: {}", problems.join("; ")));
            }
            println!(
                "{}▶ adopt {} :: CT {} · unit {} · never restarts anything{}",
                C_CYAN, m.stack_name, m.vmid, m.unit, C_RESET
            );
            rpc(&host, &token, Command::AdoptService(Box::new(m))).await;
        }
        // Y4: the client contributes what only it can see — the vmid each
        // stack directory claims — and the host contributes the rest.
        "check" => {
            let base = args.get(2).cloned().unwrap_or_else(|| "stacks".into());
            let stack_files = crate::spec::stack_files_with_vmids(&base);
            println!(
                "{}▶ fleet check :: {} stack file(s) from {}{}",
                C_CYAN,
                stack_files.len(),
                base,
                C_RESET
            );
            rpc(&host, &token, Command::FleetCheck { stack_files }).await;
        }
        "backup-native" => {
            let stack = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab backup-native <stack>"));
            rpc(
                &host,
                &token,
                Command::BackupNative {
                    stack: stack.clone(),
                },
            )
            .await;
        }
        "update-native" => {
            let stack = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab update-native <stack>"));
            rpc(
                &host,
                &token,
                Command::UpdateNative {
                    stack: stack.clone(),
                },
            )
            .await;
        }
        // H10: on-demand snapshot of vault/state/TLS/intent repo.
        "backup-host-meta" => rpc(&host, &token, Command::BackupHostMeta).await,
        // H8 (light): park / unpark a stack for the nightly scheduler.
        "enable" | "disable" => {
            let stack = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab enable|disable <stack-name>"));
            rpc(
                &host,
                &token,
                Command::SetStackEnabled {
                    stack: stack.clone(),
                    enabled: cmd == "enable",
                },
            )
            .await;
        }
        "export" => {
            // D11: single-file bundle, never secrets.
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab export stacks/<name> [out.yml]"));
            let name = Path::new(dir)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "stack".into());
            let out = args
                .get(3)
                .cloned()
                .unwrap_or_else(|| format!("{}-bundle.yml", name));
            match spec::export_bundle(Path::new(dir), &out) {
                Ok(n) => println!(
                    "{}✓ exported{} — {} ({} file(s), no secrets)",
                    C_GREEN, C_RESET, out, n
                ),
                Err(e) => die(&format!("export: {}", e)),
            }
        }
        "import" => {
            let usage = "usage: homelab import <bundle.yml> <new-name> <vmid>";
            let bundle = args.get(2).unwrap_or_else(|| die(usage));
            let name = args.get(3).unwrap_or_else(|| die(usage));
            let vmid: u16 = args
                .get(4)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| die(usage));
            match spec::import_bundle(Path::new(bundle), Path::new("stacks"), name, vmid) {
                Ok(dest) => {
                    // Validate what we just wrote with the same validator as deploy.
                    match spec::build_spec(&dest).and_then(|s| {
                        homelab_core::manifest::validate(&s).map_err(|e| e.to_string())
                    }) {
                        Ok(()) => println!(
                            "{}✓ imported{} — {} (vmid {}) :: add .env files if the apps need secrets, then deploy",
                            C_GREEN, C_RESET, dest.display(), vmid
                        ),
                        Err(e) => die(&format!("imported but invalid: {}", e)),
                    }
                }
                Err(e) => die(&format!("import: {}", e)),
            }
        }
        "resize" => {
            // C4: apply the manifest's resources to the live container.
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab resize stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            println!(
                "{}▶ resize {} :: {} MiB / {} cores / {}G{}",
                C_CYAN,
                spec.manifest.stack_name,
                spec.manifest.resources.memory_mb,
                spec.manifest.resources.cores,
                spec.manifest.resources.disk_gb,
                C_RESET
            );
            rpc(
                &host,
                &token,
                Command::ApplyResources(Box::new(spec.manifest)),
            )
            .await;
        }
        "templates" => rpc(&host, &token, Command::ListTemplates).await,
        "template-build" => {
            // B8: bake the golden template. Temp vmid defaults to 999.
            let temp_vmid: u16 = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(999);
            let version: u32 = args.get(3).and_then(|v| v.parse().ok()).unwrap_or(1);
            // O2: `--privileged` builds the second template. CT 105 and 106
            // are privileged and a clone cannot change that, so they need one.
            let unprivileged = !args.iter().any(|a| a == "--privileged");
            println!(
                "{}▶ template build :: debian-12-homelab-v{}{} on temp vmid {}{}",
                C_CYAN,
                version,
                if unprivileged { "" } else { "-priv" },
                temp_vmid,
                C_RESET
            );
            rpc(
                &host,
                &token,
                Command::BuildTemplate {
                    temp_vmid,
                    version,
                    unprivileged,
                },
            )
            .await;
        }
        "exec" => {
            // A6: requires exec_enabled = true in the host config.
            let vmid: u16 = args
                .get(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| die("usage: homelab exec <vmid> <command...>"));
            let command = args[3..].join(" ");
            if command.is_empty() {
                die("usage: homelab exec <vmid> <command...>");
            }
            rpc(&host, &token, Command::ExecIn { vmid, command }).await;
        }
        "presets" => {
            // G2: list the data-driven preset catalog (local, no network).
            for pr in homelab_client::scaffold::scan_presets(Path::new("presets")) {
                let src = if pr.dir.is_some() {
                    ""
                } else {
                    " (built-in fallback)"
                };
                println!(
                    "{:<14} {:>5} MiB  {}  [{}]{}",
                    pr.name,
                    pr.meta.ram_mb,
                    pr.meta.description,
                    if pr.apps.is_empty() {
                        "no apps".to_string()
                    } else {
                        pr.apps.join(", ")
                    },
                    src
                );
            }
        }
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
        "backup" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab backup stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            rpc(&host, &token, Command::BackupStack(Box::new(spec.manifest))).await;
        }
        "restore" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab restore stacks/<name> [snapshot]"));
            let snapshot = args.get(3).cloned().unwrap_or_else(|| "latest".into());
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            println!(
                "{}▶ restore {} from '{}'{}",
                C_YELLOW, spec.manifest.stack_name, snapshot, C_RESET
            );
            rpc(
                &host,
                &token,
                Command::RestoreStack {
                    manifest: Box::new(spec.manifest),
                    snapshot,
                },
            )
            .await;
        }
        "update" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab update stacks/<name> [app]"));
            let app = args.get(3).cloned();
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            println!(
                "{}▶ update {} :: {}{}",
                C_CYAN,
                spec.manifest.stack_name,
                app.as_deref().unwrap_or("all apps"),
                C_RESET
            );
            rpc(
                &host,
                &token,
                Command::UpdateStack {
                    manifest: Box::new(spec.manifest),
                    app,
                },
            )
            .await;
        }
        "release-update" => {
            // H7: fetch the newest GitHub release, verify its checksum, and
            // ship it over the line — the host's selfcheck/rollback pipeline
            // takes it from there.
            let tag = match args.get(2).cloned() {
                Some(t) => t,
                None => homelab_client::release::latest_release_tag()
                    .unwrap_or_else(|| die("no release found (gh authenticated? release exists?)")),
            };
            println!(
                "{}▶ release update :: staging {} from GitHub{}",
                C_CYAN, tag, C_RESET
            );
            match homelab_client::release::stage_release(&tag) {
                Ok(binary_b64) => {
                    println!(
                        "{}✓ checksum verified — shipping over the line{}",
                        C_GREEN, C_RESET
                    );
                    rpc(&host, &token, Command::SelfUpdateHost { binary_b64 }).await;
                }
                Err(e) => die(&e),
            }
        }
        "self-update" => {
            // H5: ship a new HOST binary over the line; the host selfchecks,
            // installs with an armed rollback, and restarts itself.
            let path = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab self-update <path-to-homelab-host-binary>"));
            let bytes = std::fs::read(path).unwrap_or_else(|e| die(&format!("{}: {}", path, e)));
            use base64::Engine as _;
            let binary_b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            println!(
                "{}▶ self-update :: shipping {} ({} KiB){}",
                C_YELLOW,
                path,
                bytes.len() / 1024,
                C_RESET
            );
            rpc(&host, &token, Command::SelfUpdateHost { binary_b64 }).await;
        }
        "runbook" => {
            // E7: generate the disaster-recovery runbook from the local stacks
            // directory — a document that works when everything else is down.
            let out = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "docs/DR_RUNBOOK.md".into());
            match spec::generate_runbook(Path::new("stacks"), &out) {
                Ok(n) => println!(
                    "{}✓ runbook written{} — {} ({} stack(s))",
                    C_GREEN, C_RESET, out, n
                ),
                Err(e) => die(&format!("runbook: {}", e)),
            }
        }
        "destroy" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab destroy stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            let stack = &spec.manifest.stack_name;
            // C2: typed-name confirmation, exactly like the TUI.
            eprint!(
                "{}Type the stack name '{}' to confirm destroy: {}",
                C_RED, stack, C_RESET
            );
            use std::io::Write as _;
            std::io::stderr().flush().ok();
            let mut typed = String::new();
            std::io::stdin().read_line(&mut typed).ok();
            let confirm = typed.trim().to_string();
            if &confirm != stack {
                die("name mismatch — aborted");
            }
            rpc(
                &host,
                &token,
                Command::DestroyStack {
                    manifest: Box::new(spec.manifest),
                    confirm,
                },
            )
            .await;
        }
        _ => {
            println!("homelab v{} — usage:", env!("CARGO_PKG_VERSION"));
            println!("  homelab ping|status|doctor|incidents");
            println!("  homelab plan stacks/<name>          validate locally (no network)");
            println!("  homelab deploy stacks/<name>");
            println!("  homelab backup stacks/<name>        restic snapshot (E1)");
            println!("  homelab restore stacks/<name> [snap]  restore from snapshot (E2)");
            println!("  homelab update stacks/<name> [app]  pull+up with rollback (D9/B6)");
            println!(
                "  homelab patch                       apt dist-upgrade all managed stacks (H6)"
            );
            println!("  homelab destroy stacks/<name>       gated destroy (C2)");
            println!(
                "  homelab enable|disable <stack>      (un)park for the nightly scheduler (H8)"
            );
            println!("  homelab backup-host-meta            snapshot vault/state/TLS/repo (H10)");
            println!("  homelab check [stacks/]             hold the repo against reality (Y4)");
            println!("  homelab adopt stacks/<name>         adopt a native-service CT (C7)");
            println!(
                "  homelab backup-native|update-native <stack>  drive an adopted service (C7)"
            );
            println!("  homelab zfs-replicate               ZFS snapshots + replication (E8)");
            println!("  homelab runbook [out.md]            generate DR runbook (E7, local)");
            println!("  homelab presets                     list the preset catalog (local)");
            println!(
                "  homelab exec <vmid> <cmd...>        remote exec (A6, requires exec_enabled)"
            );
            println!("  homelab self-update <binary>        replace HOST binary w/ rollback (H5)");
            println!("env: HOMELAB_HOST (default 10.10.5.250:8443), HOMELAB_TOKEN");
            println!("cert pin: ~/.config/homelab/pin (auto on first connect)");
        }
    }
}

async fn rpc(host: &str, token: &str, command: Command) {
    // Commands whose real payload arrives as a separate broadcast frame
    // (Config) may see RpcDone first — wait for the payload before exiting.
    let awaits_payload = matches!(command, Command::GetConfig);
    let mut payload_seen = false;
    let mut done: Option<bool> = None;
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
            ServerMsg::Config(view) => {
                payload_seen = true;
                // G8: plain-text dump for the CLI (`homelab config`).
                let hour = view
                    .backup_hour
                    .map(|h| format!("{:02}:00", h))
                    .unwrap_or_else(|| "off".into());
                println!("nightly run : {}", hour);
                println!(
                    "webhook     : {}",
                    view.notify_webhook.as_deref().unwrap_or("off")
                );
                for (i, t) in view.retention.iter().enumerate() {
                    let span = t
                        .span_days
                        .map(|d| format!("for {} days", d))
                        .unwrap_or_else(|| "forever".into());
                    println!("retention {} : every {} days {}", i + 1, t.every_days, span);
                }
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
                if !resp.ok {
                    println!("{}✗ {}{}", C_RED, resp.message, C_RESET);
                    std::process::exit(1);
                }
                if homelab_client::rpc_can_exit(awaits_payload, payload_seen, true) {
                    println!("{}✓ {}{}", C_GREEN, resp.message, C_RESET);
                    std::process::exit(0);
                }
                done = Some(true);
            }
        }
        if done.is_some() && homelab_client::rpc_can_exit(awaits_payload, payload_seen, true) {
            println!("{}✓ ok{}", C_GREEN, C_RESET);
            std::process::exit(0);
        }
    }
    die("connection closed before RPC completed");
}
