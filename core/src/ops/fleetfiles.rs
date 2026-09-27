//! The two fleet-wide files a deploy renders: Homepage's `services.yaml`
//! (T51) and the Uptime Kuma seeder's `host-monitors.json` (T49).
//!
//! step-22 (Kenny, 2026-09-27: everything a stack registers is declarative):
//! they used to be rendered by the deploy only, so a destroyed or forgotten
//! stack kept its tile and its host monitor until some other stack happened
//! to be deployed. Factored out of the deploy so destroy and forget render
//! them too, from the same code.

use crate::error::CoreError;
use crate::executor::{pct_sh, shq, Cmd, Executor};
use crate::runner::StepOutcome;
use crate::sink::{Level, PipelineEvent};

use super::OpCtx;

fn info(ctx: &OpCtx<'_>, msg: String) {
    ctx.sink.emit(PipelineEvent::Line {
        level: Level::Info,
        source: "HOST".into(),
        msg,
    });
}

/// T51: the front page, rendered from the routes this orchestrator has
/// written for the whole fleet.
///
/// Fleet-wide rather than per stack, because Homepage keeps one file — so
/// this reads every route fragment in the gateway's route directory. The
/// caller decides whether a failure matters; writing the file itself is
/// best-effort and reported, never an error.
pub async fn write_homepage_services(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    dest: &str,
) -> Result<StepOutcome, CoreError> {
    let dir = &ctx.safety.gateway_routes_dir;
    let listing = pct_sh(
        exec,
        ctx.safety.gateway_vmid,
        &format!("ls -1 {}/*.yml 2>/dev/null || true", shq(dir)),
        60,
    )
    .await?;
    let mut stacks: Vec<(String, Vec<crate::ops::homepage::Entry>)> = Vec::new();
    for path in listing
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
    {
        let name = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        // `112-app-almanac` → `almanac`; a hand-written `manual-…` fragment
        // keeps its own name.
        let stack = name
            .split_once("-app-")
            .map(|(_, s)| s.to_string())
            .unwrap_or(name);
        let body = pct_sh(
            exec,
            ctx.safety.gateway_vmid,
            &format!("cat {}", shq(path)),
            60,
        )
        .await?;
        let entries = crate::ops::homepage::entries_from_route(&body.stdout);
        if !entries.is_empty() {
            stacks.push((stack, entries));
        }
    }
    stacks.sort_by(|a, b| a.0.cmp(&b.0));
    // V6: the overlay is intent, not runtime config, so it lives in the
    // intent repo next to the stack file that ships it — where `git log`
    // records who changed the front page and why. Found by its name rather
    // than by a new setting: there is exactly one, and a config knob for it
    // would be one more line nobody remembers to add (which is how T51 sat
    // switched off for two days — F186).
    let found = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                &format!(
                    "ls -1 {}/repo/stacks/*/*/services-overlay.yml 2>/dev/null | head -1",
                    ctx.state_dir
                ),
            ],
            30,
        ))
        .await?;
    let overlay_path = found.stdout.trim().to_string();
    let overlay = match exec.read_file(&overlay_path).await {
        Ok(text) => {
            let ov = crate::ops::homepage::parse_overlay(&text);
            info(
                ctx,
                format!(
                    "[t51] overlay: {} entr(y/ies) from {}",
                    ov.blocks.len(),
                    overlay_path
                ),
            );
            Some(ov)
        }
        Err(_) => None,
    };
    // V6b: read each widget's API key from the application itself.
    //
    // Through ctx.exec rather than the tracing executor on purpose: the
    // tracing one echoes stdout into the transcript, and these are live keys
    // (standing rule 10 — a hash may appear there, plaintext never).
    // Best-effort per app: a key that cannot be read leaves the widget
    // pointing at Homepage's own variable, which fails visibly rather than
    // silently.
    let mut widget_keys: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();
    {
        let store = crate::state::StateStore::new(exec, &ctx.state_dir);
        let st = store.load().await.unwrap_or_default();
        for (stack, entries) in &stacks {
            let Some(vmid) = st
                .stacks
                .get(stack)
                .and_then(|s| s.manifest.as_ref())
                .map(|mf| mf.vmid)
            else {
                continue;
            };
            for e in entries {
                let Some(spec) = crate::ops::homepage::widget_for(&e.app) else {
                    continue;
                };
                let Some(cmd) = spec.key_cmd else { continue };
                // O7 makes this path the only legal one, so it is derived
                // rather than looked up.
                let dir = format!("/appdata/{}/{}-config", stack, e.app);
                let script = cmd.replace("{dir}", &dir);
                if let Ok(out) = pct_sh(ctx.exec, vmid, &script, 30).await {
                    let k = out.stdout.trim().to_string();
                    if out.success() && !k.is_empty() {
                        widget_keys.insert(e.app.clone(), k);
                    }
                }
            }
        }
    }
    info(
        ctx,
        format!(
            "[t51] widget keys read from the applications themselves: {}",
            widget_keys.len()
        ),
    );
    let body = crate::ops::homepage::services_yaml(&stacks, overlay.as_ref(), &widget_keys);
    match crate::ops::util::write_file_owned_like_dir(exec, dest, &body, 0o644).await {
        Ok(()) => {
            info(
                ctx,
                format!(
                    "[t51] {} — {} stack(s) on the front page",
                    dest,
                    stacks.len()
                ),
            );
            Ok(StepOutcome::Changed)
        }
        Err(e) => {
            info(ctx, format!("[t51] could not write {} ({})", dest, e));
            Ok(StepOutcome::Unchanged)
        }
    }
}

/// T49: the watch list, rendered from every stack in host state.
///
/// `deploying` is the stack a deploy is about to record: on a FIRST deploy
/// the state write happens after this runs, and a brand-new stack should
/// not wait a whole deploy for the monitor that says whether it came up.
/// Destroy and forget pass None — they render from state as it now is.
pub async fn write_host_monitors(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    dest: &str,
    deploying: Option<(&str, &str)>,
) -> Result<StepOutcome, CoreError> {
    let store = crate::state::StateStore::new(exec, &ctx.state_dir);
    let state = store.load().await?;
    let mut fleet: Vec<(String, String)> = state
        .stacks
        .iter()
        .filter_map(|(name, st)| {
            // gap-14: an adopted native stack has no stack manifest in
            // state; its address follows from the vmid.
            let ip = match st.manifest.as_ref() {
                Some(mf) => mf.network.ip.clone(),
                None if st.is_native() => crate::ops::monitors::address_for_vmid(st.vmid)?,
                None => return None,
            };
            Some((name.clone(), ip))
        })
        .collect();
    if let Some((stack, ip)) = deploying {
        if !fleet.iter().any(|(n, _)| n == stack) {
            fleet.push((stack.to_string(), ip.to_string()));
        }
    }
    let monitors = crate::ops::monitors::host_monitors(&fleet);
    let body = crate::ops::monitors::monitors_json(&monitors);
    match crate::ops::util::write_file_owned_like_dir(exec, dest, &body, 0o644).await {
        Ok(()) => {
            info(
                ctx,
                format!(
                    "[t49] {} — {} host monitor(s) for the seeder",
                    dest,
                    monitors.len()
                ),
            );
            Ok(StepOutcome::Changed)
        }
        Err(e) => {
            info(ctx, format!("[t49] could not write {} ({})", dest, e));
            Ok(StepOutcome::Unchanged)
        }
    }
}

/// step-22: both files after a stack left the fleet (destroy, forget).
///
/// Best-effort on purpose: by the time this runs the container and the
/// record are already gone, and failing the operation over a front page
/// would report a destroy that happened as one that did not.
pub async fn regenerate_after_removal(ctx: &OpCtx<'_>, exec: &dyn Executor) -> StepOutcome {
    let mut changed = false;
    if let Some(dest) = ctx.homepage_services_file.as_deref() {
        match write_homepage_services(ctx, exec, dest).await {
            Ok(o) => changed |= o == StepOutcome::Changed,
            Err(e) => ctx.sink.emit(PipelineEvent::Line {
                level: Level::Warn,
                source: "HOST".into(),
                msg: format!(
                    "[t51] the front page was not regenerated ({}) — it still shows the stack \
                     until the next deploy",
                    e
                ),
            }),
        }
    }
    if let Some(dest) = ctx.kuma_monitors_file.as_deref() {
        match write_host_monitors(ctx, exec, dest, None).await {
            Ok(o) => changed |= o == StepOutcome::Changed,
            Err(e) => ctx.sink.emit(PipelineEvent::Line {
                level: Level::Warn,
                source: "HOST".into(),
                msg: format!(
                    "[t49] the host monitors were not regenerated ({}) — the seeder keeps \
                     watching the stack until the next deploy",
                    e
                ),
            }),
        }
    }
    if changed {
        StepOutcome::Changed
    } else {
        StepOutcome::Unchanged
    }
}
