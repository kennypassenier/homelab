//! Helpers for `homelab ui`: the line to the dashboard, state, preflight and finishing.

use super::*;

/// review L3: the host line `ui_drive` sends its steps over.
pub(crate) struct HostLine<'a> {
    pub(crate) host: &'a str,
    pub(crate) token: &'a str,
}

impl homelab_client::ui_drive::Line for HostLine<'_> {
    async fn send(&mut self, step: UiStep) -> Option<homelab_client::ui_drive::Reply> {
        rpc_reply(
            self.host,
            self.token,
            Command::Ui {
                step,
                client_version: CLIENT_VERSION.to_string(),
            },
        )
        .await
        .map(|r| homelab_client::ui_drive::Reply {
            ok: r.ok,
            message: r.message,
        })
    }
}

/// drive-reach: the dashboard's answer to `ui state` (the screen and the
/// hash of its control catalog); the process ends when the host does not
/// answer.
pub(crate) async fn ui_state_reply(host: &str, token: &str) -> homelab_proto::RpcResponse {
    rpc_reply(
        host,
        token,
        Command::Ui {
            step: UiStep::State,
            client_version: CLIENT_VERSION.to_string(),
        },
    )
    .await
    .unwrap_or_else(|| die("the host closed the line before it answered"))
}

/// A `homelab ui` answer printed, and the process ended with its code.
pub(crate) fn ui_print(reply: &homelab_proto::RpcResponse, json: bool) -> ! {
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

/// fix-185: before a driven `press confirm` on an action that reads the
/// repository, refuse HERE — never lets a bad press reach the dashboard —
/// when local HEAD has commits not on its upstream (the dashboard's own
/// working copy fetches origin, so it would read the files from before
/// them), or a stack involved fails the same validation `homelab apply
/// --plan` runs. Reads the open form through one `ui state` first: the step
/// itself carries no stack name.
pub(crate) async fn ui_repo_preflight(host: &str, token: &str) {
    let Some(reply) = rpc_reply(
        host,
        token,
        Command::Ui {
            step: UiStep::State,
            client_version: CLIENT_VERSION.to_string(),
        },
    )
    .await
    else {
        // The real press surfaces the connection problem; nothing to check
        // here without an answer.
        return;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&reply.message) else {
        return;
    };
    let form = &v["state"]["form"];
    if !form["reads_repo"].as_bool().unwrap_or(false) {
        return;
    }
    let action = form["action"].as_str().unwrap_or("");
    let form_stack = form["stack"].as_str().unwrap_or("");
    let base = stacks_base();
    let declared: Vec<String> = homelab_client::spec::declared_stacks(&base)
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let stacks = homelab_client::ui_preflight::stacks_involved(action, form_stack, &declared);
    if let Some(root) = repo_root() {
        let unpushed = std::process::Command::new("git")
            .args(["-C"])
            .arg(root)
            .args(["rev-list", "--oneline", "@{u}..HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        // fix-200: only commits that touch what the dashboard deploys from
        // can make it act on stale files.
        let touched = std::process::Command::new("git")
            .args(["-C"])
            .arg(root)
            .args(["diff", "--name-only", "@{u}..HEAD"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            });
        let matters = touched
            .map(|t| homelab_client::ui_preflight::unpushed_touches_deploy_input(&t))
            .unwrap_or(true);
        if matters && let Some(why) = homelab_client::ui_preflight::unpushed_refusal(&unpushed) {
            die(&format!("ui press confirm: {why}"));
        }
    }
    let mut errors: Vec<(String, String)> = Vec::new();
    for name in &stacks {
        let dir = stack_dir(name);
        if !dir.join("lxc-compose.yml").is_file() {
            // Not a deployable stack (e.g. native-only): nothing here for
            // `homelab apply --plan`'s own validation to check either.
            continue;
        }
        match homelab_client::spec::build_spec(&dir) {
            Ok(sp) => {
                if let Err(e) = homelab_core::manifest::validate(&sp) {
                    errors.push((name.clone(), e.to_string()));
                }
            }
            Err(e) => errors.push((name.clone(), e)),
        }
    }
    if let Some(why) = homelab_client::ui_preflight::validation_refusal(&errors) {
        die(&format!("ui press confirm: {why}"));
    }
}

/// `homelab ui finish` (Kenny, 2026-09-29): read the screen every 2 s until
/// the open dialog's job has ended, print its outcome, then answer with the
/// `done` step to send, which closes the dialog and gives the tabs back at
/// once instead of after fix-163's 30 s. A failed job still lets go, and
/// the process then ends with 1 (after `done` answered).
pub(crate) async fn ui_finish(host: &str, token: &str, json: bool) -> UiStep {
    use homelab_client::ui_cli::{FinishNext, finish_next};
    let mut last = String::new();
    let mut misses = 0;
    loop {
        let Some(reply) = rpc_reply(
            host,
            token,
            Command::Ui {
                step: UiStep::State,
                client_version: CLIENT_VERSION.to_string(),
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
        // restart-wait-1: a job that restarts the host answers "no dashboard
        // is attached" for a few seconds while the link comes back.
        if homelab_client::ui_cli::dashboard_away(&reply.message) {
            misses += 1;
            if misses >= 30 {
                die(
                    "ui finish: the dashboard did not come back for a minute; `homelab ui state` shows where it is",
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        }
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

/// liveview-plan-runs-1: follow a plan the dashboard runs itself, printing
/// each change, until it finished (exit 0), failed or was stopped (exit 1).
pub(crate) async fn ui_follow_plan(host: &str, token: &str, json: bool) {
    use homelab_client::ui_cli::{PlanNext, dashboard_away, plan_next};
    let mut last = String::new();
    let mut misses = 0;
    loop {
        let reply = rpc_reply(
            host,
            token,
            Command::Ui {
                step: UiStep::State,
                client_version: CLIENT_VERSION.to_string(),
            },
        )
        .await;
        let Some(reply) = reply.filter(|r| !dashboard_away(&r.message)) else {
            misses += 1;
            if misses >= 90 {
                die(
                    "ui plan: the dashboard did not answer for three minutes; `homelab ui state` shows where the plan is",
                );
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            continue;
        };
        misses = 0;
        match plan_next(&reply.message).unwrap_or_else(|e| die(&e)) {
            PlanNext::Wait(line) => {
                if !json && line != last {
                    println!("{line}");
                }
                last = line;
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
            PlanNext::Done(line) => {
                if !json {
                    println!("{line}");
                }
                return;
            }
            PlanNext::Failed(why) | PlanNext::Stopped(why) => die(&format!("ui plan: {why}")),
            PlanNext::Gone => return,
        }
    }
}

/// `ui finish` saw the job end in something other than success: exit 1
/// once `done` has let go.
pub(crate) static FINISH_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "ui" => cmd_ui(c).await,
        _ => return false,
    }
    true
}

/// feat-platform-10: one step of driving the open dashboard.
pub(crate) async fn cmd_ui(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let json = args.iter().any(|a| a == "--json");
    let words: Vec<String> = args[2..]
        .iter()
        .filter(|a| a.as_str() != "--json")
        .cloned()
        .collect();
    // fix-240: Live view answers the open question with the same
    // host call as the banner's Allow/Stop; every tab's banner
    // goes once the host says the question is settled.
    if words.first().map(String::as_str) == Some("answer") {
        let (op, allow) =
            homelab_client::answer::parse_answer_words(&words[1..]).unwrap_or_else(|e| die(&e));
        rpc(host, token, Command::AnswerOpen { op, allow }).await;
        return;
    }
    use homelab_client::ui_cli::UiCall;
    let call = homelab_client::ui_cli::parse_call(&words).unwrap_or_else(|e| die(&e));
    // fix-185: a final press of an action that reads the repository
    // is checked here, on the machine that is driving, before the
    // dashboard (and its own working copy) is asked at all.
    if matches!(
        &call,
        UiCall::Step(UiStep::Press { button }) | UiCall::PressWait(UiStep::Press { button })
            if button == "confirm"
    ) {
        ui_repo_preflight(host, token).await;
    }
    let mut line = HostLine {
        host: host,
        token: token,
    };
    let cache = homelab_client::ui_drive::FileCache {
        dir: homelab_client::ui_drive::FileCache::default_dir().unwrap_or_else(std::env::temp_dir),
    };
    let step = match call {
        // drive-reach: the running dashboard's catalog (by its hash,
        // kept in ~/.cache) and its refused steps.
        UiCall::List(filter) => {
            let reply = ui_state_reply(host, token).await;
            let v: serde_json::Value = serde_json::from_str(&reply.message).unwrap_or_default();
            match homelab_client::ui_drive::catalog_for(&mut line, &v, &cache).await {
                Ok(Some(c)) => {
                    print!("{}", homelab_client::ui_catalog::render_list(&c, &filter));
                    std::process::exit(0);
                }
                Ok(None) => die(
                    "this dashboard serves no control catalog (it predates drive-reach): \
                     `homelab ui reload` once it is updated, or `homelab ui state`",
                ),
                Err(e) => die(&e),
            }
        }
        UiCall::Refusals => {
            let reply = rpc_reply(
                host,
                token,
                Command::Ui {
                    step: UiStep::Refusals,
                    client_version: CLIENT_VERSION.to_string(),
                },
            )
            .await
            .unwrap_or_else(|| die("the host closed the line before it answered"));
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            match homelab_client::ui_catalog::render_refusals(&reply.message, now) {
                Ok(text) => {
                    print!("{text}");
                    std::process::exit(0);
                }
                Err(e) => die(&e),
            }
        }
        UiCall::Step(step) => step,
        UiCall::Finish => ui_finish(host, token, json).await,
        UiCall::PressWait(step) => {
            let reply = rpc_reply(
                host,
                token,
                Command::Ui {
                    step,
                    client_version: CLIENT_VERSION.to_string(),
                },
            )
            .await
            .unwrap_or_else(|| die("the host closed the line before it answered"));
            if !reply.ok {
                ui_print(&reply, json);
            }
            if !json && let Ok(text) = homelab_client::ui_cli::render(&reply.message) {
                print!("{text}");
            }
            ui_finish(host, token, json).await
        }
    };
    // drive-reach (Kenny, 2026-10-03: guessing a name must stop):
    // a step that names a control, a field or an address is checked
    // here against the catalog of the dashboard that is running (and
    // the dialog open on its screen) before anything reaches the
    // tab; review L3: the orchestration lives in `ui_drive`, tested.
    use homelab_client::ui_drive::End;
    let is_plan = matches!(step, UiStep::Plan { .. });
    let driven = homelab_client::ui_drive::drive(&mut line, step, &cache).await;
    for n in &driven.notes {
        eprintln!("note: {n}");
    }
    match driven.end {
        // liveview-plan-runs-1: an announced plan is run by the
        // dashboard once a viewer presses Start; follow it to its end.
        End::Print(r) if is_plan && r.ok => {
            if !json {
                println!(
                    "plan announced: Kenny presses Start in the dashboard, which then takes every step itself"
                );
            }
            ui_follow_plan(host, token, json).await;
        }
        End::Print(r) => ui_print(
            &homelab_proto::RpcResponse {
                id: 0,
                ok: r.ok,
                message: r.message,
                deferred: None,
            },
            json,
        ),
        End::Refused(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
        End::HostGone => die("the host closed the line before it answered"),
    }
}
