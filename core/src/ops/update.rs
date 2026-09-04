//! Managed updates with rollback (D9 + B6). Per app: capture the running
//! image (id + repo:tag), `compose pull` + `up -d`, verify health; on a failed
//! verify re-tag the captured image back and force-recreate, then verify the
//! rollback took. Policy: the `com.homelab.update.policy` container label —
//! `auto` apps update on scheduled runs, everything else only on an explicit
//! user request (`auto=false`).

use crate::error::CoreError;
use crate::executor::{Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// One captured container image: `sha256:<id>` plus the `repo:tag` it ran as.
#[derive(Debug, Clone)]
struct CapturedImage {
    image_id: String,
    repo_tag: String,
}

async fn capture_app(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<Vec<CapturedImage>, CoreError> {
    let script = format!(
        "cd '/opt/{}/{}' && docker compose ps -q | xargs -r docker inspect --format '{{{{.Image}}}} {{{{.Config.Image}}}}'",
        stack, app
    );
    let out = super::util_pct_sh(exec, vmid, &script, 60).await?;
    Ok(out
        .stdout
        .lines()
        .filter_map(|l| {
            let mut parts = l.split_whitespace();
            Some(CapturedImage {
                image_id: parts.next()?.to_string(),
                repo_tag: parts.next()?.to_string(),
            })
        })
        .collect())
}

async fn app_policy(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<String, CoreError> {
    let script = format!(
        "cd '/opt/{}/{}' && docker compose ps -q | head -1 | xargs -r docker inspect --format '{{{{index .Config.Labels \"com.homelab.update.policy\"}}}}'",
        stack, app
    );
    let out = super::util_pct_sh(exec, vmid, &script, 60).await?;
    Ok(out.stdout.trim().to_string())
}

async fn verify_app(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<bool, CoreError> {
    let script = format!(
        "cd '/opt/{}/{}' && docker compose ps --status running --services",
        stack, app
    );
    let out = super::util_pct_sh(exec, vmid, &script, 60).await?;
    Ok(!out.stdout.trim().is_empty())
}

/// Update one app (or all apps when `only=None`) in a stack. `auto=true` means
/// a scheduled run: apps without the `auto` policy label are skipped.
pub async fn update(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    only: Option<&str>,
    auto: bool,
) -> OperationReport {
    let op = format!("update-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    let apps: Vec<String> = m
        .apps
        .iter()
        .filter(|a| only.is_none_or(|o| o == a.as_str()))
        .cloned()
        .collect();
    if apps.is_empty() {
        return runner.finish_err(
            "select apps",
            &CoreError::Other(format!(
                "app '{}' is not part of stack {}",
                only.unwrap_or("?"),
                m.stack_name
            )),
        );
    }

    // A1/A2: updates pull and recreate containers inside the target.
    step!(runner, "safety gates", {
        crate::manifest::validate_manifest(m)?;
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    for app in &apps {
        // Own the strings the step body needs; the macro closure borrows them.
        let stack = m.stack_name.clone();
        let vmid = m.vmid;

        // Policy gate (D9): scheduled runs only touch policy=auto apps.
        let policy_step = format!("{} :: policy", app);
        let skipped = step!(runner, &policy_step, {
            if auto {
                let policy = app_policy(exec, vmid, &stack, app).await?;
                if policy != "auto" {
                    return Ok(StepOutcome::Unchanged);
                }
            }
            Ok(StepOutcome::Changed)
        });
        if matches!(skipped, StepOutcome::Unchanged) {
            runner.log(
                Level::Info,
                format!("[update] {} skipped (policy is not 'auto')", app),
            );
            continue;
        }

        // B6: capture the running images so a bad update can be undone.
        let mut captured: Vec<CapturedImage> = Vec::new();
        let cap_step = format!("{} :: capture", app);
        step!(runner, &cap_step, {
            captured = capture_app(exec, vmid, &stack, app).await?;
            Ok(StepOutcome::Unchanged)
        });

        // O10: ask before walking in. Only Jellyfin is asked, because it is
        // the only service here where an update lands in somebody's evening —
        // and the check fails CLOSED, so an unreachable or unparseable answer
        // skips the update rather than assuming nobody is watching.
        let busy_step = format!("{} :: busy check", app);
        let mut skip_app: Option<String> = None;
        step!(runner, &busy_step, {
            // ctx.exec, not the tracing one: see `busy::app_busy`.
            let Some(verdict) = crate::ops::busy::app_busy(ctx.exec, vmid, &stack, app).await?
            else {
                return Ok(StepOutcome::Unchanged);
            };
            if verdict.may_update() {
                return Ok(StepOutcome::Unchanged);
            }
            skip_app = Some(crate::ops::busy::reason(&verdict));
            Ok(StepOutcome::Unchanged)
        });
        if let Some(why) = skip_app {
            runner.log(Level::Info, format!("[o10] {} skipped: {}", app, why));
            continue;
        }

        // O9: pull first, THEN stop, then start. Pulling while the service
        // still runs keeps the window in which it is down to the swap itself
        // rather than the download — which over a residential uplink is the
        // difference between seconds and minutes.
        let pull_step = format!("{} :: pull", app);
        step!(runner, &pull_step, {
            super::util_pct_sh(
                exec,
                vmid,
                &format!("cd '/opt/{}/{}' && docker compose pull -q", stack, app),
                600,
            )
            .await?;
            Ok(StepOutcome::Changed)
        });

        // O9: a container labelled `com.homelab.update.stop-first=true` is
        // stopped cleanly before the new image comes up, instead of being
        // replaced under itself. `docker compose up -d` kills and recreates,
        // and for Postgres that means the next start is a recovery — the
        // pattern mirrors `com.homelab.backup.pause`, which already does this
        // for backups.
        let stop_step = format!("{} :: stop-first", app);
        step!(runner, &stop_step, {
            let script = format!(
                "cd '/opt/{}/{}' && for c in $(docker compose ps -q); do \
                   if [ \"$(docker inspect --format '{{{{index .Config.Labels \"com.homelab.update.stop-first\"}}}}' $c)\" = true ]; then \
                     docker stop -t 60 $c; fi; done; true",
                stack, app
            );
            let out = super::util_pct_sh(exec, vmid, &script, 180).await?;
            Ok(if out.stdout.trim().is_empty() {
                StepOutcome::Unchanged
            } else {
                StepOutcome::Changed
            })
        });

        let up_step = format!("{} :: up", app);
        step!(runner, &up_step, {
            super::util_pct_sh(
                exec,
                vmid,
                &format!(
                    "cd '/opt/{}/{}' && docker compose up -d --remove-orphans",
                    stack, app
                ),
                600,
            )
            .await?;
            Ok(StepOutcome::Changed)
        });

        let verify_step = format!("{} :: verify", app);
        step!(runner, &verify_step, {
            if verify_app(exec, vmid, &stack, app).await? {
                return Ok(StepOutcome::Unchanged);
            }
            // Failed after update → roll back to the captured images (B6).
            if captured.is_empty() {
                return Err(CoreError::Other(format!(
                    "{} unhealthy after update and no captured image to roll back to",
                    app
                )));
            }
            let mut retags = String::new();
            for c in &captured {
                retags.push_str(&format!("docker tag {} {} && ", c.image_id, c.repo_tag));
            }
            super::util_pct_sh(
                exec,
                vmid,
                &format!(
                    "cd '/opt/{}/{}' && {}docker compose up -d --force-recreate",
                    stack, app, retags
                ),
                300,
            )
            .await?;
            if verify_app(exec, vmid, &stack, app).await? {
                Err(CoreError::Other(format!(
                    "{} unhealthy after update — ROLLED BACK to previous image, now healthy. \
                     The new image is bad; check its release notes",
                    app
                )))
            } else {
                Err(CoreError::Other(format!(
                    "{} unhealthy after update AND after rollback — manual intervention needed",
                    app
                )))
            }
        });

        runner.log(Level::Info, format!("[update] {} updated + verified", app));
    }

    runner.finish_ok()
}
