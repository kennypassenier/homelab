//! `homelab ui`: one step of driving the open dashboard.

use super::*;
use homelab_client::ui_cli::UiCall;
use homelab_client::ui_drive::{End, FileCache};

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "ui" => cmd_ui(c).await,
        _ => return false,
    }
    true
}

/// drive-reach: the running dashboard's catalog (by its hash,
/// kept in ~/.cache) and its refused steps.
async fn ui_list(
    host: &str,
    token: &str,
    filter: Vec<String>,
    line: &mut HostLine<'_>,
    cache: &FileCache,
) -> ! {
    let reply = ui_state_reply(host, token).await;
    let v: serde_json::Value = serde_json::from_str(&reply.message).unwrap_or_default();
    match homelab_client::ui_drive::catalog_for(line, &v, cache).await {
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

async fn ui_refusals(host: &str, token: &str) -> ! {
    let reply = ui_step_or_die(host, token, UiStep::Refusals).await;
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

/// A step pressed and then followed until its job has ended.
async fn ui_press_wait(host: &str, token: &str, step: UiStep, json: bool) -> UiStep {
    let reply = ui_step_or_die(host, token, step).await;
    if !reply.ok {
        ui_print(&reply, json);
    }
    if !json && let Ok(text) = homelab_client::ui_cli::render(&reply.message) {
        print!("{text}");
    }
    ui_finish(host, token, json).await
}

/// fix-185: a final press of an action that reads the repository
/// is checked here, on the machine that is driving, before the
/// dashboard (and its own working copy) is asked at all.
fn is_final_press(call: &UiCall) -> bool {
    matches!(
        call,
        UiCall::Step(UiStep::Press { button }) | UiCall::PressWait(UiStep::Press { button })
            if button == "confirm"
    )
}

/// drive-reach (Kenny, 2026-10-03: guessing a name must stop):
/// a step that names a control, a field or an address is checked
/// here against the catalog of the dashboard that is running (and
/// the dialog open on its screen) before anything reaches the
/// tab; review L3: the orchestration lives in `ui_drive`, tested.
async fn ui_drive_step(
    host: &str,
    token: &str,
    step: UiStep,
    json: bool,
    line: &mut HostLine<'_>,
    cache: &FileCache,
) {
    let is_plan = matches!(step, UiStep::Plan { .. });
    let driven = homelab_client::ui_drive::drive(line, step, cache).await;
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
        End::HostGone => die(NO_ANSWER),
    }
}

/// feat-platform-10: one step of driving the open dashboard.
pub(crate) async fn cmd_ui(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let json = c.args.iter().any(|a| a == "--json");
    let words: Vec<String> = c.args[2..]
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
    let call = homelab_client::ui_cli::parse_call(&words).unwrap_or_else(|e| die(&e));
    if is_final_press(&call) {
        ui_repo_preflight(host, token).await;
    }
    let mut line = HostLine { host, token };
    let cache = FileCache {
        dir: FileCache::default_dir().unwrap_or_else(std::env::temp_dir),
    };
    let step = match call {
        UiCall::List(filter) => ui_list(host, token, filter, &mut line, &cache).await,
        UiCall::Refusals => ui_refusals(host, token).await,
        UiCall::Step(step) => step,
        UiCall::Finish => ui_finish(host, token, json).await,
        UiCall::PressWait(step) => ui_press_wait(host, token, step, json).await,
    };
    ui_drive_step(host, token, step, json, &mut line, &cache).await;
}
