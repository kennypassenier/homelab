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

/// Fill HOMELAB_HOST/HOMELAB_TOKEN from a config file when they are not
/// already in the environment.
///
/// `~/.config/homelab/env` first, then `./.env` for anybody standing in the
/// repository. Both are `KEY=value` files; quotes are stripped because a
/// shell-sourced file usually has them and a token with a quote in it would
/// otherwise fail in a way that reads like a wrong token.
fn load_config_env() {
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(std::path::PathBuf::from(home).join(".config/homelab/env"));
    }
    candidates.push(std::path::PathBuf::from(".env"));
    for path in candidates {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        for line in text.lines() {
            let t = line.trim();
            if t.starts_with('#') || t.is_empty() {
                continue;
            }
            let Some((k, v)) = t.split_once('=') else {
                continue;
            };
            let k = k.trim().trim_start_matches("export ").trim();
            if k != "HOMELAB_HOST" && k != "HOMELAB_TOKEN" {
                continue;
            }
            if std::env::var(k).is_ok() {
                continue; // the environment already said so
            }
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                std::env::set_var(k, v);
            }
        }
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    // Kenny, 2026-09-02: `homelab check` has to work from any directory
    // without sourcing anything first. Every command in every document here
    // was written as `homelab <verb>`, and every one of them needed
    // `set -a; . ./.env` in front of it and a repository to stand in — a
    // ritual nobody wrote down and nothing enforced.
    //
    // Order: the environment wins (so a one-off override still works), then
    // the user's own config, then the repository's `.env` when standing in
    // it. Reading, never writing: this file is where the token lives, not a
    // cache of it.
    load_config_env();
    let host = std::env::var("HOMELAB_HOST").unwrap_or_else(|_| "10.10.5.250:8443".into());
    let token = std::env::var("HOMELAB_TOKEN").unwrap_or_default();
    let offline = args.iter().any(|a| a == "--offline" || a == "--demo");
    // Commands that never touch the network need no token: help, offline TUI,
    // and `plan` (local validation only, D10).
    let needs_token = !matches!(
        cmd,
        "help" | "plan" | "runbook" | "dashboard" | "presets" | "export" | "import"
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
        // T11: install a native service into a container the deploy has
        // already created. The other half of C7 — until now the orchestrator
        // could take over a hand-built container and could not build one.
        "install-native" => {
            let dir = args.get(2).unwrap_or_else(|| {
                die("usage: homelab install-native stacks/<name>[/<unit>] [<tag>]")
            });
            let path = Path::new(dir).join("service.yml");
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| die(&format!("cannot read {}: {}", path.display(), e)));
            let m: homelab_proto::NativeServiceManifest = serde_yaml::from_str(&raw)
                .unwrap_or_else(|e| die(&format!("service.yml parse: {}", e)));
            if let Err(problems) = homelab_core::native::validate_native(&m) {
                die(&format!("service.yml invalid: {}", problems.join("; ")));
            }
            let Some(repo) = m.release_repo.clone() else {
                die(&format!(
                    "{} declares no release_repo — this service is adopt-only, and where its \
                     binary comes from is not written down anywhere. Add release_repo to its \
                     service.yml rather than installing by hand again",
                    m.unit
                ));
            };
            // The unit file lives beside the service file, or in the unit's
            // own directory when several services share one stack.
            let unit_name = format!("{}.service", m.unit);
            let candidates = [
                Path::new(dir).join(&unit_name),
                Path::new(dir).join(&m.unit).join(&unit_name),
            ];
            let unit_file = candidates
                .iter()
                .find_map(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| {
                    die(&format!(
                        "no {} found beside {} — the file that makes the service exist is not \
                         in the repository, so a rebuilt container would have the binary and \
                         nothing to run it",
                        unit_name, dir
                    ))
                });
            let asset = m.asset_name().to_string();
            let tag = match args.get(3).cloned() {
                Some(t) => t,
                None => homelab_client::release::latest_tag_of(&repo).unwrap_or_else(|| {
                    die(&format!("no release found in {} (gh authenticated?)", repo))
                }),
            };
            println!(
                "{}▶ install-native {} :: CT {} · {} {} from {}{}",
                C_CYAN, m.unit, m.vmid, asset, tag, repo, C_RESET
            );
            let binary_b64 = homelab_client::release::stage_asset(&repo, &tag, &asset)
                .unwrap_or_else(|e| die(&e));
            if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
                die(&why);
            }
            println!(
                "{}✓ checksum verified — shipping over the line{}",
                C_GREEN, C_RESET
            );
            rpc(
                &host,
                &token,
                Command::InstallNative {
                    manifest: Box::new(m),
                    binary_b64,
                    unit_file,
                },
            )
            .await;
        }
        // Y4: the client contributes what only it can see — the vmid each
        // stack directory claims — and the host contributes the rest.
        // G1: the runaway guards, for a container the orchestrator did not
        // build. They are what keeps a container able to run for years.
        "guards" => {
            let vmid: u16 = args
                .get(2)
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| die("usage: homelab guards <vmid>"));
            println!(
                "{}▶ guards :: CT {} — log caps, journald limits, logrotate, weekly prune{}",
                C_CYAN, vmid, C_RESET
            );
            rpc(&host, &token, Command::ApplyGuards { vmid }).await;
        }
        "forget" => {
            let stack = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab forget <stack>"))
                .clone();
            rpc(&host, &token, Command::ForgetStack { stack }).await;
        }
        "check" => {
            let base = args.get(2).cloned().unwrap_or_else(|| "stacks".into());
            let stack_files = crate::spec::stack_files_with_vmids(&base);
            // Kenny ran this from inside `stacks/` on 2026-09-02 and it said
            // "0 stack file(s)" in passing, then reported the host's half as
            // if it were the whole answer. Half a check that looks like a
            // whole one is this project's most-repeated fault; say so.
            if stack_files.is_empty() {
                println!(
                    "{}▶ fleet check :: no stack files under '{}' — checking only what the \
                     HOST can see{}",
                    C_YELLOW, base, C_RESET
                );
                println!(
                    "  the half that compares your stack files against the fleet is SKIPPED. \
                     Run this from the repository root, or pass the path: homelab check \
                     ~/Projects/homelab/stacks"
                );
            } else {
                println!(
                    "{}▶ fleet check :: {} stack file(s) from {}{}",
                    C_CYAN,
                    stack_files.len(),
                    base,
                    C_RESET
                );
            }
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
        // G17: the questions only a person can answer. `homelab checks` lists
        // them with their ids; `homelab checks answer <id> ok|nok [note]`
        // records one. They used to be printed at the end of a deploy and
        // stored nowhere, which is not asking anybody anything.
        "checks" => match args.get(2).map(|s| s.as_str()) {
            None | Some("list") => rpc(&host, &token, Command::ListManualChecks).await,
            Some("answer") => {
                let id = args
                    .get(3)
                    .unwrap_or_else(|| die("usage: homelab checks answer <id> ok|nok [note]"));
                let verdict = args
                    .get(4)
                    .unwrap_or_else(|| die("usage: homelab checks answer <id> ok|nok [note]"));
                let yes = ["ok", "yes", "ja"];
                let no = ["nok", "no", "nee"];
                let ok = if yes.contains(&verdict.as_str()) {
                    true
                } else if no.contains(&verdict.as_str()) {
                    false
                } else {
                    die(&format!("answer must be ok or nok, not {}", verdict))
                };
                rpc(
                    &host,
                    &token,
                    Command::AnswerManualCheck {
                        id: id.clone(),
                        ok,
                        note: args[5..].join(" "),
                    },
                )
                .await;
            }
            Some(other) => die(&format!(
                "unknown: homelab checks {} — try `list` or `answer`",
                other
            )),
        },
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
        "new" => {
            // T65 / F136: a stack could only ever be created through the TUI
            // wizard. Twenty-one commands and not one of them scaffolded, so
            // every stack made without the TUI was hand-written — which is
            // how three stack files ended up claiming live vmids.
            //
            // Same scaffolder, same preset catalogue, same defaults as the
            // wizard: `homelab new <name> --preset <p> --vmid <n> [...]`.
            let name = args
                .get(2)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .unwrap_or_else(|| {
                    die("usage: homelab new <name> --preset <preset> --vmid <n> \
                         [--ram MiB] [--cores N] [--disk GiB] [--swap MiB] \
                         [--no-data /appdata/<stack>/<app>-config]")
                });
            let flag = |k: &str| -> Option<String> {
                args.iter()
                    .position(|a| a == k)
                    .and_then(|i| args.get(i + 1))
                    .cloned()
            };
            let num = |k: &str, what: &str| -> Option<u64> {
                flag(k).map(|v| {
                    v.parse()
                        .unwrap_or_else(|_| die(&format!("{} takes a number, got '{}'", what, v)))
                })
            };
            let presets_dir = Path::new("presets");
            let presets = homelab_client::scaffold::scan_presets(presets_dir);
            let preset_name = flag("--preset")
                .unwrap_or_else(|| die("--preset is required; `homelab presets` lists them"));
            let preset = presets
                .iter()
                .find(|p| p.name == preset_name)
                .unwrap_or_else(|| {
                    die(&format!(
                        "no preset '{}' — `homelab presets` lists what there is",
                        preset_name
                    ))
                });
            let vmid = num("--vmid", "--vmid").unwrap_or_else(|| {
                die("--vmid is required: the address and hostname are derived from it")
            }) as u16;
            let d = homelab_client::scaffold::StackDefaults::default();
            let ram = num("--ram", "--ram").unwrap_or(preset.meta.ram_mb as u64) as u32;
            // Every `--no-data` flag names one path that keeps nothing of its
            // own (D79). The wizard asks this per directory; here it is
            // repeatable so a script can say the same thing.
            let no_data: Vec<String> = args
                .iter()
                .enumerate()
                .filter(|(_, a)| *a == "--no-data")
                .filter_map(|(i, _)| args.get(i + 1).cloned())
                .collect();
            let params = homelab_client::scaffold::StackParams {
                name: &name,
                vmid,
                ram_mb: ram,
                cores: num("--cores", "--cores").unwrap_or(preset.meta.cores.unwrap_or(2) as u64)
                    as u16,
                disk_gb: num("--disk", "--disk").unwrap_or(preset.meta.disk_gb.unwrap_or(8) as u64)
                    as u16,
                swap_mb: Some(num("--swap", "--swap").unwrap_or(d.swap_for(ram) as u64) as u32),
                preset: Some(preset),
                no_data_paths: &no_data,
            };
            match homelab_client::scaffold::scaffold_stack(
                Path::new("stacks"),
                presets_dir,
                &params,
            ) {
                Ok(s) => {
                    println!(
                        "{}✓ scaffolded {}{} — {} file(s)",
                        C_GREEN,
                        s.dir.display(),
                        C_RESET,
                        s.files.len()
                    );
                    for f in &s.files {
                        println!("    {}", f);
                    }
                    println!(
                        "\n  next: read the compose files, then `homelab plan stacks/{}`",
                        name
                    );
                }
                Err(e) => die(&e),
            }
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
                    if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
                        die(&why);
                    }
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
            if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
                die(&why);
            }
            println!(
                "{}▶ self-update :: shipping {} ({} KiB){}",
                C_YELLOW,
                path,
                bytes.len() / 1024,
                C_RESET
            );
            rpc(&host, &token, Command::SelfUpdateHost { binary_b64 }).await;
        }
        "dashboard" => {
            // T2's generator, run locally for a stack the orchestrator does
            // not manage yet. CT 104, 105, 106 and 111 predate this project
            // and get their dashboard on deploy only once they are adopted
            // (M8); until then Kenny's four busiest containers would have no
            // dashboard at all, which is exactly the wrong four to be blind
            // about.
            //
            // Deliberately the same function the deploy calls, so what is
            // written by hand today is byte-identical to what the deploy
            // writes later — the adoption replaces the file instead of
            // fighting it.
            let stack = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab dashboard <stack> <app>..."));
            let apps: Vec<String> = args.iter().skip(3).cloned().collect();
            if apps.is_empty() {
                die("usage: homelab dashboard <stack> <app>... — name at least one app");
            }
            print!(
                "{}",
                homelab_core::ops::dashboard::dashboard_json(stack, &apps)
            );
        }
        // Phase 7's output document, derived from the tests rather than kept
        // beside them — the same reasoning as `runbook`.
        "testplan" => {
            let out = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "docs/deployment/TEST_PLAN.md".into());
            match homelab_client::testplan::generate_test_plan(
                &[Path::new("core/tests"), Path::new("client/tests")],
                Path::new("docs/deployment/REALIZATION_PLAN.md"),
                Path::new(&out),
            ) {
                Ok(n) => println!(
                    "{}✓ test plan written{} — {} ({} suite(s))",
                    C_GREEN, C_RESET, out, n
                ),
                Err(e) => die(&e),
            }
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
        "prune-orphans" => {
            // Kenny's H2b: the deploy REPORTS files the repository no longer
            // has, and this removes them — after the same typed confirmation
            // a destroy asks for. Two steps on purpose: deleting is the
            // irreversible direction, and a deploy runs when nobody is
            // looking.
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab prune-orphans stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            let stack = spec.manifest.stack_name.clone();
            eprint!(
                "{}Type the stack name '{}' to remove the files the repository no longer \
                 has (the deploy log lists them): {}",
                C_RED, stack, C_RESET
            );
            use std::io::Write as _;
            std::io::stderr().flush().ok();
            let mut typed = String::new();
            std::io::stdin().read_line(&mut typed).ok();
            let confirm = typed.trim().to_string();
            if confirm != stack {
                die("name mismatch — nothing removed");
            }
            rpc(
                &host,
                &token,
                Command::PruneOrphans {
                    manifest: Box::new(spec.manifest.clone()),
                    spec: Box::new(spec),
                    confirm,
                },
            )
            .await;
        }
        "destroy" => {
            let dir = args
                .get(2)
                .unwrap_or_else(|| die("usage: homelab destroy stacks/<name>"));
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            let stack = &spec.manifest.stack_name;
            // Kenny's B2: the destroy backs up first and refuses if that
            // fails. Skipping is deliberate and says so out loud.
            let skip_backup = args.iter().any(|a| a == "--no-backup");
            if skip_backup {
                eprintln!(
                    "{}! --no-backup: destroying without the backup that would otherwise be \
                     taken first{}",
                    C_YELLOW, C_RESET
                );
            }
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
                    skip_backup,
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
            println!("  homelab prune-orphans stacks/<name>  remove files the repo dropped (H2b)");
            println!(
                "  homelab enable|disable <stack>      (un)park for the nightly scheduler (H8)"
            );
            println!("  homelab backup-host-meta            snapshot vault/state/TLS/repo (H10)");
            println!("  homelab check [stacks/]             hold the repo against reality (Y4)");
            println!("  homelab guards <vmid>               apply the runaway guards (B2/G1)");
            println!("  homelab forget <stack>              drop a stale state record (no container touched)");
            println!("  homelab adopt stacks/<name>         adopt a native-service CT (C7)");
            println!(
                "  homelab backup-native|update-native <stack>  drive an adopted service (C7)"
            );
            println!("  homelab zfs-replicate               ZFS snapshots + replication (E8)");
            println!("  homelab runbook [out.md]            generate DR runbook (E7, local)");
            println!("  homelab dashboard <stack> <app>...  render a stack dashboard (T2, local)");
            println!("  homelab presets                     list the preset catalog (local)");
            println!("  homelab new <name> --preset <p> --vmid <n>   scaffold a stack (T65)");
            println!(
                "  homelab exec <vmid> <cmd...>        remote exec (A6, requires exec_enabled)"
            );
            println!("  homelab self-update <binary>        replace HOST binary w/ rollback (H5)");
            // G20 of the Phase-7 gate: eight verbs existed and appeared in no
            // help text at all, `install-native` and `release-update` among
            // them — the one that installs your own services and the one that
            // updates the host. A command nobody can discover is a command
            // that does not exist.
            println!(
                "  homelab release-update              fetch the newest release and ship it (H7)"
            );
            println!(
                "  homelab install-native stacks/<name> <unit>  install a native service (O1)"
            );
            println!("  homelab template-build              build a golden template (M3)");
            println!("  homelab templates                   list the golden templates");
            println!("  homelab resize stacks/<name>        apply changed resources (H4)");
            println!("  homelab config                      show the host's settings (G8)");
            println!(
                "  homelab testplan                    regenerate docs/deployment/TEST_PLAN.md"
            );
            println!(
                "  homelab checks                      the questions only a person can answer (G17)"
            );
            println!("  homelab checks answer <id> ok|nok [note]   record one of those answers");
            println!("  homelab export|import <file>        move state between hosts");
            println!("  homelab tui                         the terminal interface (G1)");
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

    // The request is deliberately NOT sent yet: it goes out only after the
    // host has said which version it is. See the Hello arm below.
    let req = RpcRequest { id: 1, command };
    let mut sent = false;

    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(server_msg) = serde_json::from_str::<ServerMsg>(&text) else {
            continue;
        };
        match server_msg {
            // T69: the command line is not a place to answer a question —
            // there is no prompt to draw and the operator may not even be
            // watching. Print it and let the host's timeout do the rest,
            // which lands on Unattended rather than on a guess.
            ServerMsg::Ask { op, step, what, .. } => {
                eprintln!(
                    "{}? {} :: {} is waiting for a decision — {}{}",
                    C_YELLOW, op, step, what, C_RESET
                );
                eprintln!(
                    "  no answer from here: run this from the TUI to decide, \
                     or it times out as unattended"
                );
            }
            ServerMsg::Hello { version, proto } => {
                println!(
                    "{}● HOST v{} (proto {}) — link up{}",
                    C_GREEN, version, proto, C_RESET
                );
                // A client newer than the host loses whatever the host does
                // not know about. Serde drops an unknown field silently, so
                // the deploy succeeds and simply does less than it was asked
                // to: on 2026-08-31 a host one release behind ignored the
                // `data_mounts` block, the downloader came up without its
                // disks, and 73 torrents went to `missingFiles`. Nothing said
                // a word. So the client refuses to send a mutating command to
                // an older host, and says which command fixes it.
                if homelab_client::version::mutates(&req.command)
                    && homelab_client::version::older(&version, env!("CARGO_PKG_VERSION"))
                {
                    die(&format!(
                        "host is v{} and this client is v{} :: a host that predates a \
                         field ignores it silently, which is how a deploy quietly does \
                         less than you asked — run 'homelab release-update' first",
                        version,
                        env!("CARGO_PKG_VERSION")
                    ));
                }
                if !sent {
                    tx.send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                        .await
                        .unwrap_or_else(|e| die(&format!("send: {}", e)));
                    sent = true;
                }
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
