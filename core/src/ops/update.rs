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

use super::OpCtx;
use super::util::shq;

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
    let script = super::util::app_dir_script(stack, app)
        .raw("docker compose ps -q | xargs -r docker inspect --format '{{.Image}} {{.Config.Image}}'")
        .build();
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

/// fix-117 (compose-policy-first-container, 2026-09-27): which services of an
/// app a scheduled run may update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoScope {
    /// No service carries `auto`.
    Skip,
    /// Every service carries `auto`: the app is updated whole, as before.
    All,
    /// Only these services carry `auto`; the others keep their label.
    Only(Vec<String>),
}

/// fix-117: the scope from each container's `(policy, service)`, read per
/// container rather than from the app's first one. A service whose name
/// cannot be passed to compose as it is makes a mixed app skip rather than
/// guess. Story: `docs/deployment/REGISTER.md`.
pub fn auto_scope(policies: &[(String, String)]) -> AutoScope {
    let auto: Vec<&(String, String)> = policies.iter().filter(|(p, _)| p == "auto").collect();
    if auto.is_empty() {
        return AutoScope::Skip;
    }
    if auto.len() == policies.len() {
        return AutoScope::All;
    }
    let nameable = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    if auto.iter().any(|(_, s)| !nameable(s)) {
        return AutoScope::Skip;
    }
    let mut only: Vec<String> = auto.iter().map(|(_, s)| s.clone()).collect();
    only.sort();
    only.dedup();
    AutoScope::Only(only)
}

/// fix-117: `(policy, service)` for every container of the app.
async fn service_policies(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<Vec<(String, String)>, CoreError> {
    let script = super::util::app_dir_script(stack, app)
        .raw(
            "docker compose ps -q | xargs -r docker inspect --format \
             '{{index .Config.Labels \"com.homelab.update.policy\"}}|{{index .Config.Labels \"com.docker.compose.service\"}}'",
        )
        .build();
    let out = super::util_pct_sh(exec, vmid, &script, 60).await?;
    Ok(out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_once('|') {
            Some((p, s)) => (p.trim().to_string(), s.trim().to_string()),
            None => (l.to_string(), String::new()),
        })
        .collect())
}

/// fix-118 (F300, ported from the native units; story:
/// `docs/deployment/REGISTER.md`): this asks whether the app STAYS up, not
/// just whether it started:
///
/// - every service in `want` (the ones that ran before the update) is
///   running within 30 s,
/// - and stays running through a 60 s settle window,
/// - with every container's `RestartCount` unchanged — a counter, which no
///   lucky sampling can hide a restart loop from,
/// - and no container `unhealthy`; one whose healthcheck is still `starting`
///   gets up to two more minutes to say `healthy`.
///
/// `services` is the fix-117 scope (` svc1 svc2`, or empty for the app).
/// Each failure prints its own token, so the 04:00 reader can tell "never
/// came up" from "came up and died".
pub fn settle_script(stack: &str, app: &str, want: &[String], services: &str) -> String {
    format!(
        "cd {dir} || exit 1; \
         want='{want}'; \
         running() {{ docker compose ps --services --status running{svcs} | sort; }}; \
         missing() {{ r=$(running); for w in $want; do echo \"$r\" | grep -qx \"$w\" || echo \"$w\"; done; }}; \
         counts() {{ docker compose ps -q{svcs} | xargs -r docker inspect --format '{{{{.Name}}}} {{{{.RestartCount}}}}' | sort; }}; \
         health() {{ docker compose ps -q{svcs} | xargs -r docker inspect --format '{{{{.Name}}}} {{{{if .State.Health}}}}{{{{.State.Health.Status}}}}{{{{end}}}}'; }}; \
         i=0; while [ -n \"$(missing)\" ] && [ $i -lt 15 ]; do sleep 2; i=$((i+1)); done; \
         m=$(missing); [ -z \"$m\" ] || {{ echo NOT_RUNNING $m; exit 1; }}; \
         r0=$(counts); i=0; \
         while [ $i -lt 12 ]; do sleep 5; \
           m=$(missing); [ -z \"$m\" ] || {{ echo DIED_IN_WINDOW $m; exit 1; }}; \
           [ \"$(counts)\" = \"$r0\" ] || {{ echo RESTART_LOOP; exit 1; }}; \
           u=$(health | grep ' unhealthy$'); [ -z \"$u\" ] || {{ echo UNHEALTHY $u; exit 1; }}; \
           i=$((i+1)); done; \
         i=0; while health | grep -q ' starting$'; do \
           [ $i -ge 24 ] && {{ echo NEVER_HEALTHY; exit 1; }}; sleep 5; i=$((i+1)); done; \
         u=$(health | grep ' unhealthy$'); [ -z \"$u\" ] || {{ echo UNHEALTHY $u; exit 1; }}; \
         echo HEALTHY",
        dir = shq(&format!("/opt/{}/{}", stack, app)),
        want = want.join(" "),
        svcs = services
    )
}

/// fix-118: the services that were running before the update — the ones the
/// settle check requires afterwards. A service that was already down is not
/// the update's to raise.
async fn running_services(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
    services: &str,
) -> Result<Vec<String>, CoreError> {
    // fix-132/fix-133: `--format json` through the Unknown-carrying parser.
    // A probe that could not be read is treated as before this fix found no
    // reader to prefer: nothing was running yet.
    let names = super::util::compose_running_services(exec, vmid, stack, app, services)
        .await?
        .unwrap_or_default();
    let mut names: Vec<String> = names
        .into_iter()
        .filter(|l| {
            !l.is_empty()
                && l.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// fix-118: run the settle check. The outer error is the executor's; the
/// inner one is the check's verdict.
async fn settle(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
    want: &[String],
    services: &str,
) -> Result<Result<(), String>, CoreError> {
    let out =
        super::util_pct_sh(exec, vmid, &settle_script(stack, app, want, services), 330).await?;
    if out.success() {
        return Ok(Ok(()));
    }
    let why = format!("{} {}", out.stdout.trim(), out.stderr.trim());
    Ok(Err(why.trim().to_string()))
}

/// fix-118: did `up -d` start different images from the ones captured?
fn images_changed(before: &[CapturedImage], after: &[CapturedImage]) -> bool {
    let ids = |v: &[CapturedImage]| {
        let mut ids: Vec<String> = v.iter().map(|c| c.image_id.clone()).collect();
        ids.sort();
        ids
    };
    ids(before) != ids(after)
}

async fn verify_app(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    app: &str,
) -> Result<bool, CoreError> {
    // fix-132/fix-133: JSON + Unknown. A probe that could not be read is, as
    // before this fix gave it its own name, not healthy either.
    let running = super::util::compose_running_services(exec, vmid, stack, app, "").await?;
    Ok(matches!(running, Some(r) if !r.is_empty()))
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
    services: &str,
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
        &super::util::app_dir_script(&m.stack_name, app)
            .raw(&format!(
                "docker compose config --images{} | xargs -r docker image inspect --format '{{{{.Id}}}}'",
                services
            ))
            .build(),
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
    super::util::compose_in_app(exec, m.vmid, &m.stack_name, app, "docker compose pause", 60)
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
    let unpaused = super::util::compose_in_app(
        exec,
        m.vmid,
        &m.stack_name,
        app,
        "docker compose unpause",
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

/// fix-171 round 2: one app's own step names, in order — known from the app
/// name and whether this is a scheduled run (`auto`) alone, before anything
/// runs. `pre-update copy` only exists this run when `auto` is true (a
/// manual, operator-requested update never takes the automatic data copy);
/// that is a fact about the WHOLE run, the same for every app in it, not a
/// per-app guess, so the plan still knows it up front.
fn app_steps(app: &str, auto: bool) -> Vec<String> {
    let mut v = vec![
        format!("{app} :: policy"),
        format!("{app} :: capture"),
        format!("{app} :: running before"),
        format!("{app} :: busy check"),
        format!("{app} :: pull"),
    ];
    if auto {
        v.push(format!("{app} :: pre-update copy"));
    }
    v.push(format!("{app} :: stop-first"));
    v.push(format!("{app} :: up"));
    v.push(format!("{app} :: verify"));
    v
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

    // fix-171 round 2: every app's step names, known from `apps` + `auto`
    // alone — before "safety gates" even starts. Whether an app's LATER
    // steps actually run depends on what "policy"/"busy check"/"pull"
    // discover once they run (host-side facts), but the NAMES they could
    // ever produce are fixed right here, so the announced plan is complete
    // and a skipped one still fills its slot (see the `continue` points
    // below).
    let mut plan: Vec<String> = vec!["safety gates".to_string()];
    for app in &apps {
        plan.extend(app_steps(app, auto));
    }
    runner.plan(&plan.iter().map(String::as_str).collect::<Vec<_>>());

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

        // fix-171 round 2: this app's own step names, computed once so every
        // `continue` below can skip exactly the ones it will not reach.
        let names = app_steps(app, auto);
        let skip_from = |runner: &mut Runner, from: usize| {
            for n in &names[from..] {
                runner.skip(n);
            }
        };
        let (policy_step, cap_step, ran_step, busy_step, pull_step) = (
            names[0].clone(),
            names[1].clone(),
            names[2].clone(),
            names[3].clone(),
            names[4].clone(),
        );
        let stop_step = format!("{app} :: stop-first");
        let up_step = format!("{app} :: up");
        let verify_step = format!("{app} :: verify");

        // Policy gate (D9): scheduled runs only touch policy=auto apps.
        // fix-117: read per service, not from the first container.
        let mut scope = AutoScope::All;
        let skipped = step!(runner, &policy_step, {
            if auto {
                scope = auto_scope(&service_policies(exec, vmid, &stack, app).await?);
                if scope == AutoScope::Skip {
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
            skip_from(&mut runner, 1);
            continue;
        }
        // fix-117: the services this run may touch, as compose arguments; an
        // empty string is the whole app, exactly as before.
        let services: String = match &scope {
            AutoScope::Only(only) => {
                runner.log(
                    Level::Info,
                    format!(
                        "[update] {}: only {} carry policy 'auto'; the other services keep \
                         their label and are not pulled or recreated",
                        app,
                        only.join(", ")
                    ),
                );
                format!(" {}", only.join(" "))
            }
            _ => String::new(),
        };

        // B6: capture the running images so a bad update can be undone.
        let mut captured: Vec<CapturedImage> = Vec::new();
        step!(runner, &cap_step, {
            captured = capture_app(exec, vmid, &stack, app).await?;
            Ok(StepOutcome::Unchanged)
        });

        // fix-118: what ran before, which is what must run after.
        let mut ran_before: Vec<String> = Vec::new();
        step!(runner, &ran_step, {
            ran_before = running_services(exec, vmid, &stack, app, &services).await?;
            Ok(StepOutcome::Unchanged)
        });

        // O10: ask before walking in. Only Jellyfin is asked, because it is
        // the only service here where an update lands in somebody's evening —
        // and the check fails CLOSED, so an unreachable or unparseable answer
        // skips the update rather than assuming nobody is watching.
        let mut skip_app: Option<String> = None;
        step!(runner, &busy_step, {
            // ctx.exec, not the tracing one: see `busy::app_busy`.
            let Some(verdict) =
                crate::ops::busy::app_busy(ctx.exec, &ctx.state_dir, vmid, &stack, app).await?
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
            skip_from(&mut runner, 4);
            continue;
        }

        // O9: pull first, THEN stop, then start. Pulling while the service
        // still runs keeps the window in which it is down to the swap itself
        // rather than the download — which over a residential uplink is the
        // difference between seconds and minutes.
        let pull_failed = std::sync::Mutex::new(None::<String>);
        step!(runner, &pull_step, {
            let out = super::util_pct_sh(
                exec,
                vmid,
                &super::util::app_dir_script(&stack, app)
                    .raw(&format!("docker compose pull -q{}", services))
                    .build(),
                600,
            )
            .await?;
            // gap-25: the exit status counts, but is not treated as an
            // error either — a registry that is down for an hour must not
            // park every stack (H8) and stop its backups, so the app is
            // left running as it was and the transcript says it was not
            // updated. Story: docs/deployment/REGISTER.md.
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
            skip_from(&mut runner, 5);
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
            // index 5 is "pre-update copy" whenever `auto` is true (see
            // `app_steps`).
            let copy_step = &names[5];
            let skip_why = std::sync::Mutex::new(None::<String>);
            step!(runner, copy_step, {
                match pre_update_copy_of(ctx, exec, m, app, &captured, &services).await? {
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
                skip_from(&mut runner, 6);
                continue;
            }
        }

        // O9: a container labelled `com.homelab.update.stop-first=true` is
        // stopped cleanly before the new image comes up, instead of being
        // replaced under itself. `docker compose up -d` kills and recreates,
        // and for Postgres that means the next start is a recovery — the
        // pattern mirrors `com.homelab.backup.pause`, which already does this
        // for backups.

        step!(runner, &stop_step, {
            let script = super::util::app_dir_script(&stack, app)
                .raw(&format!(
                    "for c in $(docker compose ps -q{}); do \
                       if [ \"$(docker inspect --format '{{{{index .Config.Labels \"com.homelab.update.stop-first\"}}}}' $c)\" = true ]; then \
                         docker stop -t 60 $c; fi; done; true",
                    services
                ))
                .build();
            let out = super::util_pct_sh(exec, vmid, &script, 180).await?;
            Ok(if out.stdout.trim().is_empty() {
                StepOutcome::Unchanged
            } else {
                StepOutcome::Changed
            })
        });

        step!(runner, &up_step, {
            let verb = if services.is_empty() {
                "docker compose up -d --remove-orphans".to_string()
            } else {
                // fix-117: only the auto services, and not the services they
                // depend on, which keep their own label.
                format!("docker compose up -d --no-deps{}", services)
            };
            let out = super::util_pct_sh(
                exec,
                vmid,
                &super::util::app_dir_script(&stack, app).raw(&verb).build(),
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

        let mut settle_why: Option<String> = None;
        step!(runner, &verify_step, {
            // fix-118: the quick reading first; then, when `up -d` started a
            // new image, whether the app stays up (F300). A night that brought
            // nothing new costs no settle window.
            let mut healthy = verify_app(exec, vmid, &stack, app).await?;
            if healthy {
                let after = capture_app(exec, vmid, &stack, app).await?;
                if images_changed(&captured, &after)
                    && let Err(why) =
                        settle(exec, vmid, &stack, app, &ran_before, &services).await?
                {
                    settle_why = Some(why);
                    healthy = false;
                }
            }
            if healthy {
                return Ok(StepOutcome::Unchanged);
            }
            let settled = match &settle_why {
                Some(why) => format!(" (settle check: {})", why),
                None => String::new(),
            };
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
                &super::util::app_dir_script(&stack, app)
                    .raw(&format!(
                        "{}docker compose up -d --force-recreate{}",
                        retags, services
                    ))
                    .build(),
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
            // fix-118: the rollback is held to the same settle check.
            let back = verify_app(exec, vmid, &stack, app).await?
                && settle(exec, vmid, &stack, app, &ran_before, &services)
                    .await?
                    .is_ok();
            if back {
                Err(CoreError::Other(format!(
                    "{} unhealthy after update{} — ROLLED BACK to previous image, now healthy. \
                     The new image is bad; check its release notes{}",
                    app, settled, kept
                )))
            } else {
                Err(CoreError::Other(format!(
                    "{} unhealthy after update{} AND after rollback — manual intervention \
                     needed{}",
                    app, settled, kept
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
