//! `homelab ui press confirm` preflight: refuse on this machine, before the
//! dashboard is asked, what would act on stale or invalid repository files.

use super::*;

/// Lines of the output of `git -C <root> <args>`; None when git failed.
fn git_lines(root: &Path, args: &[&str]) -> Option<Vec<String>> {
    std::process::Command::new("git")
        .args(["-C"])
        .arg(root)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
}

/// Refuses when local HEAD has commits, touching what the dashboard deploys
/// from, that are not on its upstream.
fn refuse_unpushed(root: &Path) {
    let unpushed = git_lines(root, &["rev-list", "--oneline", "@{u}..HEAD"]).unwrap_or_default();
    // fix-200: only commits that touch what the dashboard deploys from
    // can make it act on stale files.
    let touched = git_lines(root, &["diff", "--name-only", "@{u}..HEAD"]);
    let matters = touched
        .map(|t| homelab_client::ui_preflight::unpushed_touches_deploy_input(&t))
        .unwrap_or(true);
    if matters && let Some(why) = homelab_client::ui_preflight::unpushed_refusal(&unpushed) {
        die(&format!("ui press confirm: {why}"));
    }
}

/// Refuses when a stack involved fails the validation `apply --plan` runs.
fn refuse_invalid_stacks(stacks: &[String]) {
    let mut errors: Vec<(String, String)> = Vec::new();
    for name in stacks {
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

/// fix-185: before a driven `press confirm` on an action that reads the
/// repository, refuse HERE — never lets a bad press reach the dashboard —
/// when local HEAD has commits not on its upstream (the dashboard's own
/// working copy fetches origin, so it would read the files from before
/// them), or a stack involved fails the same validation `homelab apply
/// --plan` runs. Reads the open form through one `ui state` first: the step
/// itself carries no stack name.
pub(crate) async fn ui_repo_preflight(host: &str, token: &str) {
    let Some(reply) = ui_step_reply(host, token, UiStep::State).await else {
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
    let declared: Vec<String> = homelab_client::spec::declared_stacks(&stacks_base())
        .into_iter()
        .map(|(n, _)| n)
        .collect();
    let stacks = homelab_client::ui_preflight::stacks_involved(action, form_stack, &declared);
    if let Some(root) = repo_root() {
        refuse_unpushed(root);
    }
    refuse_invalid_stacks(&stacks);
}
