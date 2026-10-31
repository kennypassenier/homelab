//! Host administration verbs: tui, ping, answer, config, doctor, tokens.

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

pub(crate) async fn cmd_doctor(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    rpc(host, token, Command::Doctor { json: false }).await;
}

/// `homelab token issue | list | revoke`.
pub(crate) async fn cmd_token(c: &Ctx) {
    let (host, token, args) = (&c.host, &c.token, &c.args);
    match args.get(2).map(String::as_str) {
        Some("issue") => token_issue(host, token, args).await,
        Some("list") => token_list(host, token).await,
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

async fn token_issue(host: &str, token: &str, args: &[String]) {
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
        Some(r) if r.ok => print_issued(&r.message),
        Some(r) => die(&r.message),
        None => die("the host did not answer"),
    }
}

fn print_issued(message: &str) {
    match serde_json::from_str::<homelab_proto::TokenIssued>(message) {
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

async fn token_list(host: &str, token: &str) {
    match rpc_reply(host, token, Command::TokenList).await {
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
    }
}
