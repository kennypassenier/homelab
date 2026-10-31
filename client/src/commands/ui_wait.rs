//! `homelab ui` plumbing: the line to the dashboard, state reads, printing an
//! answer, and the two waiting loops (`ui finish`, following a plan).

use super::*;
use homelab_client::ui_cli::{FinishNext, PlanNext};

/// review L3: the host line `ui_drive` sends its steps over.
pub(crate) struct HostLine<'a> {
    pub(crate) host: &'a str,
    pub(crate) token: &'a str,
}

impl homelab_client::ui_drive::Line for HostLine<'_> {
    async fn send(&mut self, step: UiStep) -> Option<homelab_client::ui_drive::Reply> {
        ui_step_reply(self.host, self.token, step)
            .await
            .map(|r| homelab_client::ui_drive::Reply {
                ok: r.ok,
                message: r.message,
            })
    }
}

pub(crate) const NO_ANSWER: &str = "the host closed the line before it answered";

/// One UI step sent to the dashboard, with this build's version attached.
pub(crate) async fn ui_step_reply(
    host: &str,
    token: &str,
    step: UiStep,
) -> Option<homelab_proto::RpcResponse> {
    let command = Command::Ui {
        step,
        client_version: CLIENT_VERSION.to_string(),
    };
    rpc_reply(host, token, command).await
}

/// drive-reach: the dashboard's answer to `ui state` (the screen and the
/// hash of its control catalog); the process ends when the host does not
/// answer.
pub(crate) async fn ui_state_reply(host: &str, token: &str) -> homelab_proto::RpcResponse {
    ui_step_reply(host, token, UiStep::State)
        .await
        .unwrap_or_else(|| die(NO_ANSWER))
}

/// The host's answer to a step, or the process ends.
pub(crate) async fn ui_step_or_die(
    host: &str,
    token: &str,
    step: UiStep,
) -> homelab_proto::RpcResponse {
    ui_step_reply(host, token, step)
        .await
        .unwrap_or_else(|| die(NO_ANSWER))
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

/// `ui finish` saw the job end in something other than success: exit 1
/// once `done` has let go.
pub(crate) static FINISH_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

async fn pause() {
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
}

/// Counts one look without an answer; ends the process at `limit`.
fn register_miss(misses: &mut u32, limit: u32, why: &str) {
    *misses += 1;
    if *misses >= limit {
        die(why);
    }
}

/// What `ui finish` does with the job's state: wait on, or release.
fn finish_step(next: FinishNext, json: bool, last: &mut String) -> Option<UiStep> {
    match next {
        FinishNext::Wait(line) => {
            if !json && line != *last {
                println!("{line}");
            }
            *last = line;
            None
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
            Some(UiStep::Done)
        }
    }
}

/// `homelab ui finish` (Kenny, 2026-09-29): read the screen every 2 s until
/// the open dialog's job has ended, print its outcome, then answer with the
/// `done` step to send, which closes the dialog and gives the tabs back at
/// once instead of after fix-163's 30 s. A failed job still lets go, and
/// the process then ends with 1 (after `done` answered).
pub(crate) async fn ui_finish(host: &str, token: &str, json: bool) -> UiStep {
    use homelab_client::ui_cli::{dashboard_away, finish_next};
    let mut last = String::new();
    let mut misses = 0;
    loop {
        let Some(reply) = ui_step_reply(host, token, UiStep::State).await else {
            // A job that restarts the dashboard drops the line for a moment.
            register_miss(
                &mut misses,
                30,
                "ui finish: the host did not answer for a minute; `homelab ui state` shows where it is",
            );
            pause().await;
            continue;
        };
        // restart-wait-1: a job that restarts the host answers "no dashboard
        // is attached" for a few seconds while the link comes back.
        if dashboard_away(&reply.message) {
            register_miss(
                &mut misses,
                30,
                "ui finish: the dashboard did not come back for a minute; `homelab ui state` shows where it is",
            );
            pause().await;
            continue;
        }
        misses = 0;
        let next = finish_next(&reply.message).unwrap_or_else(|e| die(&e));
        if let Some(step) = finish_step(next, json, &mut last) {
            return step;
        }
        pause().await;
    }
}

/// What the plan follower does with the plan's state; true when finished.
fn plan_step(next: PlanNext, json: bool, last: &mut String) -> bool {
    match next {
        PlanNext::Wait(line) => {
            if !json && line != *last {
                println!("{line}");
            }
            *last = line;
            false
        }
        PlanNext::Done(line) => {
            if !json {
                println!("{line}");
            }
            true
        }
        PlanNext::Failed(why) | PlanNext::Stopped(why) => die(&format!("ui plan: {why}")),
        PlanNext::Gone => true,
    }
}

/// liveview-plan-runs-1: follow a plan the dashboard runs itself, printing
/// each change, until it finished (exit 0), failed or was stopped (exit 1).
pub(crate) async fn ui_follow_plan(host: &str, token: &str, json: bool) {
    use homelab_client::ui_cli::{dashboard_away, plan_next};
    let mut last = String::new();
    let mut misses = 0;
    loop {
        let reply = ui_step_reply(host, token, UiStep::State).await;
        let Some(reply) = reply.filter(|r| !dashboard_away(&r.message)) else {
            register_miss(
                &mut misses,
                90,
                "ui plan: the dashboard did not answer for three minutes; `homelab ui state` shows where the plan is",
            );
            pause().await;
            continue;
        };
        misses = 0;
        let next = plan_next(&reply.message).unwrap_or_else(|e| die(&e));
        if plan_step(next, json, &mut last) {
            return;
        }
        pause().await;
    }
}
