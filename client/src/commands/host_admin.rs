//! TODO

use super::*;

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "tui" => cmd_tui(c).await,
        "ping" => cmd_ping(c).await,
        "answer" => cmd_answer(c).await,
        "patch" => cmd_patch(c).await,
        "config" => cmd_config(c).await,
        "host" => cmd_host(c).await,
        "doctor" => cmd_doctor(c).await,
        "token" => cmd_token(c).await,
        _ => return false,
    }
    true
}

/// `homelab tui` launches the control deck; `--offline` uses a fake host.
pub(crate) async fn cmd_tui(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let offline = c.offline;
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

pub(crate) async fn cmd_ping(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::Ping).await;
}

/// fix-240: answer a question a running operation is waiting on, from
/// any terminal — the run that raised it may be headless.
pub(crate) async fn cmd_answer(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let (op, allow) =
        homelab_client::answer::parse_answer_words(&args[2..]).unwrap_or_else(|e| die(&e));
    rpc(host, token, Command::AnswerOpen { op, allow }).await
}

pub(crate) async fn cmd_patch(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::PatchFleet).await;
}

pub(crate) async fn cmd_config(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::GetConfig).await;
}

/// Owner decision 2026-09-30 (item 2): CLI parity for the
/// dashboard's "Save and restart the host" — `homelab host
/// restart` schedules the same systemd restart of
/// homelab-host.service and waits the same way `release-update`
/// waits for the host to come back (no version to check, so it
/// only waits for it to go down and answer again).
pub(crate) async fn cmd_host(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    match args.get(2).map(|s| s.as_str()) {
        Some("restart") => {
            println!(
                "{}▶ host restart :: restarting homelab-host.service{}",
                C_CYAN, C_RESET
            );
            let ok = rpc_with(host, token, Command::RestartHost).await
                && wait_for_updated_host(host, token, None).await;
            std::process::exit(if ok { 0 } else { 1 });
        }
        // fix-110 (homelab-admin, 2026-10-01): host settings declarative
        // like a stack — `config/host.toml` sent whole, the host keeps
        // its own secrets and writes it. The repository's path unless
        // one is given.
        // fix-guards-7: `diff` is `apply` without the write — every key
        // whose effective value differs between config/host.toml and
        // the running host, exit 3 (DRIFT_EXIT_CODE) when there is one;
        // an unreachable host is exit 1 and says so, because it says
        // nothing about drift. `make release` runs it:
        // 2026-10-03, fix-240 was "done" with ask_timeout_s 600 in the
        // repository and 120 on the host, and nothing compared them.
        Some(sub @ ("apply" | "diff")) => {
            let path = args
                .get(3)
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| in_repo("config/host.toml"));
            let diff = sub == "diff";
            let toml = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                die(&format!(
                    "{} :: {e} — `homelab host {sub}` reads a repository's config/host.toml; \
                     run this inside the repository, or pass the path",
                    path.display()
                ))
            });
            if let Err(e) = toml.parse::<toml::Table>() {
                die(&format!("{} does not read as TOML: {e}", path.display()));
            }
            println!("{}▶ host {sub} :: {}{}", C_CYAN, path.display(), C_RESET);
            // The same optimistic-concurrency guard a dashboard save
            // uses: read host.toml's current sha256 first, so an edit
            // made meanwhile (over ssh, or a TUI save) is refused
            // rather than silently overwritten.
            let current = rpc_reply(host, token, Command::GetHostConfig)
                .await
                .filter(|r| r.ok)
                .and_then(|r| {
                    serde_json::from_str::<homelab_proto::HostConfigFile>(&r.message).ok()
                })
                .unwrap_or_else(|| {
                    die(if diff {
                        "the host did not answer GetHostConfig — it is unreachable or refused, \
                         so drift could not be checked (this is not a drift verdict)"
                    } else {
                        "the host did not answer GetHostConfig — nothing applied"
                    })
                });
            // fix-191: every key this apply moves, shown before anything
            // is written, and confirmed by typing `apply`. Only these
            // keys are sent as `allow`; the host refuses any other.
            let declared: std::collections::BTreeMap<String, serde_json::Value> = toml
                .parse::<toml::Table>()
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(k, v)| serde_json::to_value(v).ok().map(|v| (k, v)))
                .collect();
            let changes = homelab_core::hostconfig::declared_changes(&declared, &current.values);
            if diff {
                let drift = homelab_core::hostconfig::effective_drift(
                    changes,
                    &current.known_keys,
                    &current.defaults,
                );
                for line in homelab_core::hostconfig::drift_lines(&drift) {
                    println!("{}  {}{}", C_YELLOW, line, C_RESET);
                }
                if drift.is_empty() {
                    println!(
                        "{}✓ config/host.toml and the host's host.toml agree{}",
                        C_GREEN, C_RESET
                    );
                    std::process::exit(0);
                }
                println!(
                    "  {} key(s) differ; `homelab host apply` sends the repository's values",
                    drift.len()
                );
                std::process::exit(homelab_core::hostconfig::DRIFT_EXIT_CODE);
            }
            if changes.is_empty() {
                println!(
                    "{}✓ nothing to apply — the host already runs these settings{}",
                    C_GREEN, C_RESET
                );
                std::process::exit(0);
            }
            let show = |v: &Option<serde_json::Value>| match v {
                None => "(not set)".to_string(),
                Some(v) => {
                    let t = v.to_string();
                    if t.len() > 100 {
                        format!("{}…", &t[..t.floor_char_boundary(100)])
                    } else {
                        t
                    }
                }
            };
            println!("  {} key(s) change on the host:", changes.len());
            for c in &changes {
                let colour = if c.to.is_none() { C_RED } else { C_YELLOW };
                println!(
                    "{}  {}: {} → {}{}",
                    colour,
                    c.key,
                    show(&c.from),
                    show(&c.to),
                    C_RESET
                );
            }
            println!("  type `apply` to write exactly these changes:");
            let mut answer = String::new();
            let _ = std::io::stdin().read_line(&mut answer);
            if answer.trim() != "apply" {
                die("not confirmed — nothing applied");
            }
            let allow: Vec<String> = changes.iter().map(|c| c.key.clone()).collect();
            let expect_sha256 = Some(current.sha256);
            let r = rpc_reply(
                host,
                token,
                Command::ApplyHostConfig {
                    toml,
                    expect_sha256,
                    allow,
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
            "usage: homelab host restart | homelab host apply [path] | homelab host diff [path] (got {:?})",
            other.unwrap_or("nothing")
        )),
    }
}

pub(crate) async fn cmd_doctor(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::Doctor { json: false }).await;
}

/// fix-120 (per-machine tokens, owner decision 2026-10-01): a token
/// per machine, so one can be revoked without touching the others.
pub(crate) async fn cmd_token(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    match args.get(2).map(String::as_str) {
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
            match rpc_reply(host, token, Command::TokenIssue { name, scope }).await {
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
        Some("list") => match rpc_reply(host, token, Command::TokenList).await {
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
            rpc(host, token, Command::TokenRevoke { name }).await
        }
        _ => die(
            "usage: homelab token issue <name> <read|operate|all> | homelab token list | \
             homelab token revoke <name>",
        ),
    }
}
