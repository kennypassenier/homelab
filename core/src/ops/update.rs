//! Managed updates with rollback (D9 + B6). Per app: capture the running
//! image (id + repo:tag), `compose pull` + `up -d`, verify health; on a failed
//! verify re-tag the captured image back and force-recreate, then verify the
//! rollback took. Policy: the `com.homelab.update.policy` container label —
//! `auto` apps update on scheduled runs, everything else only on an explicit
//! user request (`auto=false`).

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::util::shq;
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

/// fix-61: what the pre-update copy step decided.
enum CopyOutcome {
    /// The pull brought nothing new, or the app keeps no data.
    NotNeeded,
    /// The data is at this directory.
    Copied(String),
    /// The update must not go ahead, for this reason.
    Skip(String),
}

/// fix-61: the host paths that hold this app's data. A path names its app;
/// a path without one belongs to the stack, which is this app when the stack
/// runs only one. Declared-empty and declared-reproducible paths are left out.
fn app_data_paths(m: &StackManifest, app: &str) -> Vec<String> {
    m.storage
        .iter()
        .filter(|s| !s.no_data && s.no_backup.is_none())
        .filter(|s| match s.app.as_deref() {
            Some(a) => a == app,
            None => m.apps.len() == 1,
        })
        .map(|s| s.host_path.clone())
        .collect()
}

/// fix-61: copy the app's data aside before a new image starts on it.
async fn pre_update_copy_of(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &StackManifest,
    app: &str,
    captured: &[CapturedImage],
) -> Result<CopyOutcome, CoreError> {
    let paths = app_data_paths(m, app);
    if paths.is_empty() {
        return Ok(CopyOutcome::NotNeeded);
    }
    // Did the pull bring a different image? An empty or failed answer counts
    // as yes: a copy too many costs disk for a minute, one too few costs the
    // data.
    let pulled = super::util_pct_sh(
        exec,
        m.vmid,
        &format!(
            "cd '/opt/{}/{}' && docker compose config --images | xargs -r docker image inspect \
             --format '{{{{.Id}}}}'",
            m.stack_name, app
        ),
        60,
    )
    .await?;
    let pulled_ids: Vec<&str> = pulled.stdout.split_whitespace().collect();
    if pulled.success()
        && !pulled_ids.is_empty()
        && pulled_ids
            .iter()
            .all(|id| captured.iter().any(|c| c.image_id == *id))
    {
        return Ok(CopyOutcome::NotNeeded);
    }

    // Room for it, with as much again to spare: a copy that fills the host's
    // root would take everything else down with it.
    let quoted: Vec<String> = paths.iter().map(|p| shq(p)).collect();
    let sizes = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                &format!(
                    "du -sbc {} | tail -1 | cut -f1; df -B1 --output=avail {} | tail -1",
                    quoted.join(" "),
                    shq(&ctx.state_dir)
                ),
            ],
            300,
        ))
        .await?;
    let nums: Vec<u64> = sizes
        .stdout
        .split_whitespace()
        .filter_map(|w| w.parse().ok())
        .collect();
    let [size, avail] = nums[..] else {
        return Ok(CopyOutcome::Skip(format!(
            "could not measure the data or the free space for the pre-update copy ({})",
            sizes.stderr.trim()
        )));
    };
    const SPARE: u64 = 1 << 30;
    if avail < size.saturating_mul(2).saturating_add(SPARE) {
        return Ok(CopyOutcome::Skip(format!(
            "no room for the pre-update copy: {} bytes of data, {} bytes free under {}",
            size, avail, ctx.state_dir
        )));
    }

    // A directory of its own per update: never an existing one, so no copy
    // is ever replaced.
    let dest = format!(
        "{}/pre-update/{}/{}-{}",
        ctx.state_dir, m.stack_name, app, ctx.now_unix
    );
    let exists = exec
        .run(&Cmd::new(
            "sh",
            &["-c", &format!("test -e {} && echo yes || true", shq(&dest))],
            30,
        ))
        .await?;
    if exists.stdout.trim() == "yes" {
        return Ok(CopyOutcome::Skip(format!(
            "{} already exists, and a pre-update copy is never written over",
            dest
        )));
    }

    // Paused, not stopped: the processes are frozen for the copy and carry on
    // on the old image if anything below fails.
    super::util_pct_sh(
        exec,
        m.vmid,
        &format!("cd '/opt/{}/{}' && docker compose pause", m.stack_name, app),
        60,
    )
    .await?;
    let copied = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                &format!(
                    "mkdir -p {d} && cp -a {src} {d}/",
                    d = shq(&dest),
                    src = quoted.join(" ")
                ),
            ],
            1800,
        ))
        .await;
    let unpaused = super::util_pct_sh(
        exec,
        m.vmid,
        &format!(
            "cd '/opt/{}/{}' && docker compose unpause",
            m.stack_name, app
        ),
        60,
    )
    .await;
    let copy_ok = matches!(&copied, Ok(o) if o.success());
    if !copy_ok {
        // Only the directory this step just made.
        let _ = exec
            .run(&Cmd::new(
                "sh",
                &["-c", &format!("rm -rf {}", shq(&dest))],
                600,
            ))
            .await;
        let why = match copied {
            Ok(o) => o.stderr.trim().to_string(),
            Err(e) => e.to_string(),
        };
        return Ok(CopyOutcome::Skip(format!(
            "the pre-update copy failed ({})",
            why
        )));
    }
    unpaused?;
    Ok(CopyOutcome::Copied(dest))
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
        let pull_failed = std::sync::Mutex::new(None::<String>);
        step!(runner, &pull_step, {
            let out = super::util_pct_sh(
                exec,
                vmid,
                &format!("cd '/opt/{}/{}' && docker compose pull -q", stack, app),
                600,
            )
            .await?;
            // gap-25: the exit status counts. A failed pull used to be
            // followed by `up` on the old image, a passing verify, and a
            // report saying the app was updated. It is not an error either:
            // a registry that is down for an hour must not park every stack
            // (H8) and stop its backups, so the app is left running as it
            // was and the transcript says it was not updated.
            if !out.success() {
                *pull_failed.lock().unwrap() = Some(out.stderr.trim().to_string());
                return Ok(StepOutcome::Unchanged);
            }
            Ok(StepOutcome::Changed)
        });
        let failed = pull_failed.lock().unwrap().take();
        if let Some(why) = failed {
            runner.log(
                Level::Warn,
                format!(
                    "[update] {} NOT updated: docker compose pull failed ({}); nothing was \
                     stopped or recreated, the running version is untouched",
                    app, why
                ),
            );
            continue;
        }

        // fix-61 (no-pre-update-snapshot, 2026-09-27): the rollback below
        // re-tags the old image, but a migration the new one ran on the app's
        // data stays. So before an automatic update starts a new image, the
        // app's data is copied aside, containers paused for a consistent
        // copy, into a directory of its own that nothing else writes or
        // prunes. A verified update drops that copy; a rolled-back one keeps
        // it and names it. No room, or a failed copy, means no update.
        let mut pre_update_copy: Option<String> = None;
        if auto {
            let copy_step = format!("{} :: pre-update copy", app);
            let skip_why = std::sync::Mutex::new(None::<String>);
            step!(runner, &copy_step, {
                match pre_update_copy_of(ctx, exec, m, app, &captured).await? {
                    CopyOutcome::NotNeeded => Ok(StepOutcome::Unchanged),
                    CopyOutcome::Copied(dest) => {
                        pre_update_copy = Some(dest);
                        Ok(StepOutcome::Changed)
                    }
                    CopyOutcome::Skip(why) => {
                        *skip_why.lock().unwrap() = Some(why);
                        Ok(StepOutcome::Unchanged)
                    }
                }
            });
            let skipped = skip_why.lock().unwrap().take();
            if let Some(why) = skipped {
                runner.log(
                    Level::Warn,
                    format!(
                        "[update] {} NOT updated: {} — the running version is untouched",
                        app, why
                    ),
                );
                continue;
            }
        }

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
            let out = super::util_pct_sh(
                exec,
                vmid,
                &format!(
                    "cd '/opt/{}/{}' && docker compose up -d --remove-orphans",
                    stack, app
                ),
                600,
            )
            .await?;
            // gap-25: a failed `up` is named here but does not abort: the
            // verify step after it is what rolls back to the captured image,
            // and aborting now would skip that rollback.
            if !out.success() {
                ctx.sink.emit(crate::sink::PipelineEvent::Line {
                    level: Level::Warn,
                    source: "HOST".into(),
                    msg: format!(
                        "[update] docker compose up for {} exited {}: {} — verify decides \
                         whether to roll back",
                        app,
                        out.code,
                        out.stderr.trim()
                    ),
                });
            }
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
                // A digest reference (`name:tag@sha256:…`, every pinned image
                // since fix-82) names one image for ever, and docker refuses
                // to create a tag with a digest in it. Nothing to re-tag.
                if c.repo_tag.contains('@') {
                    continue;
                }
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
            // fix-61: the image is back, the data may not be. Say where the
            // copy from before the update is; nothing removes it.
            let kept = match &pre_update_copy {
                Some(dest) => format!(
                    ". The new image may have migrated the app's data; the data as it was \
                     before the update is kept at {} (stop the app and copy it back if the old \
                     version misbehaves; delete it by hand once it is not needed)",
                    dest
                ),
                None => String::new(),
            };
            if verify_app(exec, vmid, &stack, app).await? {
                Err(CoreError::Other(format!(
                    "{} unhealthy after update — ROLLED BACK to previous image, now healthy. \
                     The new image is bad; check its release notes{}",
                    app, kept
                )))
            } else {
                Err(CoreError::Other(format!(
                    "{} unhealthy after update AND after rollback — manual intervention needed{}",
                    app, kept
                )))
            }
        });

        // fix-61: verified, so this operation's own copy has done its job.
        // Only that directory, never another: an earlier rolled-back
        // update's copy is somebody's way back and stays.
        if let Some(dest) = &pre_update_copy {
            let _ = exec
                .run(&Cmd::new(
                    "sh",
                    &["-c", &format!("rm -rf {}", shq(dest))],
                    600,
                ))
                .await;
        }

        runner.log(Level::Info, format!("[update] {} updated + verified", app));
    }

    runner.finish_ok()
}
