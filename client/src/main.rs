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

use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::Message;

use homelab_proto::{Command, LogLevel, RpcRequest, ServerMsg, UiStep};

use homelab_client::{spec, tui};

// fix-109 (colour-codes-when-piped, 2026-09-27): each prints itself only when
// `main` found a terminal and no NO_COLOR; they were bare escape codes that
// landed in every pipe and log.
use homelab_client::output::Paint;
const C_RESET: Paint = Paint("\x1b[0m");
const C_CYAN: Paint = Paint("\x1b[36m");
const C_GREEN: Paint = Paint("\x1b[32m");
const C_YELLOW: Paint = Paint("\x1b[33m");
const C_RED: Paint = Paint("\x1b[31m");
const C_DIM: Paint = Paint("\x1b[2m");

/// Where the address in use came from, and the repository's pin — set once
/// in `main`, read by `rpc`, which is called from every verb.
static HOST_SOURCE: std::sync::OnceLock<homelab_client::repo_config::HostSource> =
    std::sync::OnceLock::new();
static REPO_PIN: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
/// fix-101 (cli-path-vs-name-and-cwd, 2026-09-27): the repository, found once
/// in `main`, so every verb reads the same stacks from any directory.
static REPO_ROOT: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
// fix-66: `--answer allow|stop`, read once in `run`. Pre-answers any host
// question an operation started from this command raises, so a script
// never waits on a question nobody is watching for.
static PRE_ANSWER: std::sync::OnceLock<Option<bool>> = std::sync::OnceLock::new();

fn repo_root() -> Option<&'static Path> {
    REPO_ROOT.get().and_then(|r| r.as_deref())
}

fn cwd() -> std::path::PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// fix-101: the stack directory an argument names, `almanac` or
/// `stacks/almanac` alike.
fn stack_dir(arg: &str) -> std::path::PathBuf {
    let dir = homelab_client::repo_config::stack_dir(arg, &cwd(), repo_root());
    if repo_root().is_none() && !dir.exists() {
        eprintln!(
            "{}note: no repository found from here — run this inside it, or set \
             HOMELAB_REPO=<path> in ~/.config/homelab/env{}",
            C_YELLOW, C_RESET
        );
    }
    dir
}

/// fix-101: the stacks directory, the repository's wherever the command runs.
fn stacks_base() -> std::path::PathBuf {
    homelab_client::repo_config::stacks_dir(repo_root())
}

/// fix-101: a path inside the repository, or relative to where the command
/// runs when no repository was found.
fn in_repo(rel: &str) -> std::path::PathBuf {
    repo_root()
        .map(|r| r.join(rel))
        .unwrap_or_else(|| std::path::PathBuf::from(rel))
}

fn die(msg: &str) -> ! {
    eprintln!("{}error:{} {}", C_RED, C_RESET, msg);
    std::process::exit(1);
}

/// fix-92 (routes-outside-repo-unvalidated, 2026-09-27): every route in the
/// stacks directory `base`, held against every other before anything is
/// sent — one owner per hostname, every backend a stack's address or
/// declared external. The whole directory rather than the one stack: a
/// duplicate hostname is a fact about two stacks, and the one being planned
/// may be either of them.
/// feat-stacks-7: the verbs that act on a stack take their arguments from
/// `cli_args::parse`, the parser the dashboard's "copy as CLI command" is
/// tested against.
fn invocation(args: &[String]) -> homelab_client::cli_args::Invocation {
    match homelab_client::cli_args::parse(args.get(1..).unwrap_or(&[])) {
        Ok(Some(inv)) => inv,
        Ok(None) => die("internal: this verb has no parsed form"),
        Err(e) => die(&e),
    }
}

fn check_fleet_routes(base: &Path) {
    match spec::fleet_route_problems(base) {
        Ok(problems) if problems.is_empty() => {}
        Ok(problems) => die(&format!("route check failed:\n  {}", problems.join("\n  "))),
        Err(e) => die(&format!("route check: {}", e)),
    }
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
            // Every HOMELAB_* key, not a hand-picked pair.
            //
            // The first version named HOMELAB_HOST and HOMELAB_TOKEN
            // explicitly and silently left HOMELAB_LATCH_ENV behind, so
            // `homelab check` worked from anywhere and `homelab deploy` of any
            // stack with secrets did not — a fix that turned "works from any
            // directory" into "works for some commands", which is worse than
            // not fixing it, because it looks done.
            if !k.starts_with("HOMELAB_") {
                continue;
            }
            if std::env::var(k).is_ok() {
                continue; // the environment already said so
            }
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                // SAFETY: this runs before the tokio runtime starts (this is
                // the first thing main() calls), so no other thread exists
                // yet to observe the write. The one exception to the
                // workspace's `unsafe_code = "deny"` besides fix-53's
                // `libc::killpg`.
                #[allow(unsafe_code)]
                unsafe {
                    std::env::set_var(k, v);
                }
            }
        }
    }
}

/// The env file is read before the tokio runtime starts: `set_var` while
/// worker threads may read the environment is a data race (and `unsafe` from
/// edition 2024 on, which is what kept the workspace on 2021).
/// rust-code-hygiene, expert panel 2026-09-27.
fn main() {
    // `explicit_host` is what was typed before the command, read before the
    // env file can add to it (feat-client-1, see `run`).
    let explicit_host = std::env::var("HOMELAB_HOST").ok();
    load_config_env();
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("start the tokio runtime")
        .block_on(run(explicit_host));
}

async fn run(explicit_host: Option<String>) {
    use std::io::IsTerminal as _;
    homelab_client::output::set_colour(homelab_client::output::should_colour(
        std::io::stdout().is_terminal(),
        std::env::var("NO_COLOR").ok().as_deref(),
    ));
    let args: Vec<String> = std::env::args().collect();
    // fix-68: bare `homelab` used to print the whole help; what an operator
    // sitting down actually wants is the morning answer ("is anything
    // waiting for me?"), the same one `homelab today` gives.
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("today");

    // F309: `--help` anywhere means SHOW the help, never do the thing.
    //
    // On 2026-09-09 `homelab template-build --help` did not print usage. The
    // verb reads its arguments positionally, `--help` failed to parse as a
    // vmid, the parse fell back to the default — and a real golden template
    // was built on a live host, ending as a stray Debian 12 template that had
    // to be destroyed afterwards. Nothing was lost, and only because that
    // verb's default target is a scratch vmid it owns.
    //
    // `unwrap_or(default)` on an argument that could not be understood is the
    // fault, and it is a fault this whole suite is otherwise built against:
    // an unparseable input is a question, not a licence to proceed. Fixing it
    // here covers every verb at once rather than the one that happened to
    // bite.
    if homelab_client::version::wants_help(&args) {
        // fix-108 (help-flat-ids-typo-exit0, 2026-09-27): `homelab <verb>
        // --help` is that verb's help with an example, not the whole list.
        match args
            .get(1)
            .and_then(|v| homelab_client::cli_help::verb_help(v))
        {
            Some(help) => print!("{}", help),
            None => print!("{}", homelab_client::cli_help::usage()),
        }
        std::process::exit(0);
    }
    // fix-108: a mistyped verb printed the help and exited 0, so a typo in a
    // script looked like success. It says what was probably meant and exits 2.
    if !homelab_client::cli_help::is_verb(cmd) {
        let hint = homelab_client::cli_help::suggest(cmd)
            .map(|v| format!("; did you mean '{}'?", v))
            .unwrap_or_default();
        eprintln!(
            "{}error:{} unknown command '{}'{} — `homelab help` lists them",
            C_RED, C_RESET, cmd, hint
        );
        std::process::exit(2);
    }

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
    // feat-client-1 (Kenny, 2026-09-19): the address is a fact about the
    // fleet and lives in the repository, `config/client.toml`; a value typed
    // before the command still wins, the machine's env file comes after the
    // repository, and the compiled-in default is the last resort.
    // (The env file was loaded in `main`, before the runtime started.)
    let root = homelab_client::repo_config::repo_root(
        &cwd(),
        std::env::var("HOMELAB_REPO").ok().as_deref(),
    );
    let repo_cfg = homelab_client::repo_config::load(root.as_deref().unwrap_or(&cwd()))
        .unwrap_or_else(|e| die(&e));
    let _ = REPO_ROOT.set(root);
    let (host, host_source) = homelab_client::repo_config::resolve_host(
        explicit_host,
        repo_cfg.as_ref().map(|(p, c)| (p.as_path(), c)),
        std::env::var("HOMELAB_HOST").ok(),
    );
    let _ = HOST_SOURCE.set(host_source);
    let _ = REPO_PIN.set(repo_cfg.as_ref().and_then(|(_, c)| c.pin.clone()));
    let token = std::env::var("HOMELAB_TOKEN").unwrap_or_default();
    let offline = args.iter().any(|a| a == "--offline" || a == "--demo");
    // fix-66: `--answer allow|stop` pre-answers every host question this
    // command's operation raises, without waiting for a terminal.
    let pre_answer = homelab_client::answer::parse_pre_answer_flag(
        args.iter()
            .position(|a| a == "--answer")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str),
    )
    .unwrap_or_else(|e| die(&e));
    let _ = PRE_ANSWER.set(pre_answer);
    // Commands that never touch the network need no token: help, offline TUI,
    // and `plan` (local validation only, D10).
    let needs_token = !matches!(
        cmd,
        "help"
            | "plan"
            | "runbook"
            | "update-policy"
            | "presets"
            | "export"
            | "import"
            | "self-install"
            // fix-110: local, they never reach the host.
            | "new"
            | "testplan"
    ) && !(cmd == "tui" && offline);
    if token.is_empty() && needs_token {
        // fix-110: the refusal names where the token goes.
        die(
            "HOMELAB_TOKEN is not set — put HOMELAB_TOKEN=<token> in ~/.config/homelab/env \
             (or export it); it is the token in the daemon's host.toml",
        );
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
                    repo_pin: REPO_PIN.get().cloned().flatten(),
                    built_in_pin: homelab_client::repo_config::built_in_pin().map(str::to_string),
                })
            };
            if let Err(e) = tui::run(backend, repo_root().map(Path::to_path_buf)).await {
                die(&format!("tui: {}", e));
            }
        }
        "ping" => rpc(&host, &token, Command::Ping).await,
        // feat-platform-10: one step of driving the open dashboard.
        "ui" => {
            let json = args.iter().any(|a| a == "--json");
            let words: Vec<String> = args[2..]
                .iter()
                .filter(|a| a.as_str() != "--json")
                .cloned()
                .collect();
            use homelab_client::ui_cli::UiCall;
            let call = homelab_client::ui_cli::parse_call(&words).unwrap_or_else(|e| die(&e));
            let step = match call {
                UiCall::Step(step) => step,
                UiCall::Finish => ui_finish(&host, &token, json).await,
                UiCall::PressWait(step) => {
                    let reply = rpc_reply(&host, &token, Command::Ui { step })
                        .await
                        .unwrap_or_else(|| die("the host closed the line before it answered"));
                    if !reply.ok {
                        ui_print(&reply, json);
                    }
                    if !json && let Ok(text) = homelab_client::ui_cli::render(&reply.message) {
                        print!("{text}");
                    }
                    ui_finish(&host, &token, json).await
                }
            };
            let reply = rpc_reply(&host, &token, Command::Ui { step })
                .await
                .unwrap_or_else(|| die("the host closed the line before it answered"));
            ui_print(&reply, json);
        }
        "patch" => rpc(&host, &token, Command::PatchFleet).await,
        "config" => rpc(&host, &token, Command::GetConfig).await,
        // E8: ZFS snapshots + replication of the declared jobs.
        "zfs-replicate" => rpc(&host, &token, Command::ZfsReplicate).await,
        // C7: adopt a hand-built native-service container, and drive an
        // adopted one. The stack file is stacks/<name>/service.yml.
        "adopt" => {
            let homelab_client::cli_args::Invocation::Adopt { stack } = invocation(&args) else {
                die("internal: adopt parsed as another verb")
            };
            let dir = stack_dir(&stack);
            let path = dir.join("service.yml");
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
            let dir = stack_dir(args.get(2).unwrap_or_else(|| {
                die("usage: homelab install-native stacks/<name>[/<unit>] [<tag> | --file <path>]")
            }));
            let path = dir.join("service.yml");
            let raw = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| die(&format!("cannot read {}: {}", path.display(), e)));
            let m: homelab_proto::NativeServiceManifest = serde_yaml::from_str(&raw)
                .unwrap_or_else(|e| die(&format!("service.yml parse: {}", e)));
            if let Err(problems) = homelab_core::native::validate_native(&m) {
                die(&format!("service.yml invalid: {}", problems.join("; ")));
            }
            // B7: `--file <path>` hands the bytes over from disk — the
            // rollback drill's way of installing a fake service, good and
            // then deliberately broken. A release stays the normal source.
            let source =
                homelab_client::release::install_source(&args[3..]).unwrap_or_else(|e| die(&e));
            let from_file = matches!(source, homelab_client::release::InstallSource::File(_));
            let repo = match m.release_repo.clone() {
                Some(r) => r,
                None if from_file => String::new(),
                None => die(&format!(
                    "{} declares no release_repo — this service is adopt-only, and where its \
                     binary comes from is not written down anywhere. Add release_repo to its \
                     service.yml rather than installing by hand again",
                    m.unit
                )),
            };
            // The unit file lives beside the service file, or in the unit's
            // own directory when several services share one stack.
            let unit_name = format!("{}.service", m.unit);
            let candidates = [dir.join(&unit_name), dir.join(&m.unit).join(&unit_name)];
            let unit_file = candidates
                .iter()
                .find_map(|p| std::fs::read_to_string(p).ok())
                .unwrap_or_else(|| {
                    die(&format!(
                        "no {} found beside {} — the file that makes the service exist is not \
                         in the repository, so a rebuilt container would have the binary and \
                         nothing to run it",
                        unit_name,
                        dir.display()
                    ))
                });
            let asset = m.asset_name().to_string();
            let binary_b64 = match source {
                homelab_client::release::InstallSource::File(path) => {
                    let (b64, sha) =
                        homelab_client::release::stage_file(&path).unwrap_or_else(|e| die(&e));
                    println!(
                        "{}▶ install-native {} :: CT {} · from file {} (sha256 {}){}",
                        C_CYAN, m.unit, m.vmid, path, sha, C_RESET
                    );
                    b64
                }
                homelab_client::release::InstallSource::Release(tag) => {
                    let tag = match tag {
                        Some(t) => t,
                        None => {
                            homelab_client::release::latest_tag_of(&repo).unwrap_or_else(|| {
                                die(&format!("no release found in {} (gh authenticated?)", repo))
                            })
                        }
                    };
                    println!(
                        "{}▶ install-native {} :: CT {} · {} {} from {}{}",
                        C_CYAN, m.unit, m.vmid, asset, tag, repo, C_RESET
                    );
                    let b64 = homelab_client::release::stage_asset(&repo, &tag, &asset)
                        .unwrap_or_else(|e| die(&e));
                    println!(
                        "{}✓ checksum verified — shipping over the line{}",
                        C_GREEN, C_RESET
                    );
                    b64
                }
            };
            if let Some(why) = homelab_client::version::too_large(binary_b64.len()) {
                die(&why);
            }
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
            let homelab_client::cli_args::Invocation::Guards { vmid } = invocation(&args) else {
                die("internal: guards parsed as another verb")
            };
            println!(
                "{}▶ guards :: CT {} — log caps, journald limits, logrotate, weekly prune{}",
                C_CYAN, vmid, C_RESET
            );
            rpc(&host, &token, Command::ApplyGuards { vmid }).await;
        }
        "forget" => {
            let homelab_client::cli_args::Invocation::Forget { stack } = invocation(&args) else {
                die("internal: forget parsed as another verb")
            };
            let stack = homelab_client::repo_config::stack_name(&stack);
            rpc(&host, &token, Command::ForgetStack { stack }).await;
        }
        "check" => {
            let base = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| stacks_base().display().to_string());
            let stack_files = crate::spec::stack_files_with_vmids(&base);
            let digests = stack_digests(&stack_files);
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
                     Run this inside the repository, set HOMELAB_REPO in \
                     ~/.config/homelab/env, or pass the path: homelab check \
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
            let fleet_ok = rpc_with(
                &host,
                &token,
                Command::FleetCheck {
                    json: false,
                    stack_files,
                    digests,
                    host_config: declared_host_config(),
                },
            )
            .await;
            // fix-143 (expert panel 2026-09-27, edge-changes-unnoticed): the
            // Cloudflare edge against captured/gateway/, from here because
            // the read-only token and the capture both live on this side.
            let captured = Path::new(&base).join("../captured/gateway");
            let edge_ok = match homelab_client::edge::check_edge(&captured) {
                homelab_client::edge::EdgeOutcome::NotCompared(why) => {
                    println!("{}edge: not compared — {}{}", C_DIM, why, C_RESET);
                    true
                }
                homelab_client::edge::EdgeOutcome::Compared(findings) if findings.is_empty() => {
                    println!("edge: Cloudflare agrees with captured/gateway/");
                    true
                }
                homelab_client::edge::EdgeOutcome::Compared(findings) => {
                    println!("edge: {} finding(s)", findings.len());
                    for f in &findings {
                        println!(
                            "  [{}] {} — {}\n      remedy: {}",
                            match f.severity {
                                homelab_core::ops::fleetcheck::Severity::Broken => "broken",
                                homelab_core::ops::fleetcheck::Severity::Drift => "drift",
                                homelab_core::ops::fleetcheck::Severity::Noted => "noted",
                            },
                            f.subject,
                            f.what,
                            f.remedy
                        );
                    }
                    homelab_core::ops::fleetcheck::check_passes(&findings)
                }
            };
            // gap-37 (2026-09-28, Kenny: "eerst checken of de image bestaat"):
            // every pinned digest in the stack files, asked of its registry.
            let pin_findings = homelab_client::pinexists::check_pins(Path::new(&base));
            let broken_pins: Vec<_> = pin_findings
                .iter()
                .filter(|f| f.severity == homelab_core::ops::fleetcheck::Severity::Broken)
                .collect();
            if broken_pins.is_empty() {
                println!(
                    "pins: every pinned image digest still exists ({} not asked)",
                    pin_findings.len()
                );
            } else {
                println!("pins: {} missing upstream", broken_pins.len());
                for f in &broken_pins {
                    println!(
                        "  [broken] {} — {}\n      remedy: {}",
                        f.subject, f.what, f.remedy
                    );
                }
            }
            let pins_ok = homelab_core::ops::fleetcheck::check_passes(&pin_findings);
            std::process::exit(if fleet_ok && edge_ok && pins_ok { 0 } else { 1 });
        }
        // fix-68 (four-answers-to-is-anything-wrong, 2026-09-27): the morning
        // question in one verb. `check`, `doctor`, `incidents` and `checks`
        // each gave part of the answer, and the operator merged them in his
        // head; this asks the host for all of it and prints one verdict.
        "today" => {
            let base = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| stacks_base().display().to_string());
            let stack_files = crate::spec::stack_files_with_vmids(&base);
            let digests = stack_digests(&stack_files);
            if stack_files.is_empty() {
                println!(
                    "{}▶ today :: no stack files under '{}' — the check reads only what the \
                     HOST can see; run this inside the repository, set HOMELAB_REPO in \
                     ~/.config/homelab/env, or pass the path{}",
                    C_YELLOW, base, C_RESET
                );
            } else {
                println!(
                    "{}▶ today :: doctor, fleet check ({} stack file(s)), incidents, manual \
                     checks — about a minute{}",
                    C_CYAN,
                    stack_files.len(),
                    C_RESET
                );
            }
            let reply = rpc_reply(
                &host,
                &token,
                Command::Today {
                    stack_files,
                    digests,
                    host_config: declared_host_config(),
                },
            )
            .await
            .unwrap_or_else(|| die("the host did not answer"));
            let today: homelab_core::ops::today::Today = serde_json::from_str(&reply.message)
                .unwrap_or_else(|_| die(&format!("the host answered: {}", reply.message)));
            let color = if today.needs_you() { C_YELLOW } else { C_GREEN };
            let text = homelab_core::ops::today::render(&today);
            let (body, verdict) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
            if !body.is_empty() {
                println!("{}", body);
            }
            println!("{}{}{}", color, verdict, C_RESET);
            std::process::exit(if today.needs_you() { 1 } else { 0 });
        }
        "backup-native" => {
            let homelab_client::cli_args::Invocation::BackupNative { stack } = invocation(&args)
            else {
                die("internal: backup-native parsed as another verb")
            };
            let stack = homelab_client::repo_config::stack_name(&stack);
            rpc(&host, &token, Command::BackupNative { stack }).await;
        }
        "update-native" => {
            let homelab_client::cli_args::Invocation::UpdateNative { stack } = invocation(&args)
            else {
                die("internal: update-native parsed as another verb")
            };
            let stack = homelab_client::repo_config::stack_name(&stack);
            rpc(&host, &token, Command::UpdateNative { stack }).await;
        }
        // B1: the orchestrator's own release update of a native stack, now.
        "release-update-native" => {
            let homelab_client::cli_args::Invocation::ReleaseUpdateNative { stack } =
                invocation(&args)
            else {
                die("internal: release-update-native parsed as another verb")
            };
            let stack = homelab_client::repo_config::stack_name(&stack);
            rpc(&host, &token, Command::ReleaseUpdateNative { stack }).await;
        }
        // fix-114: back to a native unit's kept previous binary.
        "rollback-native" => {
            let homelab_client::cli_args::Invocation::RollbackNative { stack, unit } =
                invocation(&args)
            else {
                die("internal: rollback-native parsed as another verb")
            };
            rpc(&host, &token, Command::RollbackNative { stack, unit }).await;
        }
        // Route A: ask every configured device for its own configuration now,
        // instead of waiting for 04:00 to find out whether it works.
        "backup-devices" => rpc(&host, &token, Command::BackupDevices).await,
        // H10: on-demand snapshot of vault/state/TLS/intent repo.
        "backup-host-meta" => rpc(&host, &token, Command::BackupHostMeta).await,
        // Owner decision 2026-09-30 (item 2): CLI parity for the
        // dashboard's "Save and restart the host" — `homelab host
        // restart` schedules the same systemd restart of
        // homelab-host.service and waits the same way `release-update`
        // waits for the host to come back (no version to check, so it
        // only waits for it to go down and answer again).
        "host" => match args.get(2).map(|s| s.as_str()) {
            Some("restart") => {
                println!(
                    "{}▶ host restart :: restarting homelab-host.service{}",
                    C_CYAN, C_RESET
                );
                let ok = rpc_with(&host, &token, Command::RestartHost).await
                    && wait_for_updated_host(&host, &token, None).await;
                std::process::exit(if ok { 0 } else { 1 });
            }
            // fix-110 (homelab-admin, 2026-10-01): host settings declarative
            // like a stack — `config/host.toml` sent whole, the host keeps
            // its own secrets and writes it. The repository's path unless
            // one is given.
            Some("apply") => {
                let path = args
                    .get(3)
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| in_repo("config/host.toml"));
                let toml = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                    die(&format!(
                        "{} :: {e} — `homelab host apply` reads a repository's config/host.toml; \
                         run this inside the repository, or pass the path",
                        path.display()
                    ))
                });
                if let Err(e) = toml.parse::<toml::Table>() {
                    die(&format!("{} does not read as TOML: {e}", path.display()));
                }
                println!("{}▶ host apply :: {}{}", C_CYAN, path.display(), C_RESET);
                // The same optimistic-concurrency guard a dashboard save
                // uses: read host.toml's current sha256 first, so an edit
                // made meanwhile (over ssh, or a TUI save) is refused
                // rather than silently overwritten.
                let expect_sha256 = rpc_reply(&host, &token, Command::GetHostConfig)
                    .await
                    .filter(|r| r.ok)
                    .and_then(|r| {
                        serde_json::from_str::<homelab_proto::HostConfigFile>(&r.message).ok()
                    })
                    .map(|f| f.sha256);
                let r = rpc_reply(
                    &host,
                    &token,
                    Command::ApplyHostConfig {
                        toml,
                        expect_sha256,
                    },
                )
                .await;
                match r {
                    Some(r) if r.ok => {
                        let saved: homelab_proto::HostConfigSaved =
                            serde_json::from_str(&r.message).unwrap_or_default();
                        println!("{}✓ applied{}", C_GREEN, C_RESET);
                        if !saved.live.is_empty() {
                            println!("  live now: {}", saved.live.join(", "));
                        }
                        if !saved.restart.is_empty() {
                            println!(
                                "  takes effect at the host's next start: {} — `homelab host restart`",
                                saved.restart.join(", ")
                            );
                        }
                        std::process::exit(0);
                    }
                    Some(r) => die(&r.message),
                    None => die("the host did not answer"),
                }
            }
            other => die(&format!(
                "usage: homelab host restart | homelab host apply [path] (got {:?})",
                other.unwrap_or("nothing")
            )),
        },
        // G17: the questions only a person can answer. `homelab checks` lists
        // them with their ids; `homelab checks answer <id> ok|nok [note]`
        // records one. They used to be printed at the end of a deploy and
        // stored nowhere, which is not asking anybody anything.
        "checks" => match args.get(2).map(|s| s.as_str()) {
            None | Some("list") => {
                rpc(&host, &token, Command::ListManualChecks { json: false }).await
            }
            Some("answer") => {
                // fix-65 (nightly-report-always-red, 2026-09-27): one id
                // used to mean one command; a comma-separated list answers
                // several at once with the same verdict, note and (for
                // `accept`) days — the morning round of open checks no
                // longer needs one invocation each.
                let usage = "usage: homelab checks answer <id>[,<id>,...] ok|nok [note]";
                let ids =
                    homelab_client::checks::split_ids(args.get(3).unwrap_or_else(|| die(usage)));
                if ids.is_empty() {
                    die(usage);
                }
                let verdict = args.get(4).unwrap_or_else(|| die(usage));
                // fix-65: `accept <days> <reason>` records a deliberate nok
                // that is noted, not broken, until that many days from now.
                let (ok, note, accept_days) = if verdict == "accept" {
                    let usage = "usage: homelab checks answer <id>[,<id>,...] accept <days> \
                                  <reason>";
                    let days: u32 = args
                        .get(5)
                        .and_then(|d| d.parse().ok())
                        .filter(|d| *d > 0)
                        .unwrap_or_else(|| die(usage));
                    let reason = args.get(6..).map(|r| r.join(" ")).unwrap_or_default();
                    if reason.trim().is_empty() {
                        die(usage);
                    }
                    (false, reason, Some(days))
                } else {
                    let yes = ["ok", "yes", "ja"];
                    let no = ["nok", "no", "nee"];
                    let ok = if yes.contains(&verdict.as_str()) {
                        true
                    } else if no.contains(&verdict.as_str()) {
                        false
                    } else {
                        die(&format!(
                            "answer must be ok, nok or accept, not {}",
                            verdict
                        ))
                    };
                    (ok, args[5..].join(" "), None)
                };
                let mut all_ok = true;
                for id in &ids {
                    if ids.len() > 1 {
                        println!("{}▶ {}{}", C_CYAN, id, C_RESET);
                    }
                    let one_ok = rpc_with(
                        &host,
                        &token,
                        Command::AnswerManualCheck {
                            check_id: id.clone(),
                            ok,
                            note: note.clone(),
                            accept_days,
                        },
                    )
                    .await;
                    all_ok &= one_ok;
                }
                std::process::exit(if all_ok { 0 } else { 1 });
            }
            Some(other) => die(&format!(
                "unknown: homelab checks {} — try `list` or `answer`",
                other
            )),
        },
        // H8 (light): park / unpark a stack for the nightly scheduler.
        "enable" | "disable" => {
            let homelab_client::cli_args::Invocation::Enable { stack, enabled } = invocation(&args)
            else {
                die("internal: enable parsed as another verb")
            };
            let stack = homelab_client::repo_config::stack_name(&stack);
            rpc(&host, &token, Command::SetStackEnabled { stack, enabled }).await;
        }
        "export" => {
            // D11: single-file bundle, never secrets.
            let dir = stack_dir(
                args.get(2)
                    .unwrap_or_else(|| die("usage: homelab export stacks/<name> [out.yml]")),
            );
            let name = dir
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "stack".into());
            let out = args
                .get(3)
                .cloned()
                .unwrap_or_else(|| format!("{}-bundle.yml", name));
            match spec::export_bundle(&dir, &out) {
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
            match spec::import_bundle(Path::new(bundle), &stacks_base(), name, vmid) {
                Ok(dest) => {
                    // Validate what we just wrote with the same validator as deploy.
                    match spec::build_spec(&dest).and_then(|s| {
                        homelab_core::manifest::validate(&s).map_err(|e| e.to_string())
                    }) {
                        Ok(()) => println!(
                            "{}✓ imported{} — {} (vmid {}) :: add .env files if the apps need secrets, then deploy",
                            C_GREEN,
                            C_RESET,
                            dest.display(),
                            vmid
                        ),
                        Err(e) => die(&format!("imported but invalid: {}", e)),
                    }
                }
                Err(e) => die(&format!("import: {}", e)),
            }
        }
        "resize" => {
            // C4: apply the manifest's resources to the live container.
            let homelab_client::cli_args::Invocation::Resize { stack } = invocation(&args) else {
                die("internal: resize parsed as another verb")
            };
            let dir = &stack_dir(&stack);
            // gap-34: resize sends only the manifest (`ApplyResources`
            // takes `Box<StackManifest>`) — building the full spec ran
            // latch for any stack with `latch_secrets`/`latch_files` and
            // then threw every secret away unread.
            let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
            println!(
                "{}▶ resize {} :: {} MiB / {} cores / {}G{}",
                C_CYAN,
                manifest.stack_name,
                manifest.resources.memory_mb,
                manifest.resources.cores,
                manifest.resources.disk_gb,
                C_RESET
            );
            rpc(&host, &token, Command::ApplyResources(Box::new(manifest))).await;
        }
        "templates" => rpc(&host, &token, Command::ListTemplates).await,
        "template-build" => {
            // B8: bake the golden template. Temp vmid defaults to 999.
            // O2: `--privileged` builds the second template. CT 105 and 106
            // are privileged and a clone cannot change that, so they need one.
            // `--base <vztmpl>` picks the OS. Absent keeps the host's default,
            // so this stays the command it has always been for a caller that
            // does not care which Debian it is baking.
            // fix-110 (small-sharp-edges, 2026-09-27): an argument that does
            // not parse is refused; it used to become vmid 999 or version 1.
            let homelab_client::version::TemplateArgs {
                temp_vmid,
                version,
                unprivileged,
                base_template,
            } = homelab_client::version::template_build_args(&args[2..])
                .unwrap_or_else(|e| die(&e));
            let shown = base_template
                .as_deref()
                .map(homelab_core::ops::template::os_slug)
                .unwrap_or_else(|| "the host's default OS".into());
            println!(
                "{}▶ template build :: {}-homelab-v{}{} on temp vmid {}{}",
                C_CYAN,
                shown,
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
                    base_template,
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
            let presets_path = in_repo("presets");
            let presets_dir = presets_path.as_path();
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
            match homelab_client::scaffold::scaffold_stack(&stacks_base(), presets_dir, &params) {
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
            for pr in homelab_client::scaffold::scan_presets(&in_repo("presets")) {
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
        // fix-68: a human table by default; `--json` still gets the
        // `FleetState` as-is, for a script (previously raw `pct list` +
        // the whole of state.json — 1,473 lines, measured live).
        "status" => {
            let json = args.iter().any(|a| a == "--json");
            let reply = rpc_reply(&host, &token, Command::Status)
                .await
                .unwrap_or_else(|| die("the host did not answer"));
            if !reply.ok {
                die(&reply.message);
            }
            if json {
                println!("{}", reply.message);
            } else {
                match serde_json::from_str::<homelab_proto::FleetState>(&reply.message) {
                    Ok(fleet) => print!("{}", homelab_client::status::render_table(&fleet)),
                    Err(_) => println!("{}", reply.message),
                }
            }
        }
        "doctor" => rpc(&host, &token, Command::Doctor { json: false }).await,
        // fix-131: `incidents show <name>` reads one bundle; it took a root
        // shell on pve before.
        "incidents" => match (args.get(2).map(String::as_str), args.get(3)) {
            (None, _) => rpc(&host, &token, Command::Incidents { json: false }).await,
            (Some("show"), Some(name)) => {
                rpc(&host, &token, Command::IncidentShow { name: name.clone() }).await
            }
            (Some("show"), None) => {
                die("usage: homelab incidents show <name> (the names `homelab incidents` lists)")
            }
            (Some(other), _) => die(&format!(
                "unknown: homelab incidents {} :: `homelab incidents` lists, `homelab incidents \
                 show <name>` reads one",
                other
            )),
        },
        // fix-64 (restore-no-confirm-no-safety-snapshot, 2026-09-27):
        // `homelab restore` already takes a snapshot id; this is where to
        // read what ids exist without the dashboard. Reuses `GetBackups`
        // (feat-backup-1/2) unchanged — the same per-repository status,
        // with every snapshot, the Backups page and its restore picker
        // already read.
        "snapshots" => {
            let stack = homelab_client::repo_config::stack_name(
                args.get(2)
                    .unwrap_or_else(|| die("usage: homelab snapshots stacks/<name>")),
            );
            let json = args.iter().any(|a| a == "--json");
            let reply = rpc_reply(&host, &token, Command::GetBackups { stack })
                .await
                .unwrap_or_else(|| die("the host did not answer"));
            if !reply.ok {
                die(&reply.message);
            }
            if json {
                println!("{}", reply.message);
            } else {
                match serde_json::from_str::<homelab_client::snapshots::GetBackupsReply>(
                    &reply.message,
                ) {
                    Ok(r) => print!("{}", homelab_client::snapshots::render(r.native, &r.repos)),
                    Err(_) => println!("{}", reply.message),
                }
            }
        }
        // fix-120 (per-machine tokens, owner decision 2026-10-01): a token
        // per machine, so one can be revoked without touching the others.
        "token" => match args.get(2).map(String::as_str) {
            Some("issue") => {
                let name = args.get(3).cloned().unwrap_or_else(|| {
                    die(
                        "usage: homelab token issue <name> <read|operate|all> :: name is what \
                         `homelab doctor` and audit.log will call this machine",
                    )
                });
                let scope = match args.get(4).map(String::as_str) {
                    Some("read") => homelab_proto::Scope::Read,
                    Some("operate") => homelab_proto::Scope::Operate,
                    Some("all") => homelab_proto::Scope::All,
                    _ => die(
                        "usage: homelab token issue <name> <read|operate|all> :: read = look, \
                         operate = act without destroying, all = everything",
                    ),
                };
                match rpc_reply(&host, &token, Command::TokenIssue { name, scope }).await {
                    Some(r) if r.ok => {
                        match serde_json::from_str::<homelab_proto::TokenIssued>(&r.message) {
                            Ok(issued) => {
                                println!(
                                    "issued {:?} (scope {:?}) — shown once, save it now:",
                                    issued.name, issued.scope
                                );
                                println!("{}", issued.token);
                                println!(
                                    "set it as HOMELAB_TOKEN on that machine (its own .env or \
                                     ~/.config/homelab/env), never shared with another machine's"
                                );
                            }
                            Err(e) => die(&format!("issued, but could not read the reply: {}", e)),
                        }
                    }
                    Some(r) => die(&r.message),
                    None => die("the host did not answer"),
                }
            }
            Some("list") => match rpc_reply(&host, &token, Command::TokenList).await {
                Some(r) if r.ok => {
                    match serde_json::from_str::<Vec<homelab_proto::TokenView>>(&r.message) {
                        Ok(views) => {
                            if views.is_empty() {
                                println!("no tokens");
                            }
                            for v in views {
                                println!("{:<20} {:?}", v.name, v.scope);
                            }
                        }
                        Err(e) => die(&format!("could not read the reply: {}", e)),
                    }
                }
                Some(r) => die(&r.message),
                None => die("the host did not answer"),
            },
            Some("revoke") => {
                let name = args.get(3).cloned().unwrap_or_else(|| {
                    die("usage: homelab token revoke <name> :: `homelab token list` shows the names")
                });
                rpc(&host, &token, Command::TokenRevoke { name }).await
            }
            _ => die(
                "usage: homelab token issue <name> <read|operate|all> | homelab token list | \
                 homelab token revoke <name>",
            ),
        },
        "plan" => {
            // D6/D10: validate locally first — this much never needs the
            // network, and a bad stack file fails before anything is asked
            // of the host.
            let dir = &stack_dir(
                args.get(2)
                    .unwrap_or_else(|| die("usage: homelab plan stacks/<name>")),
            );
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            check_fleet_routes(Path::new(dir).parent().unwrap_or(Path::new(".")));
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
            // fix-100 (apply-no-confirm-creates-drill, 2026-09-27): the
            // per-file diff this verb never had, now that `apply` has one
            // for every stack at once — this reads one stack's, with
            // HOMELAB_TOKEN set (D10's "no network" promise is kept when
            // it is not: validation above already ran and stands).
            if token.is_empty() {
                println!(
                    "{}  (per-file diff needs HOMELAB_TOKEN — validation above stands on its \
                     own){}",
                    C_DIM, C_RESET
                );
            } else {
                match rpc_reply(
                    &host,
                    &token,
                    Command::GetApplied {
                        stack: spec.manifest.stack_name.clone(),
                    },
                )
                .await
                {
                    Some(r) if r.ok => {
                        match serde_json::from_str::<Vec<homelab_proto::FileBlob>>(&r.message) {
                            Ok(applied) => {
                                let changes =
                                    homelab_client::apply::file_changes(&spec.files, &applied);
                                if changes.is_empty() {
                                    println!(
                                        "{}  files unchanged (secrets or settings differ){}",
                                        C_DIM, C_RESET
                                    );
                                }
                                for c in changes {
                                    println!("  {}", c);
                                }
                                for r in
                                    homelab_client::apply::native_restarts(&spec.files, &applied)
                                {
                                    println!("{}  ↻ {}{}", C_YELLOW, r, C_RESET);
                                }
                            }
                            Err(_) => println!(
                                "{}  (could not read what the host applied — the host answered: \
                                 {}){}",
                                C_DIM, r.message, C_RESET
                            ),
                        }
                    }
                    Some(_not_ok) => println!(
                        "{}  new: the host has never applied {} — the deploy would create CT \
                         {}{}",
                        C_DIM, spec.manifest.stack_name, spec.manifest.vmid, C_RESET
                    ),
                    None => println!(
                        "{}  (the host did not answer — per-file diff skipped, validation above \
                         stands){}",
                        C_DIM, C_RESET
                    ),
                }
            }
        }
        "deploy" => {
            let homelab_client::cli_args::Invocation::Deploy { stack, force } = invocation(&args)
            else {
                die("internal: deploy parsed as another verb")
            };
            let dir = &stack_dir(&stack);
            let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
            // D10: fail fast client-side before opening a connection.
            if let Err(e) = homelab_core::manifest::validate(&spec) {
                die(&format!("validation failed: {}", e));
            }
            check_fleet_routes(Path::new(dir).parent().unwrap_or(Path::new(".")));
            // arch-deploy-guard: not over a deploy this tree has not seen.
            if !force {
                let (ok, fleet) = rpc_collect(&host, &token, Command::GetState).await;
                if let (true, Some(fleet)) = (ok, fleet)
                    && let Err(e) =
                        deploy_guard(Path::new(dir), &spec.manifest.stack_name, &fleet, false)
                {
                    die(&e);
                }
            }
            let ok = deploy_spec(&host, &token, spec).await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        // ask-8 (Kenny, 2026-09-27): the whole stacks directory against the
        // host. Changed stacks are deployed; a stack in host state whose
        // directory is gone is destroyed only after its name is typed. The
        // nightly round never does this — it never destroys anything.
        "apply" => {
            let base = args
                .get(2)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .unwrap_or_else(|| stacks_base().display().to_string());
            let skip_backup = args.iter().any(|a| a == "--no-backup");
            // fix-100 (apply-no-confirm-creates-drill, 2026-09-27): the plan
            // used to be printed and deployed in the same breath.
            let dry_run = args.iter().any(|a| a == "--dry-run");
            let assume_yes = args.iter().any(|a| a == "--yes");
            let plan_only = args.iter().any(|a| a == "--plan");
            let base_path = Path::new(&base);
            if !base_path.is_dir() {
                die(&format!(
                    "no stacks directory at '{}' — run this from the repository root, or pass \
                     the path: homelab apply ~/Projects/homelab/stacks",
                    base
                ));
            }
            check_fleet_routes(base_path);
            let (ok, fleet) = rpc_collect(&host, &token, Command::GetState).await;
            let fleet = match fleet {
                Some(f) if ok => f,
                _ => die("could not read the host's state — nothing applied"),
            };
            let host_pairs: Vec<(String, String)> = fleet
                .stacks
                .iter()
                .map(|s| (s.name.clone(), s.applied_hash.clone()))
                .collect();
            // Every stack is built and validated before anything is sent: a
            // stack file that does not build stops the whole apply rather
            // than leaving the fleet half-applied.
            let mut local: Vec<(String, String)> = Vec::new();
            let mut specs: std::collections::BTreeMap<String, homelab_proto::DeploySpec> =
                std::collections::BTreeMap::new();
            let declared = spec::declared_stacks(base_path);
            let ephemeral: Vec<String> = spec::scan_local_stacks(base_path)
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| !declared.iter().any(|(d, _)| d == n))
                .collect();
            for (name, dir) in declared {
                let sp = spec::build_spec(&dir)
                    .unwrap_or_else(|e| die(&format!("{}: {} — nothing applied", name, e)));
                if let Err(e) = homelab_core::manifest::validate(&sp) {
                    die(&format!(
                        "{}: validation failed: {} — nothing applied",
                        name, e
                    ));
                }
                local.push((name.clone(), homelab_core::manifest::intent_hash(&sp)));
                specs.insert(name, sp);
            }
            let dirs: Vec<String> = std::fs::read_dir(base_path)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default();
            let plan = homelab_client::apply::plan(&local, &dirs, &host_pairs);
            println!(
                "{}▶ apply :: {} to deploy · {} unchanged · {} gone from the files{}",
                C_CYAN,
                plan.deploy.len(),
                plan.unchanged.len(),
                plan.destroy.len(),
                C_RESET
            );
            for n in &plan.unchanged {
                println!("{}  = {}{}", C_DIM, n, C_RESET);
            }
            for n in &ephemeral {
                println!(
                    "{}  · {} — ephemeral: deployed by name only, never by apply{}",
                    C_DIM, n, C_RESET
                );
            }
            for n in &plan.deploy {
                println!("  ↑ {}", n);
                // fix-100: what the deploy changes, file by file, removals
                // included — the host's applied files against the local ones.
                let Some(sp) = specs.get(n) else { continue };
                if !host_pairs.iter().any(|(h, _)| h == n) {
                    println!(
                        "{}      new: creates CT {}{}",
                        C_DIM, sp.manifest.vmid, C_RESET
                    );
                    continue;
                }
                let applied: Vec<homelab_proto::FileBlob> =
                    rpc_reply(&host, &token, Command::GetApplied { stack: n.clone() })
                        .await
                        .filter(|r| r.ok)
                        .and_then(|r| serde_json::from_str(&r.message).ok())
                        .unwrap_or_else(|| {
                            die(&format!(
                                "could not read what the host applied for {} — nothing applied",
                                n
                            ))
                        });
                let changes = homelab_client::apply::file_changes(&sp.files, &applied);
                if changes.is_empty() {
                    println!(
                        "{}      files unchanged (secrets or settings differ){}",
                        C_DIM, C_RESET
                    );
                }
                for c in changes {
                    println!("      {}", c);
                }
                // fix-159: which running native units the deploy restarts.
                for r in homelab_client::apply::native_restarts(&sp.files, &applied) {
                    println!("{}      ↻ {}{}", C_YELLOW, r, C_RESET);
                }
            }
            for n in &plan.destroy {
                println!(
                    "{}  ✗ {} — in host state, no {}/{}/{}",
                    C_YELLOW, n, base, n, C_RESET
                );
            }
            // fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift):
            // the plan alone, with an exit code a script can test.
            if plan_only {
                std::process::exit(homelab_client::apply::plan_exit_code(&plan));
            }
            let answer = if dry_run || assume_yes || plan.deploy.is_empty() {
                None
            } else {
                use std::io::Write as _;
                eprint!("Deploy these {} stack(s)? [y/N] ", plan.deploy.len());
                std::io::stderr().flush().ok();
                let mut line = String::new();
                std::io::stdin()
                    .read_line(&mut line)
                    .ok()
                    .filter(|n| *n > 0)
                    .map(|_| line)
            };
            match homelab_client::apply::decide(
                dry_run,
                assume_yes || plan.deploy.is_empty(),
                answer.as_deref(),
            ) {
                homelab_client::apply::Decision::Preview => {
                    println!("dry run — nothing deployed, nothing destroyed");
                    std::process::exit(0);
                }
                homelab_client::apply::Decision::Decline => {
                    die("not confirmed — nothing deployed, nothing destroyed")
                }
                homelab_client::apply::Decision::Deploy => {}
            }
            // arch-deploy-guard: every planned stack is checked before the
            // first one is sent, so a refusal leaves nothing half-applied.
            let force = args.iter().any(|a| a == "--force");
            for name in &plan.deploy {
                if let Err(e) = deploy_guard(&base_path.join(name), name, &fleet, force) {
                    die(&e);
                }
            }
            for name in &plan.deploy {
                let Some(sp) = specs.remove(name) else {
                    continue;
                };
                if !deploy_spec(&host, &token, sp).await {
                    die(&format!(
                        "deploy of {} failed — apply stopped here; nothing after it was \
                         deployed and nothing was destroyed",
                        name
                    ));
                }
            }
            let mut all_ok = true;
            if !plan.destroy.is_empty() {
                println!(
                    "{}! {} stack(s) run on the host but are gone from the files. Each is \
                     destroyed from the manifest the host recorded — backed up first{}, its \
                     /appdata, backups and vault kept (see `homelab check`) — only after you \
                     type its name. Enter keeps it.{}",
                    C_YELLOW,
                    plan.destroy.len(),
                    if skip_backup {
                        " (NOT: --no-backup)"
                    } else {
                        ""
                    },
                    C_RESET
                );
                for name in &plan.destroy {
                    let typed =
                        read_typed(&format!("Type the stack name '{}' to destroy it: ", name));
                    if &typed != name {
                        println!("  kept {} — nothing destroyed", name);
                        continue;
                    }
                    all_ok &= rpc_with(
                        &host,
                        &token,
                        Command::DestroyRecorded {
                            stack: name.clone(),
                            confirm: typed,
                            skip_backup,
                        },
                    )
                    .await;
                }
            }
            std::process::exit(if all_ok { 0 } else { 1 });
        }
        // ask-9: delete what a retired stack, app or unit kept — never
        // automatic, always after the list and the typed name.
        "wipe" => {
            let homelab_client::cli_args::Invocation::Wipe { name, yes } = invocation(&args) else {
                die("internal: wipe parsed as another verb")
            };
            let name = homelab_client::repo_config::stack_name(&name);
            let listed = rpc_with(
                &host,
                &token,
                Command::WipeRetired {
                    name: name.clone(),
                    confirm: None,
                },
            )
            .await;
            if !listed {
                std::process::exit(1);
            }
            // cli-yes: a line copied from the dashboard's form, where the
            // name was already typed, carries --yes.
            let typed = if yes {
                name.clone()
            } else {
                read_typed(&format!(
                    "Type '{}' to delete all of the above, permanently: ",
                    name
                ))
            };
            if typed != name {
                die("name mismatch — nothing deleted");
            }
            rpc(
                &host,
                &token,
                Command::WipeRetired {
                    name,
                    confirm: Some(typed),
                },
            )
            .await;
        }
        "backup" => {
            let homelab_client::cli_args::Invocation::Backup { stack } = invocation(&args) else {
                die("internal: backup parsed as another verb")
            };
            let dir = &stack_dir(&stack);
            // F291: the manifest alone. A backup runs on the host against
            // /appdata paths and needs no file and no secret from here.
            let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
            rpc(&host, &token, Command::BackupStack(Box::new(manifest))).await;
        }
        "restore" => {
            // fix-64: flags may stand anywhere; the rest is dir then snapshot.
            // fix-112: `--app <name>` restores one app of the stack.
            let homelab_client::cli_args::Invocation::Restore(homelab_client::RestoreArgs {
                dir,
                snapshot,
                app,
                yes,
                skip_safety_copy,
            }) = invocation(&args)
            else {
                die("internal: restore parsed as another verb")
            };
            // fix-101: every spelling of a stack names the same directory.
            let dir = &stack_dir(&dir);
            // F294: the manifest alone, for the same reason as F291 above.
            let manifest = spec::build_manifest(Path::new(&dir)).unwrap_or_else(|e| die(&e));
            let stack = manifest.stack_name.clone();
            println!(
                "{}▶ restore {}{} from '{}'{}",
                C_YELLOW,
                stack,
                app.as_deref()
                    .map(|a| format!(" :: {}", a))
                    .unwrap_or_default(),
                snapshot,
                C_RESET
            );
            // fix-64: a restore overwrites live data, so the command line asks
            // for the name the way the TUI always did; `--yes` answers it
            // for scripts. The host refuses a request without it.
            let confirm = if yes {
                stack.clone()
            } else {
                let typed = read_typed(&format!(
                    "This overwrites the live data of '{}'{}. Type the stack name to confirm: ",
                    stack,
                    if skip_safety_copy {
                        ", WITHOUT a copy of it (--no-safety-copy)"
                    } else {
                        " (a copy of it is kept on the host first)"
                    }
                ));
                if typed != stack {
                    die("name mismatch — nothing was restored");
                }
                typed
            };
            rpc(
                &host,
                &token,
                Command::RestoreStack {
                    manifest: Box::new(manifest),
                    snapshot,
                    confirm: Some(confirm),
                    skip_safety_copy,
                    app,
                },
            )
            .await;
        }
        "update" => {
            let homelab_client::cli_args::Invocation::Update { stack, app } = invocation(&args)
            else {
                die("internal: update parsed as another verb")
            };
            let dir = &stack_dir(&stack);
            // F294: an update pulls an image and recreates a container on the
            // host; the files and secrets it used to build here were thrown
            // away one line later. Demanding them meant a machine without the
            // latch key could not update a stack whose secrets had not
            // changed — the same fault F291 closed for `backup`, still open in
            // the two verbs beside it.
            let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
            println!(
                "{}▶ update {} :: {}{}",
                C_CYAN,
                manifest.stack_name,
                app.as_deref().unwrap_or("all apps"),
                C_RESET
            );
            rpc(
                &host,
                &token,
                Command::UpdateStack {
                    manifest: Box::new(manifest),
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
                        "{}✓ signature and checksum verified — shipping over the line{}",
                        C_GREEN, C_RESET
                    );
                    // fix-121: done means the shipped version answered, not
                    // that a restart was scheduled.
                    let expected = homelab_client::release::expected_host_version(&tag);
                    let ok = rpc_with(&host, &token, Command::SelfUpdateHost { binary_b64 }).await
                        && wait_for_updated_host(&host, &token, expected).await;
                    std::process::exit(if ok { 0 } else { 1 });
                }
                Err(e) => die(&e),
            }
        }
        // fix-105 (older-client-no-warning, 2026-09-27): the client updates
        // itself the way `release-update` updates the host — the release's
        // own binary, checksum-verified, then put where this one runs from.
        // It was a five-line gh/sha256sum/install routine per workstation.
        "self-install" => {
            let tag = match args.get(2).cloned() {
                Some(t) => t,
                None => homelab_client::release::latest_release_tag()
                    .unwrap_or_else(|| die("no release found (gh authenticated? release exists?)")),
            };
            let target = std::env::current_exe().unwrap_or_else(|e| {
                die(&format!("cannot tell where this client runs from: {}", e))
            });
            println!(
                "{}▶ self-install :: client {} from GitHub into {}{}",
                C_CYAN,
                tag,
                target.display(),
                C_RESET
            );
            let bytes = homelab_client::release::stage_client(&tag).unwrap_or_else(|e| die(&e));
            println!("{}✓ signature and checksum verified{}", C_GREEN, C_RESET);
            homelab_client::release::install_binary(&bytes, &target).unwrap_or_else(|e| die(&e));
            println!(
                "{}✓ client {} installed{} — the next command runs it",
                C_GREEN, tag, C_RESET
            );
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
            // fix-121: the version inside a local file is not known here, so
            // whatever answers after the restart is the one checked.
            let ok = rpc_with(&host, &token, Command::SelfUpdateHost { binary_b64 }).await
                && wait_for_updated_host(&host, &token, None).await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        // Phase 7's output document, derived from the tests rather than kept
        // beside them — the same reasoning as `runbook`.
        "testplan" => {
            let out = args.get(2).cloned().unwrap_or_else(|| {
                in_repo("docs/deployment/TEST_PLAN.md")
                    .display()
                    .to_string()
            });
            match homelab_client::testplan::generate_test_plan(
                &[&in_repo("core/tests"), &in_repo("client/tests")],
                &in_repo("docs/deployment/REALIZATION_PLAN.md"),
                &stacks_base(),
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
                .unwrap_or_else(|| in_repo("docs/DR_RUNBOOK.md").display().to_string());
            match spec::generate_runbook(&stacks_base(), &out) {
                Ok(n) => println!(
                    "{}✓ runbook written{} — {} ({} stack(s))",
                    C_GREEN, C_RESET, out, n
                ),
                Err(e) => die(&format!("runbook: {}", e)),
            }
        }
        // fix-144 (expert panel 2026-09-27, update-policy-doc-drift): the
        // policy table in UPDATE_POLICY.md is written from the stack files,
        // like the runbook, and a test fails when the committed one is stale.
        "update-policy" => {
            let out = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "docs/deployment/UPDATE_POLICY.md".into());
            match homelab_client::updatepolicy::write_update_policy(
                Path::new("stacks"),
                Path::new(&out),
            ) {
                Ok(n) => println!(
                    "{}✓ update policy written{} — {} ({} row(s))",
                    C_GREEN, C_RESET, out, n
                ),
                Err(e) => die(&format!("update-policy: {}", e)),
            }
        }
        "prune-orphans" => {
            // Kenny's H2b made this the only remover of files the repository
            // no longer has; since ask-8 (2026-09-27) the deploy removes them
            // itself, so this is mostly a no-op kept for a container that has
            // not been deployed since. Same typed confirmation as before.
            let homelab_client::cli_args::Invocation::PruneOrphans { stack, yes } =
                invocation(&args)
            else {
                die("internal: prune-orphans parsed as another verb")
            };
            let dir = &stack_dir(&stack);
            // gap-34: `orphan_files_keeping` (core/src/ops/deploy.rs) reads
            // only `spec.files` — never `.env` or `.secret_files` — so
            // latch never needed to run here. `build_spec` ran it anyway
            // for any stack with `latch_secrets`/`latch_files`.
            let spec = spec::build_spec_files_only(Path::new(dir)).unwrap_or_else(|e| die(&e));
            let stack = spec.manifest.stack_name.clone();
            let confirm = if yes {
                stack.clone()
            } else {
                eprint!(
                    "{}Type the stack name '{}' to remove the files the repository no longer \
                     has (the deploy log lists them): {}",
                    C_RED, stack, C_RESET
                );
                use std::io::Write as _;
                std::io::stderr().flush().ok();
                let mut typed = String::new();
                std::io::stdin().read_line(&mut typed).ok();
                typed.trim().to_string()
            };
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
            let homelab_client::cli_args::Invocation::Destroy {
                stack,
                skip_backup,
                yes,
            } = invocation(&args)
            else {
                die("internal: destroy parsed as another verb")
            };
            let dir = &stack_dir(&stack);
            // ask-8: a stack whose directory is gone is destroyed from the
            // manifest the host recorded when it last applied it.
            if !Path::new(dir).join("lxc-compose.yml").exists() {
                let stack = Path::new(dir)
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                eprintln!(
                    "{}! no {}/lxc-compose.yml — destroying '{}' from the manifest the host \
                     recorded{}",
                    C_YELLOW,
                    dir.display(),
                    stack,
                    C_RESET
                );
                let confirm = if yes {
                    stack.clone()
                } else {
                    read_typed(&format!(
                        "Type the stack name '{}' to confirm destroy: ",
                        stack
                    ))
                };
                if confirm != stack {
                    die("name mismatch — aborted");
                }
                rpc(
                    &host,
                    &token,
                    Command::DestroyRecorded {
                        stack,
                        confirm,
                        skip_backup,
                    },
                )
                .await;
            }
            // gap-34: `DestroyStack` sends only the manifest — building
            // the full spec ran latch for any stack with
            // `latch_secrets`/`latch_files` and then threw every secret
            // away unread (F291 made the same point for `backup`).
            let manifest = spec::build_manifest(Path::new(dir)).unwrap_or_else(|e| die(&e));
            let stack = &manifest.stack_name;
            // Kenny's B2: the destroy backs up first and refuses if that
            // fails. Skipping is deliberate and says so out loud.
            if skip_backup {
                eprintln!(
                    "{}! --no-backup: destroying without the backup that would otherwise be \
                     taken first{}",
                    C_YELLOW, C_RESET
                );
            }
            // C2: typed-name confirmation, exactly like the TUI; cli-yes:
            // a line copied from the dashboard's form, where the name was
            // typed already, carries --yes.
            let confirm = if yes {
                stack.clone()
            } else {
                eprint!(
                    "{}Type the stack name '{}' to confirm destroy: {}",
                    C_RED, stack, C_RESET
                );
                use std::io::Write as _;
                std::io::stderr().flush().ok();
                let mut typed = String::new();
                std::io::stdin().read_line(&mut typed).ok();
                typed.trim().to_string()
            };
            if &confirm != stack {
                die("name mismatch — aborted");
            }
            rpc(
                &host,
                &token,
                Command::DestroyStack {
                    manifest: Box::new(manifest),
                    confirm,
                    skip_backup,
                },
            )
            .await;
        }
        // fix-108: `help` (or no arguments). An unknown verb never reaches
        // here: it is refused before the token is read.
        _ => print!("{}", homelab_client::cli_help::usage()),
    }
}

async fn rpc(host: &str, token: &str, command: Command) {
    let ok = rpc_with(host, token, command).await;
    std::process::exit(if ok { 0 } else { 1 });
}

/// The version a host announces in its Hello, or None when it cannot be
/// reached or says nothing within a few seconds.
async fn host_version(host: &str, token: &str) -> Option<String> {
    let link = homelab_client::link::connect(
        host,
        token,
        REPO_PIN.get().and_then(|p| p.as_deref()),
        homelab_client::repo_config::built_in_pin(),
    )
    .await
    .ok()?;
    let (_, mut rx) = link.ws.split();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(Message::Text(text))) = rx.next().await {
            if let Ok(ServerMsg::Hello { version, .. }) = serde_json::from_str(&text) {
                return Some(version);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// fix-121 (expert panel, self-update-acceptance-weak, 2026-09-27): after a
/// self-update, watch the host come back. `true` once the shipped version
/// (`expected`; any restarted daemon when unknown) has answered a ping, which
/// is also what accepts the update on the host; `false` when the old version
/// came back (the rollback ran) or nothing answered in time. It used to
/// return as soon as the restart was scheduled, and a rollback was visible
/// only in a notification.
async fn wait_for_updated_host(host: &str, token: &str, expected: Option<String>) -> bool {
    use homelab_client::release::{AfterUpdate, after_update};
    // The old daemon may finish a running operation first (up to 60 s,
    // fix-52), then the restart and a possible rollback follow.
    const WAIT_S: u64 = 150;
    println!(
        "{}… waiting up to {} s for the host to come back{}{}",
        C_DIM,
        WAIT_S,
        expected
            .as_deref()
            .map(|v| format!(" as v{}", v))
            .unwrap_or_default(),
        C_RESET
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(WAIT_S);
    let mut seen_down = false;
    let mut last: Option<String> = None;
    while std::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let answered = host_version(host, token).await;
        if answered.is_none() {
            seen_down = true;
        } else {
            last = answered.clone();
        }
        match after_update(expected.as_deref(), seen_down, answered.as_deref()) {
            AfterUpdate::Wait => continue,
            AfterUpdate::Answered => {
                // The round trip that accepts the update on the host.
                let ok = rpc_with(host, token, Command::Ping).await;
                if ok {
                    println!(
                        "{}✓ self-update accepted — the new host answered{}",
                        C_GREEN, C_RESET
                    );
                }
                return ok;
            }
            AfterUpdate::RolledBack(v) => {
                eprintln!(
                    "{}✗ the host came back as v{}, not v{}: the self-update was rolled back \
                     :: `journalctl -u homelab-host` on pve says why the new binary failed{}",
                    C_RED,
                    v,
                    expected.as_deref().unwrap_or("?"),
                    C_RESET
                );
                return false;
            }
        }
    }
    eprintln!(
        "{}✗ no confirmation within {} s (last answer: {}) :: the update is not accepted \
         until the new daemon answers a request; `homelab ping` retries that, \
         `journalctl -u homelab-host` on pve shows what it is doing{}",
        C_RED,
        WAIT_S,
        last.map(|v| format!("v{}", v))
            .unwrap_or_else(|| "none".into()),
        C_RESET
    );
    false
}

/// One command over the link; `true` when the host reported it done and ok.
/// T85 needed a caller that sends several commands in a row (one staged
/// binary per message, then the deploy), so the exit moved to `rpc`.
async fn rpc_with(host: &str, token: &str, command: Command) -> bool {
    rpc_collect(host, token, command).await.0
}

/// [`rpc_with`], also handing back the fleet snapshot a `GetState` answers
/// with (ask-8: `homelab apply` compares the stacks directory with it).
async fn rpc_collect(
    host: &str,
    token: &str,
    command: Command,
) -> (bool, Option<homelab_proto::FleetState>) {
    let (ok, fleet, _) = rpc_exchange(host, token, command, true).await;
    (ok, fleet)
}

/// A `homelab ui` answer printed, and the process ended with its code.
fn ui_print(reply: &homelab_proto::RpcResponse, json: bool) -> ! {
    let failed = FINISH_FAILED.load(std::sync::atomic::Ordering::Relaxed);
    if json {
        println!("{}", reply.message);
        std::process::exit(if reply.ok && !failed { 0 } else { 1 });
    }
    match homelab_client::ui_cli::render(&reply.message) {
        Ok(text) => {
            print!("{text}");
            std::process::exit(if failed { 1 } else { 0 });
        }
        Err(text) => {
            eprint!("{text}");
            std::process::exit(1);
        }
    }
}

/// `homelab ui finish` (Kenny, 2026-09-29): read the screen every 2 s until
/// the open dialog's job has ended, print its outcome, then answer with the
/// `done` step to send, which closes the dialog and gives the tabs back at
/// once instead of after fix-163's 30 s. A failed job still lets go, and
/// the process then ends with 1 (after `done` answered).
async fn ui_finish(host: &str, token: &str, json: bool) -> UiStep {
    use homelab_client::ui_cli::{FinishNext, finish_next};
    let mut last = String::new();
    let mut misses = 0;
    loop {
        let Some(reply) = rpc_reply(
            host,
            token,
            Command::Ui {
                step: UiStep::State,
            },
        )
        .await
        else {
            // A job that restarts the dashboard drops the line for a moment.
            misses += 1;
            if misses >= 30 {
                die(
                    "ui finish: the host did not answer for a minute; `homelab ui state` shows where it is",
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        };
        misses = 0;
        match finish_next(&reply.message).unwrap_or_else(|e| die(&e)) {
            FinishNext::Wait(line) => {
                if !json && line != last {
                    println!("{line}");
                }
                last = line;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            FinishNext::NoJob(why) => die(&format!("ui finish: {why}")),
            FinishNext::Stopped(why) => die(&format!("ui finish: {why}")),
            FinishNext::Release { outcome, failed } => {
                if let Some(o) = outcome.filter(|_| !json) {
                    println!("{o}");
                }
                if failed {
                    FINISH_FAILED.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                return UiStep::Done;
            }
        }
    }
}

/// `ui finish` saw the job end in something other than success: exit 1
/// once `done` has let go.
static FINISH_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// fix-68: the host's reply handed back instead of printed, for a verb whose
/// reply is data to render (`today`).
async fn rpc_reply(
    host: &str,
    token: &str,
    command: Command,
) -> Option<homelab_proto::RpcResponse> {
    rpc_exchange(host, token, command, false).await.2
}

async fn rpc_exchange(
    host: &str,
    token: &str,
    command: Command,
    echo_reply: bool,
) -> (
    bool,
    Option<homelab_proto::FleetState>,
    Option<homelab_proto::RpcResponse>,
) {
    use std::io::IsTerminal as _;
    let mut fleet_seen: Option<homelab_proto::FleetState> = None;
    // F303: the size guard lives HERE, where every command passes, and not in
    // the three call sites that happened to remember it.
    //
    // `too_large` was written after `release-update` answered "Connection
    // reset by peer" for a host binary 132 KB past the old ceiling — its own
    // comment says so. It was then wired into the verbs that ship the host
    // binary, and nowhere else. On 2026-09-09 `deploy stacks/kyu` built a
    // 94.7 MiB payload (three service binaries at once) and got the identical
    // reset, with the identical nothing to read. Same fault, one caller over,
    // because the fix had been fitted to the place it was found rather than
    // to the property: any message can outgrow the link.
    if let Some(why) = serde_json::to_vec(&command)
        .ok()
        .and_then(|v| homelab_client::version::too_large(v.len()))
    {
        die(&why);
    }
    // Commands whose real payload arrives as a separate broadcast frame
    // (Config) may see RpcDone first — wait for the payload before exiting.
    let awaits_payload = matches!(command, Command::GetConfig | Command::GetState);
    let is_ping = matches!(command, Command::Ping);
    // A UI step prints only its own answer and notes, never the host's
    // broadcasts (log lines, transfers, the fleet snapshot, questions).
    let quiet = homelab_client::ui_cli::quiet_line(&command);
    // fix-141: ping and status also name the client's own build.
    let names_builds = matches!(command, Command::Ping | Command::Status);
    let mut payload_seen = false;
    let mut done: Option<bool> = None;
    // fix-66 (host-questions-unanswerable, 2026-09-27): the CLI could not
    // answer a question an operation started from it raised — "no answer
    // from here: run this from the TUI to decide" was the whole story.
    // Answers now run beside the host's queue (that part is built); this is
    // the client side of actually sending one. `PRE_ANSWER` (`--answer
    // allow|stop`, read once in `run`) answers every question without
    // waiting; otherwise, on a terminal, the same `[a] allow / [s] stop`
    // choice the TUI's focus window offers is asked here. Not a terminal
    // and no pre-answer: unchanged — printed, and left to time out as
    // Unattended. Each answer sent here gets its own RpcDone, which would
    // otherwise be read as the whole exchange's result; `answer_pending`
    // tells those apart from the command's own reply.
    let mut answer_pending: u32 = 0;
    // fix-67: the pin, the frame ceiling and the version gate live in one
    // place, shared with the TUI, whose own copy had drifted from this one.
    let link = homelab_client::link::connect(
        host,
        token,
        REPO_PIN.get().and_then(|p| p.as_deref()),
        homelab_client::repo_config::built_in_pin(),
    )
    .await
    .unwrap_or_else(|e| die(&e));
    match &link.pinned {
        Some(
            homelab_client::link::Pinned::BuiltIn(_) | homelab_client::link::Pinned::FromRepo(_),
        ) if quiet => {}
        // fix-149: nothing trusted on first use; say where the pin came from.
        Some(homelab_client::link::Pinned::BuiltIn(fp)) => eprintln!(
            "{}● pinned host certificate SHA256:{}, the one this client was built with{}",
            C_YELLOW, fp, C_RESET
        ),
        Some(homelab_client::link::Pinned::FromRepo(fp)) => eprintln!(
            "{}● pinned host certificate SHA256:{} from {}{}",
            C_YELLOW,
            fp,
            homelab_client::repo_config::REPO_FILE,
            C_RESET
        ),
        Some(homelab_client::link::Pinned::FirstUse(fp)) => {
            eprintln!(
                "{}● pinned host certificate SHA256:{}{}",
                C_YELLOW, fp, C_RESET
            );
            eprintln!(
                "{}  verify this matches the fingerprint the host printed at boot{}",
                C_DIM, C_RESET
            );
        }
        None => {}
    }
    let (mut tx, mut rx) = link.ws.split();

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
            ServerMsg::Ask { .. } if quiet => {}
            ServerMsg::Ask {
                id,
                op,
                step,
                what,
                if_allowed,
                if_stopped,
                boot,
            } => {
                eprintln!(
                    "{}? {} :: {} is waiting for a decision — {}{}",
                    C_YELLOW, op, step, what, C_RESET
                );
                let pre_answer = PRE_ANSWER.get().copied().flatten();
                let allow = match homelab_client::answer::choose(
                    pre_answer,
                    std::io::stdin().is_terminal(),
                ) {
                    homelab_client::answer::AnswerChoice::PreAnswered(allow) => {
                        eprintln!(
                            "  --answer {} :: {}",
                            if allow { "allow" } else { "stop" },
                            if allow { &if_allowed } else { &if_stopped }
                        );
                        Some(allow)
                    }
                    homelab_client::answer::AnswerChoice::Prompt => {
                        eprintln!("    [a] allow — {}", if_allowed);
                        eprintln!("    [s] stop  — {}", if_stopped);
                        loop {
                            match homelab_client::answer::parse_prompt_answer(&read_typed("  ")) {
                                Some(allow) => break Some(allow),
                                None => eprintln!("  type 'a' or 's'"),
                            }
                        }
                    }
                    homelab_client::answer::AnswerChoice::TimesOut => None,
                };
                match allow {
                    Some(allow) => {
                        let answer = RpcRequest {
                            id: req.id + answer_pending as u64 + 1,
                            command: Command::Answer { id, allow, boot },
                        };
                        if tx
                            .send(Message::Text(
                                serde_json::to_string(&answer).unwrap().into(),
                            ))
                            .await
                            .is_ok()
                        {
                            answer_pending += 1;
                        }
                    }
                    None => eprintln!(
                        "  no answer from here: run this from the TUI to decide, \
                         `homelab <verb> --answer allow|stop` answers without waiting, \
                         or it times out as unattended"
                    ),
                }
            }
            ServerMsg::Hello {
                version,
                proto,
                build,
            } => {
                // fix-141 (expert panel 2026-09-27,
                // changes-reach-prod-without-ci): "v3.59.3" named the release
                // and a hand build alike; the build tells them apart.
                if !quiet {
                    println!(
                        "{}● HOST {} · proto {} — link up{}",
                        C_GREEN,
                        homelab_client::link::version_label(&version, build.as_deref()),
                        proto,
                        C_RESET
                    );
                }
                if names_builds {
                    println!(
                        "{}  client {}{}",
                        C_DIM,
                        homelab_client::link::version_label(
                            env!("CARGO_PKG_VERSION"),
                            Some(homelab_client::BUILD)
                        ),
                        C_RESET
                    );
                }
                // Only on ping: which door was knocked on, and who said so.
                // The rest of the verbs stay quiet about it.
                if is_ping && let Some(src) = HOST_SOURCE.get() {
                    println!("{}  via {} ({}){}", C_DIM, host, src, C_RESET);
                }
                // The client refuses to send a mutating command to an older
                // host, and says which command fixes it (the 2026-08-31
                // data_mounts incident; the rule is in `link`).
                if let Some(why) = homelab_client::link::refuse_older_host(&req.command, &version) {
                    die(&why);
                }
                // fix-105 (older-client-no-warning, 2026-09-27): the other
                // direction — a stale client may not change anything, and
                // says so when it only reads.
                if let Some(why) = homelab_client::link::refuse_older_client(&req.command, &version)
                {
                    die(&why);
                }
                if let Some(warn) = homelab_client::link::older_client_warning(&version) {
                    eprintln!("{}! {}{}", C_YELLOW, warn, C_RESET);
                }
                if !sent {
                    tx.send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                        .await
                        .unwrap_or_else(|e| die(&format!("send: {}", e)));
                    sent = true;
                }
            }
            ServerMsg::Log { .. } | ServerMsg::Transfer { .. } | ServerMsg::State(_) if quiet => {}
            ServerMsg::Log {
                level, source, msg, ..
            } => {
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
            // feat-platform-10: only the attached dashboard gets UI steps.
            ServerMsg::Ui { .. } => {}
            // Live view: the dashboard holds this CLI's UI step ("paused by
            // the viewer …"); on stderr, so `--json` stays one JSON answer.
            ServerMsg::UiNote { note } => {
                eprintln!("{}● {}{}", C_YELLOW, note, C_RESET);
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
                payload_seen = true;
                fleet_seen = Some(*fleet);
            }
            ServerMsg::RpcDone(resp) => {
                // fix-66: an answer sent above is its own RPC and gets its
                // own RpcDone, which would otherwise be read as the whole
                // exchange's result — it arrives beside, not instead of,
                // the operation's own reply (the host runs it off the
                // queue, "beside" it as the register entry says).
                if answer_pending > 0 && resp.id != req.id {
                    answer_pending -= 1;
                    if !resp.ok {
                        eprintln!(
                            "{}✗ answer not delivered: {}{}",
                            C_RED, resp.message, C_RESET
                        );
                    }
                    continue;
                }
                if !echo_reply {
                    return (resp.ok, fleet_seen, Some(resp));
                }
                // fix-103: a fleet check is printed group by group, each in
                // its own colour, instead of one red block.
                if resp.message.starts_with("fleet check:") {
                    print_check(&resp.message, resp.ok);
                    return (resp.ok, fleet_seen, Some(resp));
                }
                if !resp.ok {
                    println!("{}✗ {}{}", C_RED, resp.message, C_RESET);
                    return (false, fleet_seen, Some(resp));
                }
                if homelab_client::rpc_can_exit(awaits_payload, payload_seen, true) {
                    println!("{}✓ {}{}", C_GREEN, resp.message, C_RESET);
                    return (true, fleet_seen, Some(resp));
                }
                done = Some(true);
            }
        }
        if done.is_some() && homelab_client::rpc_can_exit(awaits_payload, payload_seen, true) {
            println!("{}✓ ok{}", C_GREEN, C_RESET);
            return (true, fleet_seen, None);
        }
    }
    eprintln!(
        "{}✗ connection closed before RPC completed{}",
        C_RED, C_RESET
    );
    (false, fleet_seen, None)
}

/// fix-103 (check-output-buries-problem, 2026-09-27): the fleet check with
/// the summary first and each severity group in its own colour.
fn print_check(msg: &str, ok: bool) {
    use homelab_client::output::{Tone, check_tones};
    for (i, (tone, line)) in check_tones(msg).into_iter().enumerate() {
        let color = match tone {
            Tone::Broken => C_RED,
            Tone::Drift => C_YELLOW,
            Tone::Noted => C_DIM,
            Tone::Plain => C_GREEN,
        };
        let mark = match (i, ok) {
            (0, true) => "✓ ",
            (0, false) => "✗ ",
            _ => "",
        };
        println!("{}{}{}{}", color, mark, line, C_RESET);
    }
}

/// Read one line the operator typed after `prompt` (typed-name gates).
fn read_typed(prompt: &str) -> String {
    use std::io::Write as _;
    eprint!("{}{}{}", C_RED, prompt, C_RESET);
    std::io::stderr().flush().ok();
    let mut typed = String::new();
    std::io::stdin().read_line(&mut typed).ok();
    typed.trim().to_string()
}

/// Ship one stack: its native binaries one per message (T85), then the
/// deploy itself. `true` when the host reported the deploy done and ok.
/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): what each
/// stack directory says, for the host to compare with what it applied. A
/// directory that does not read is said out loud and left out; the rest of
/// the check still runs.
fn stack_digests(stack_files: &[(String, u16)]) -> Vec<homelab_core::ops::fleetcheck::StackDigest> {
    let mut out = Vec::new();
    for (dir, _) in stack_files {
        match spec::stack_digest(Path::new(dir)) {
            Ok(d) => out.push(d),
            Err(e) => eprintln!(
                "{}  {} not compared with the host: {}{}",
                C_YELLOW, dir, e, C_RESET
            ),
        }
    }
    out
}

/// fix-110: `config/host.toml` as this working copy reads it, for the fleet
/// check to compare with the host's own running settings. `None` when the
/// repository has no such file (an older checkout, or one that has not
/// taken the coordinator's reconciliation yet) — `homelab check` then skips
/// the comparison, same as an older client talking to a newer host.
fn declared_host_config() -> Option<std::collections::BTreeMap<String, serde_json::Value>> {
    let path = in_repo("config/host.toml");
    let raw = std::fs::read_to_string(&path).ok()?;
    let table: toml::Table = toml::from_str(&raw)
        .inspect_err(|e| {
            eprintln!(
                "{}  {} does not read as TOML: {} — the host-settings comparison is skipped{}",
                C_YELLOW,
                path.display(),
                e,
                C_RESET
            )
        })
        .ok()?;
    let mut out = std::collections::BTreeMap::new();
    for (key, value) in &table {
        // A secret must never have been in this file; dropped defensively
        // rather than sent, in case one was pasted in by hand.
        if homelab_core::hostconfig::is_secret(key) {
            continue;
        }
        if let Ok(v) = serde_json::to_value(value) {
            out.insert(key.clone(), v);
        }
    }
    Some(out)
}

/// arch-deploy-guard: may this tree deploy `stack` over what the host runs?
/// The decision is `homelab_core::ops::deployguard::decide`; this only asks
/// git whether the host's commit is behind HEAD.
fn deploy_guard(
    dir: &Path,
    stack: &str,
    fleet: &homelab_proto::FleetState,
    force: bool,
) -> Result<(), String> {
    use homelab_core::ops::deployguard::{Ancestry, applied_commit, decide};
    let applied = fleet
        .stacks
        .iter()
        .find(|s| s.name == stack)
        .and_then(|s| s.applied_source.clone());
    let ancestry = match applied.as_deref().and_then(applied_commit) {
        None => Ancestry::Contained,
        Some(commit) => {
            let git = |args: &[&str]| {
                std::process::Command::new("git")
                    .arg("-C")
                    .arg(dir)
                    .args(args)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .status()
                    .map(|st| st.success())
                    .unwrap_or(false)
            };
            let object = format!("{}^{{commit}}", commit);
            if !git(&["cat-file", "-e", &object]) {
                Ancestry::Unknown
            } else if git(&["merge-base", "--is-ancestor", commit, "HEAD"]) {
                Ancestry::Contained
            } else {
                Ancestry::Diverged
            }
        }
    };
    decide(stack, applied.as_deref(), ancestry, force)
}

async fn deploy_spec(host: &str, token: &str, mut spec: homelab_proto::DeploySpec) -> bool {
    println!(
        "{}▶ deploy {} :: vmid {} :: {} file(s), {} env(s){}",
        C_CYAN,
        spec.manifest.stack_name,
        spec.manifest.vmid,
        spec.files.len(),
        spec.env.len(),
        C_RESET
    );
    // fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci):
    // `deploy` and `apply` send the working tree; say so when it is not a
    // commit. A warning, not a refusal (Kenny's batch, 2026-09-27).
    if let Some(w) = spec
        .source
        .as_ref()
        .and_then(|s| spec::uncommitted_warning(&spec.manifest.stack_name, s))
    {
        eprintln!("{}{}{}", C_YELLOW, w, C_RESET);
    }
    // T85: each native binary goes over the link on its own, then the deploy
    // follows with the map emptied — the host fills it back in from what was
    // staged. Three binaries in one message measured 94.7 MiB against a
    // 64 MiB frame (F303); one at a time never meets that ceiling, however
    // many services a stack grows.
    let staged: Vec<(String, String)> = spec
        .native_binaries
        .iter()
        .map(|(u, b)| (u.clone(), b.clone()))
        .collect();
    for (unit, b64) in staged {
        println!(
            "{}▶ staging {} ({} KiB of base64){}",
            C_DIM,
            unit,
            b64.len() / 1024,
            C_RESET
        );
        let ok = rpc_with(
            host,
            token,
            Command::StageNativeBinary {
                stack: spec.manifest.stack_name.clone(),
                unit: unit.clone(),
                binary_b64: b64,
            },
        )
        .await;
        if !ok {
            eprintln!(
                "{}✗ staging the binary of {} failed — deploy not started, nothing changed{}",
                C_RED, unit, C_RESET
            );
            return false;
        }
        spec.native_binaries.insert(unit, String::new());
    }
    rpc_with(host, token, Command::DeployStack(Box::new(spec))).await
}
