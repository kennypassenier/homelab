//! `homelab host`: restart, and the declarative host settings (apply, diff).

use super::*;

/// `homelab host restart | apply [path] | diff [path]`.
pub(crate) async fn cmd_host(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    match c.args.get(2).map(|s| s.as_str()) {
        Some("restart") => host_restart(host, token).await,
        Some(sub @ ("apply" | "diff")) => host_settings(host, token, &c.args, sub == "diff").await,
        other => die(&format!(
            "usage: homelab host restart | homelab host apply [path] | homelab host diff [path] (got {:?})",
            other.unwrap_or("nothing")
        )),
    }
}

async fn host_restart(host: &str, token: &str) {
    println!(
        "{}▶ host restart :: restarting homelab-host.service{}",
        C_CYAN, C_RESET
    );
    let ok = rpc_with(host, token, Command::RestartHost).await
        && wait_for_updated_host(host, token, None).await;
    std::process::exit(if ok { 0 } else { 1 });
}

/// The repository's host.toml, read and checked to be TOML.
fn read_host_toml(path: &Path, sub: &str) -> String {
    let toml = std::fs::read_to_string(path).unwrap_or_else(|e| {
        die(&format!(
            "{} :: {e} — `homelab host {sub}` reads a repository's config/host.toml; \
             run this inside the repository, or pass the path",
            path.display()
        ))
    });
    if let Err(e) = toml.parse::<toml::Table>() {
        die(&format!("{} does not read as TOML: {e}", path.display()));
    }
    toml
}

/// The same optimistic-concurrency guard a dashboard save
/// uses: read host.toml's current sha256 first, so an edit
/// made meanwhile (over ssh, or a TUI save) is refused
/// rather than silently overwritten.
async fn current_host_config(host: &str, token: &str, diff: bool) -> homelab_proto::HostConfigFile {
    rpc_reply(host, token, Command::GetHostConfig)
        .await
        .filter(|r| r.ok)
        .and_then(|r| serde_json::from_str::<homelab_proto::HostConfigFile>(&r.message).ok())
        .unwrap_or_else(|| {
            die(if diff {
                "the host did not answer GetHostConfig — it is unreachable or refused, \
                 so drift could not be checked (this is not a drift verdict)"
            } else {
                "the host did not answer GetHostConfig — nothing applied"
            })
        })
}

/// fix-110 (homelab-admin, 2026-10-01): host settings declarative
/// like a stack — `config/host.toml` sent whole, the host keeps
/// its own secrets and writes it. The repository's path unless
/// one is given.
/// fix-guards-7: `diff` is `apply` without the write — every key
/// whose effective value differs between config/host.toml and
/// the running host, exit 3 (DRIFT_EXIT_CODE) when there is one;
/// an unreachable host is exit 1 and says so, because it says
/// nothing about drift. `make release` runs it:
/// 2026-10-03, fix-240 was "done" with ask_timeout_s 600 in the
/// repository and 120 on the host, and nothing compared them.
async fn host_settings(host: &str, token: &str, args: &[String], diff: bool) {
    let sub = if diff { "diff" } else { "apply" };
    let path = args
        .get(3)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| in_repo("config/host.toml"));
    let toml = read_host_toml(&path, sub);
    println!("{}▶ host {sub} :: {}{}", C_CYAN, path.display(), C_RESET);
    let current = current_host_config(host, token, diff).await;
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
        report_drift(changes, &current);
    }
    if changes.is_empty() {
        println!(
            "{}✓ nothing to apply — the host already runs these settings{}",
            C_GREEN, C_RESET
        );
        std::process::exit(0);
    }
    print_changes(&changes);
    confirm_apply();
    let allow: Vec<String> = changes.iter().map(|c| c.key.clone()).collect();
    let expect_sha256 = Some(current.sha256);
    let command = Command::ApplyHostConfig {
        toml,
        expect_sha256,
        allow,
    };
    report_applied(rpc_reply(host, token, command).await);
}

/// `host diff`: prints the drift and exits with its verdict.
fn report_drift(
    changes: Vec<homelab_core::hostconfig::KeyChange>,
    current: &homelab_proto::HostConfigFile,
) -> ! {
    let drift =
        homelab_core::hostconfig::effective_drift(changes, &current.known_keys, &current.defaults);
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

fn show_value(v: &Option<serde_json::Value>) -> String {
    match v {
        None => "(not set)".to_string(),
        Some(v) => {
            let t = v.to_string();
            if t.len() > 100 {
                format!("{}…", &t[..t.floor_char_boundary(100)])
            } else {
                t
            }
        }
    }
}

fn print_changes(changes: &[homelab_core::hostconfig::KeyChange]) {
    println!("  {} key(s) change on the host:", changes.len());
    for c in changes {
        let colour = if c.to.is_none() { C_RED } else { C_YELLOW };
        println!(
            "{}  {}: {} → {}{}",
            colour,
            c.key,
            show_value(&c.from),
            show_value(&c.to),
            C_RESET
        );
    }
}

fn confirm_apply() {
    println!("  type `apply` to write exactly these changes:");
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    if answer.trim() != "apply" {
        die("not confirmed — nothing applied");
    }
}

fn report_applied(reply: Option<homelab_proto::RpcResponse>) {
    match reply {
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
