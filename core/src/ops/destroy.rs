//! Gated destroy (C2): the only operation that removes a container. Guarded
//! four ways — typed-name confirmation (checked by the caller/TUI), the A1
//! no-touch list, the A2 hostname guard (live), and it lifts the Proxmox
//! protection flag deliberately before removal. Every step is journaled (B5).

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, TracingExecutor, run_ok};
use crate::runner::{OperationReport, Runner, Scope, StepFailure, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

/// A warning that reaches the transcript from inside a step body, where the
/// runner itself is mutably borrowed.
fn runner_warn(ctx: &OpCtx<'_>, msg: String) {
    ctx.sink.emit(crate::sink::PipelineEvent::Line {
        level: Level::Warn,
        source: "HOST".into(),
        msg,
    });
}

/// CORRECTIONS 2026-10-02 (home and uptime were destroyed with a backup
/// nobody had restored): between the backup and the first irreversible
/// step, every repository that backup wrote is restored to scratch and
/// judged (`restoredrill::restore_and_judge`); one that does not restore
/// stops the destroy while the container still exists.
pub const VERIFY_RESTORE_STEP: &str = "verify restore";

/// Where the restore-check restores to: the nightly drill's own default
/// scratch directory, one sub-directory per repository, removed after.
fn restore_check_target(stack: &str, repo: &str) -> String {
    format!(
        "{}/destroy-{}-{}",
        crate::ops::restoredrill::DEFAULT_DRILL_SCRATCH_DIR,
        stack,
        repo
    )
}

/// An informational line from inside a step body (see `runner_warn`).
fn runner_info(ctx: &OpCtx<'_>, msg: String) {
    ctx.sink.emit(crate::sink::PipelineEvent::Line {
        level: Level::Info,
        source: "HOST".into(),
        msg,
    });
}

/// step-22: `destroy` and `forget` both end by unregistering everything a
/// deploy registers outside the container — shared so the two cannot drift
/// apart (see `unregister` below). Every one of these steps is already
/// unconditional too.
pub const UNREGISTER_STEPS: &[&str] = &[
    "remove metrics discovery",
    "remove gateway route",
    "update state",
];

/// fix-171 round 2: destroy's own step plan — fixed, every run, because
/// every step here is already unconditional (a precondition that does not
/// hold makes the step a no-op, never an absent one; see `step!` below).
/// Shared with the dashboard (`admin::shell::actions::execute`) so Apply and
/// a batch of destroys can announce the combined total before the first one
/// starts.
pub const STEPS: &[&str] = &[
    "confirm",
    "no-touch check",
    "hostname guard",
    "backup before destroy",
    "stop container",
    "lift protection",
    "destroy container",
    "remove metrics discovery",
    "remove gateway route",
    "update state",
];

/// fix-step-plan-nested: `destroy`'s own FULL plan — `STEPS` above with its
/// single "backup before destroy" entry replaced by the `backup` op it
/// always ends up marking there (run, or skip-marked when `--no-backup` was
/// passed or the manifest declares no storage — either way every one of
/// `backup`'s names gets exactly one mark). Constant length regardless of
/// those two facts, because a skip IS a mark; the single source `destroy`
/// itself plans from and `admin`'s batch-total calls.
pub fn destroy_plan_names(stack_name: &str) -> Vec<String> {
    let mut v: Vec<String> = vec![
        "confirm".to_string(),
        "no-touch check".to_string(),
        "hostname guard".to_string(),
    ];
    v.extend(crate::ops::backup::backup_plan_names(stack_name));
    v.push(VERIFY_RESTORE_STEP.to_string());
    v.push("stop container".to_string());
    v.push("lift protection".to_string());
    v.push("destroy container".to_string());
    v.extend(UNREGISTER_STEPS.iter().map(|s| s.to_string()));
    v
}

/// Destroy a managed container by stack name + vmid. `confirmed` must be the
/// caller's proof the user typed the stack name (the TUI enforces this); we
/// re-check it here so the core is safe on its own.
pub async fn destroy(
    ctx: &OpCtx<'_>,
    manifest: &crate::manifest::StackManifest,
    confirmed_name: &str,
    skip_backup: bool,
) -> OperationReport {
    let stack_name = &manifest.stack_name;
    let mut scope = Scope::top(&format!("destroy-{}", stack_name), ctx.sink, ctx.journal);
    // fix-step-plan-nested: the FULL plan, including the `backup` this
    // always marks one way or the other (run, or skip-marked when
    // `--no-backup` was passed or the manifest declares no storage) —
    // `destroy_plan_names` is the one source this and `admin`'s batch-total
    // both call.
    scope.plan_if_top(&destroy_plan_names(stack_name));
    let result = destroy_impl(ctx, manifest, confirmed_name, skip_backup, &mut scope).await;
    scope.finish(result)
}

/// fix-step-plan-nested: the step logic behind `destroy`, run through the
/// `Scope` its own wrapper (above) already planned — it composes `backup`
/// (`crate::ops::backup::backup_impl`) as a NESTED call sharing this same
/// scope's `Runner`, rather than a second standalone op announcing its own
/// plan (the exact shape `install_native`/`adopt` and `release_install` were
/// fixed to carry the LIVE kyu counter fault).
async fn destroy_impl<'a>(
    ctx: &OpCtx<'a>,
    manifest: &crate::manifest::StackManifest,
    confirmed_name: &str,
    skip_backup: bool,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let stack_name = &manifest.stack_name;
    let vmid = manifest.vmid;
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let vm = vmid.to_string();
    let expected_hostname = format!("{}-app-{}", vmid, stack_name);

    scope.log(
        Level::Warn,
        format!(
            "[destroy] requested for {} (vmid {})",
            expected_hostname, vmid
        ),
    );

    // Gate 0: typed-name confirmation.
    scoped_step!(scope, "confirm", {
        if confirmed_name != stack_name {
            return Err(CoreError::SafetyAbort(format!(
                "typed name '{}' does not match stack '{}'",
                confirmed_name, stack_name
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Gate 1: no-touch list (A1).
    scoped_step!(scope, "no-touch check", {
        if ctx.safety.no_touch.contains(&vmid) {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} is on the no-touch list — never destroyed",
                vmid
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Gate 2: live hostname guard (A2) — the container must actually be ours.
    scoped_step!(scope, "hostname guard", {
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if !cfg.success() {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} does not exist — nothing to destroy",
                vmid
            )));
        }
        let live = cfg
            .stdout
            .lines()
            .find(|l| l.starts_with("hostname:"))
            .map(|l| l.trim_start_matches("hostname:").trim().to_string())
            .unwrap_or_default();
        if live != expected_hostname {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} has hostname '{}', expected '{}' — refusing to destroy",
                vmid, live, expected_hostname
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Kenny's B1/B2 (2026-08-31): back the stack up here, while it still
    // exists, and refuse if that fails.
    //
    // The question he asked was the right one — until tonight the backup
    // before a destroy existed only because whoever ran it remembered. The
    // procedure has always said to take one; nothing enforced it, so it was a
    // habit, and a habit is not a safety net.
    //
    // For a stack whose config lives on /appdata this costs seconds and
    // changes little: those directories survive a destroy anyway. It matters
    // for what lives INSIDE the container, which is exactly where the native
    // services keep their state.
    //
    // fix-step-plan-nested: `backup` is composed in — nested through THIS
    // scope — instead of called as a standalone op with its own plan. Both
    // early-out cases below still skip-mark every one of `backup`'s names:
    // they are part of the announced plan and every name in it gets exactly
    // one mark, run or skip.
    if skip_backup {
        runner_warn(
            ctx,
            format!(
                "[destroy] backup SKIPPED for {} at the operator's explicit request — \
                 whatever this container holds that is not on /appdata is gone in a moment",
                stack_name
            ),
        );
        for name in &crate::ops::backup::backup_plan_names(stack_name) {
            scope.skip(name);
        }
        scope.skip(VERIFY_RESTORE_STEP);
    } else if manifest.storage.is_empty() {
        // F210: said loudly rather than in silence — for a native stack
        // this is exactly wrong, since its state lives INSIDE the
        // container about to be deleted. Story: docs/deployment/REGISTER.md.
        runner_warn(
            ctx,
            format!(
                "[destroy] {} declares no storage, so there is nothing to back up \
                 from the host — anything this container holds internally goes with \
                 it",
                stack_name
            ),
        );
        for name in &crate::ops::backup::backup_plan_names(stack_name) {
            scope.skip(name);
        }
        scope.skip(VERIFY_RESTORE_STEP);
    } else {
        let mut backup_scope = scope.child(format!("backup-{}", stack_name));
        // fix-223: this backup is the safety copy taken right before the
        // container is destroyed — tagged as such, not as a manual backup.
        let backup_cfg = crate::ops::backup::BackupCfg {
            trigger: crate::ops::backup::BackupTrigger::PreDestroy,
            ..ctx.backup.clone()
        };
        crate::ops::backup::backup_impl(ctx, manifest, &backup_cfg, &mut backup_scope)
            .await
            .map_err(|f| {
                let why = crate::error::OperatorError::from_core(&f.step, &f.err).why;
                StepFailure {
                    step: f.step,
                    err: CoreError::SafetyAbort(format!(
                        "refusing to destroy '{}': the backup taken first did not succeed :: {} \
                         :: pass --no-backup to destroy anyway, which is a decision, not a retry",
                        stack_name, why
                    )),
                }
            })?;
        let repos = crate::ops::restoredrill::drill_repos(&[(
            manifest.storage.clone(),
            stack_name.clone(),
            Vec::new(),
        )]);
        scoped_step!(scope, VERIFY_RESTORE_STEP, {
            for repo in &repos {
                let target = restore_check_target(stack_name, repo);
                let outcome =
                    crate::ops::restoredrill::restore_and_judge(exec, &ctx.backup, repo, &target)
                        .await;
                let _ = exec.run(&Cmd::new("rm", &["-rf", &target], 300)).await;
                match outcome {
                    crate::ops::restoredrill::Outcome::Passed {
                        files,
                        largest_bytes,
                    } => runner_info(
                        ctx,
                        format!(
                            "[destroy] {} restores: {} file(s), largest {} bytes",
                            repo, files, largest_bytes
                        ),
                    ),
                    crate::ops::restoredrill::Outcome::Failed(why) => {
                        return Err(CoreError::SafetyAbort(format!(
                            "refusing to destroy '{}': the backup of '{}' does not restore :: {} \
                             :: the container is untouched; pass --no-backup to destroy anyway, \
                             which is a decision, not a retry",
                            stack_name, repo, why
                        )));
                    }
                }
            }
            Ok(StepOutcome::Unchanged)
        });
    }

    // Stop the container (ignore "already stopped").
    scoped_step!(scope, "stop container", {
        let _ = exec.run(&Cmd::new("pct", &["stop", &vm], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    // Lift the Proxmox protection flag deliberately (it exists precisely to
    // block accidental destroys).
    scoped_step!(scope, "lift protection", {
        let _ = exec
            .run(&Cmd::new("pct", &["set", &vm, "--protection", "0"], 30))
            .await?;
        Ok(StepOutcome::Changed)
    });

    // Destroy — purge removes it from all configs; the guest disk on the thin
    // pool goes with it. /appdata data on the host is intentionally kept.
    scoped_step!(scope, "destroy container", {
        run_ok(exec, &Cmd::new("pct", &["destroy", &vm, "--purge"], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    // step-22: whatever a deploy registers, a destroy unregisters — the same
    // list `forget` runs, so the two cannot drift apart.
    unregister(scope, ctx, exec, stack_name, vmid, Some(manifest))
        .await
        .map_err(|(step, err)| StepFailure {
            step: step.to_string(),
            err,
        })?;

    scope.log(
        Level::Info,
        format!(
            "[destroy] {} removed (data under /appdata kept for redeploy)",
            expected_hostname
        ),
    );
    Ok(())
}

/// step-22: every registration a stack has outside its container, removed
/// in one place. Destroy runs it after the container is gone; forget runs it
/// for a record whose container is already gone.
///
/// Returns the failing step's name with the error, so the caller can finish
/// its own runner with it.
async fn unregister(
    scope: &mut Scope<'_, '_>,
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    stack_name: &str,
    vmid: u16,
    manifest: Option<&crate::manifest::StackManifest>,
) -> Result<(), (&'static str, CoreError)> {
    // T1: a removed stack stops being a scrape target. Without this it would
    // keep firing HostDown on its way out — which is exactly what the
    // scratch container at 10.10.10.14 was set up to do.
    scope
        .step("remove metrics discovery", || async {
            let Some(dir) = ctx.metrics_targets_dir.as_deref() else {
                return Ok(StepOutcome::Unchanged);
            };
            let path = crate::ops::discovery::target_file(dir, stack_name);
            let _ = exec.run(&Cmd::new("rm", &["-f", &path], 30)).await;
            Ok(StepOutcome::Changed)
        })
        .await
        .map_err(|e| ("remove metrics discovery", e))?;

    scope
        .step("remove gateway route", || async {
            let mut files = vec![format!("{}-app-{}.yml", vmid, stack_name)];
            // fix-91: the `extra_routes` files this stack's deploy recorded go
            // with it. They carry a name of their own, so the derived one above
            // does not reach them, and a router left behind for a stack that
            // is gone is F115. Only recorded files: a route written by hand
            // that no deploy recorded stays (fix-41).
            let recorded = crate::state::StateStore::new(exec, &ctx.state_dir)
                .load()
                .await
                .ok()
                .and_then(|s| s.stacks.get(stack_name).cloned())
                .map(|st| st.extra_route_files)
                .unwrap_or_default();
            files.extend(recorded);
            for f in &files {
                let dest = format!("{}/{}", ctx.safety.gateway_routes_dir, f);
                let _ = exec
                    .run(&Cmd::new(
                        "pct",
                        &[
                            "exec",
                            &ctx.safety.gateway_vmid.to_string(),
                            "--",
                            "rm",
                            "-f",
                            &dest,
                        ],
                        30,
                    ))
                    .await?;
            }
            Ok(StepOutcome::Changed)
        })
        .await
        .map_err(|e| ("remove gateway route", e))?;

    // Drop the stack from HOST state (B4), and its manual checks with it: a
    // question about a stack that no longer exists can never be answered
    // meaningfully, and the fleet check would ask it forever. /appdata and
    // the secrets vault are kept so a redeploy can auto-restore (E3).
    scope
        .step("update state", || async {
            let store = crate::state::StateStore::new(exec, &ctx.state_dir);
            let mut state = store.load().await?;
            let record = state.stacks.remove(stack_name);
            state.manual_checks.retain(|_, r| r.stack != stack_name);
            state.probes.retain(|_, r| r.stack != stack_name);
            let prefix = format!("{}/", stack_name);
            state.busy_checks.retain(|k, _| !k.starts_with(&prefix));
            // ask-9: what it leaves behind is kept forever by default, and
            // recorded so the fleet check can say so and `homelab wipe` can
            // find it. The state's own manifest first (it is what applied),
            // the caller's when the stack was never recorded.
            let natives = record
                .as_ref()
                .map(|r| r.natives.clone())
                .unwrap_or_default();
            let recorded = record.as_ref().and_then(|r| r.manifest.clone());
            crate::ops::retired::retire_stack(
                &mut state,
                stack_name,
                vmid,
                recorded.as_ref().or(manifest),
                &natives,
                &ctx.state_dir,
                ctx.now_unix,
            );
            store.save(state).await?;
            Ok(StepOutcome::Changed)
        })
        .await
        .map_err(|e| ("update state", e))?;

    Ok(())
}

/// Drop a stale record, and everything registered for it, without touching
/// any container (`homelab forget`).
///
/// step-22: this used to remove the state record and nothing else, so a
/// stack forgotten after its container was lost kept its route, scrape
/// target and manual checks. It now runs the same unregister steps a destroy
/// runs.
///
/// The one guard it always had stays first: only a record whose container no
/// longer answers to that hostname may be forgotten. A live one being
/// forgotten would go silently unbacked-up.
pub async fn forget(ctx: &OpCtx<'_>, stack: &str) -> OperationReport {
    let mut scope = Scope::top(&format!("forget-{}", stack), ctx.sink, ctx.journal);
    let mut plan: Vec<&str> = vec!["read record", "live check"];
    plan.extend(UNREGISTER_STEPS);
    scope.plan_if_top(&plan);
    let result = forget_impl(ctx, stack, &mut scope).await;
    scope.finish(result)
}

async fn forget_impl<'a>(
    ctx: &OpCtx<'a>,
    stack: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let mut entry: Option<crate::state::StackState> = None;

    scoped_step!(scope, "read record", {
        let store = crate::state::StateStore::new(exec, &ctx.state_dir);
        let state = store.load().await?;
        match state.stacks.get(stack) {
            Some(st) => {
                entry = Some(st.clone());
                Ok(StepOutcome::Unchanged)
            }
            None => Err(CoreError::Other(format!(
                "no stack '{}' in host state",
                stack
            ))),
        }
    });
    let Some(entry) = entry else {
        return Err(StepFailure {
            step: scope.qualify("read record"),
            err: CoreError::Other(format!("no stack '{}' in host state", stack)),
        });
    };

    scoped_step!(scope, "live check", {
        let live = exec
            .run(&Cmd::new("pct", &["list"], 30))
            .await
            .map(|o| o.stdout)
            .unwrap_or_default();
        if live
            .lines()
            .any(|l| l.split_whitespace().any(|w| w == entry.hostname))
        {
            return Err(CoreError::SafetyAbort(format!(
                "'{}' still names a live container ({}) :: this record is current, not stale — \
                 destroy the stack or rename it first",
                stack, entry.hostname
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    unregister(scope, ctx, exec, stack, entry.vmid, None)
        .await
        .map_err(|(step, err)| StepFailure {
            step: step.to_string(),
            err,
        })?;
    scope.log(
        Level::Info,
        format!(
            "[forget] forgot '{}' (was vmid {} as {}) — the container was not touched",
            stack, entry.vmid, entry.hostname
        ),
    );
    Ok(())
}

/// Destroy a stack from the manifest recorded in host state (ask-8).
///
/// `homelab apply` destroys a stack whose directory is gone from the
/// repository, so there is no stack file to build a manifest from — but
/// state holds the one that was last applied (`StackState.manifest`). This
/// looks it up and runs the ordinary [`destroy`], so the typed name, the
/// no-touch list, the hostname guard and the backup-first rule all apply
/// unchanged. The nightly round never calls this: a destroy always needs a
/// person who typed the name.
pub async fn destroy_recorded(
    ctx: &OpCtx<'_>,
    stack: &str,
    confirmed_name: &str,
    skip_backup: bool,
) -> OperationReport {
    let refuse = |why: String| {
        let runner = Runner::new(&format!("destroy-{}", stack), ctx.sink, ctx.journal);
        runner.finish_err("read record", &CoreError::SafetyAbort(why))
    };
    let store = crate::state::StateStore::new(ctx.exec, &ctx.state_dir);
    let state = match store.load().await {
        Ok(s) => s,
        Err(e) => return refuse(format!("state unreadable: {}", e)),
    };
    let Some(record) = state.stacks.get(stack) else {
        return refuse(format!("no stack '{}' in host state", stack));
    };
    let Some(manifest) = record.manifest.as_ref() else {
        return refuse(format!(
            "'{}' has no manifest recorded in host state (an adopted service) — there is \
             nothing to destroy it from; remove the container by hand, then `homelab forget {}`",
            stack, stack
        ));
    };
    if manifest.stack_name != stack {
        return refuse(format!(
            "the record '{}' holds a manifest for '{}' — refusing to guess which one is meant",
            stack, manifest.stack_name
        ));
    }
    destroy(ctx, manifest, confirmed_name, skip_backup).await
}
