//! The nightly round over managed tools: keep each one current under its
//! follow rule, hold a major for a person, and say so in the notices.

use crate::*;
use homelab_core::notify::{Event, explain_event, urgency};
use homelab_core::ops::tools::{check, install};
use homelab_core::tools::policy::Verdict;
use homelab_core::tools::spec::ToolSpec;

/// One notice for a tool: `ok = false` with `deferred = true` is "stood aside,
/// nothing broke" (a held major), `deferred = false` a failed install.
async fn tool_notice(
    state: &AppState,
    exec: &RealExecutor,
    spec: &ToolSpec,
    kind: &str,
    deferred: bool,
    why: &str,
) {
    let op = format!("{kind}-{}", spec.name);
    let ex = explain_event(&op, &spec.name, false, Some(why));
    publish_notice(
        state,
        exec,
        NoticeFacts {
            label: spec.name.clone(),
            ok: false,
            deferred,
            since: unix_now(),
            urgency: urgency(&Event::Op {
                label: "tool-update",
                ok: false,
                deferred,
            }),
            incident: None,
            req: None,
            by: None,
            findings: Vec::new(),
            op,
            ex,
        },
    )
    .await;
}

/// Bring one tool in line with its newest signed release.
async fn follow_one(state: &AppState, exec: &RealExecutor, spec: &ToolSpec) {
    let status = match check::status(exec, spec).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("tool {}: could not read its release :: {e}", spec.name);
            return;
        }
    };
    match status.verdict {
        Verdict::Current => {}
        Verdict::Install(_) => {
            let outcome = match check::newest_release(exec, spec).await {
                Ok((_, refs)) => install::install(exec, spec, &refs)
                    .await
                    .map(|tag| (tag, refs)),
                Err(e) => Err(e),
            };
            match outcome {
                Ok((tag, _)) => tracing::info!("tool {}: {tag} installed", spec.name),
                Err(e) => {
                    tool_notice(state, exec, spec, "tool-update", false, &e.to_string()).await
                }
            }
        }
        Verdict::Hold { version, major } => {
            let why = if major {
                format!(
                    "{} {version} is a major release and may carry breaking changes; it waits \
                     for you to press Update on the Update page",
                    spec.name
                )
            } else {
                format!(
                    "{} {version} is available; {} is pinned and waits for you to press Update",
                    spec.name, spec.name
                )
            };
            tool_notice(state, exec, spec, "tool-held", true, &why).await;
        }
    }
}

/// The scheduler wakes every 20 minutes inside the night window; the tools are
/// looked at once per night, not at every wake.
const TOOLS_ROUND_EVERY_S: u64 = 20 * 3600;
static LAST_TOOLS_ROUND: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Every managed tool, one after the other; a failing tool never stops the
/// next, and none of this can fail the night.
pub(crate) async fn tools_round(state: &AppState, exec: &RealExecutor) {
    use std::sync::atomic::Ordering::Relaxed;
    let now = unix_now();
    if state.config.tools.is_empty()
        || now.saturating_sub(LAST_TOOLS_ROUND.load(Relaxed)) < TOOLS_ROUND_EVERY_S
    {
        return;
    }
    LAST_TOOLS_ROUND.store(now, Relaxed);
    for spec in &state.config.tools {
        follow_one(state, exec, spec).await;
    }
}
