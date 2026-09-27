//! Gated destroy (C2): the only operation that removes a container. Guarded
//! four ways — typed-name confirmation (checked by the caller/TUI), the A1
//! no-touch list, the A2 hostname guard (live), and it lifts the Proxmox
//! protection flag deliberately before removal. Every step is journaled (B5).

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::runner::{OperationReport, Runner, StepOutcome};
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

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
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
    let vmid = manifest.vmid;
    let op = format!("destroy-{}", stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let vm = vmid.to_string();
    let expected_hostname = format!("{}-app-{}", vmid, stack_name);

    runner.log(
        Level::Warn,
        format!(
            "[destroy] requested for {} (vmid {})",
            expected_hostname, vmid
        ),
    );

    // Gate 0: typed-name confirmation.
    step!(runner, "confirm", {
        if confirmed_name != stack_name {
            return Err(CoreError::SafetyAbort(format!(
                "typed name '{}' does not match stack '{}'",
                confirmed_name, stack_name
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Gate 1: no-touch list (A1).
    step!(runner, "no-touch check", {
        if ctx.safety.no_touch.contains(&vmid) {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} is on the no-touch list — never destroyed",
                vmid
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Gate 2: live hostname guard (A2) — the container must actually be ours.
    step!(runner, "hostname guard", {
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
    step!(runner, "backup before destroy", {
        if skip_backup {
            runner_warn(
                ctx,
                format!(
                    "[destroy] backup SKIPPED for {} at the operator's explicit request — \
                     whatever this container holds that is not on /appdata is gone in a moment",
                    stack_name
                ),
            );
            return Ok(StepOutcome::Unchanged);
        }
        if manifest.storage.is_empty() {
            // F210: this used to return in silence, so a stack that declares
            // no storage was destroyed with no backup and nothing said —
            // indistinguishable in the transcript from a backup that ran.
            // For a native stack that is exactly wrong: its state lives
            // INSIDE the container, which is the thing about to be deleted.
            runner_warn(
                ctx,
                format!(
                    "[destroy] {} declares no storage, so there is nothing to back up \
                     from the host — anything this container holds internally goes with \
                     it",
                    stack_name
                ),
            );
            return Ok(StepOutcome::Unchanged);
        }
        let report = crate::ops::backup::backup(ctx, manifest, &ctx.backup).await;
        if !report.ok {
            let why = report
                .error
                .as_ref()
                .map(|e| e.why.clone())
                .unwrap_or_else(|| "backup failed".into());
            return Err(CoreError::SafetyAbort(format!(
                "refusing to destroy '{}': the backup taken first did not succeed :: {} :: \
                 pass --no-backup to destroy anyway, which is a decision, not a retry",
                stack_name, why
            )));
        }
        Ok(StepOutcome::Changed)
    });

    // Stop the container (ignore "already stopped").
    step!(runner, "stop container", {
        let _ = exec.run(&Cmd::new("pct", &["stop", &vm], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    // Lift the Proxmox protection flag deliberately (it exists precisely to
    // block accidental destroys).
    step!(runner, "lift protection", {
        let _ = exec
            .run(&Cmd::new("pct", &["set", &vm, "--protection", "0"], 30))
            .await?;
        Ok(StepOutcome::Changed)
    });

    // Destroy — purge removes it from all configs; the guest disk on the thin
    // pool goes with it. /appdata data on the host is intentionally kept.
    step!(runner, "destroy container", {
        run_ok(exec, &Cmd::new("pct", &["destroy", &vm, "--purge"], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    // step-22: whatever a deploy registers, a destroy unregisters — the same
    // list `forget` runs, so the two cannot drift apart.
    if let Err((step, e)) =
        unregister(&mut runner, ctx, exec, stack_name, vmid, Some(manifest)).await
    {
        return runner.finish_err(step, &e);
    }

    runner.log(
        Level::Info,
        format!(
            "[destroy] {} removed (data under /appdata kept for redeploy)",
            expected_hostname
        ),
    );
    runner.finish_ok()
}

/// step-22: every registration a stack has outside its container, removed
/// in one place. Destroy runs it after the container is gone; forget runs it
/// for a record whose container is already gone.
///
/// Returns the failing step's name with the error, so the caller can finish
/// its own runner with it.
async fn unregister(
    runner: &mut Runner<'_>,
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    stack_name: &str,
    vmid: u16,
    manifest: Option<&crate::manifest::StackManifest>,
) -> Result<(), (&'static str, CoreError)> {
    // T1: a removed stack stops being a scrape target. Without this it would
    // keep firing HostDown on its way out — which is exactly what the
    // scratch container at 10.10.10.14 was set up to do.
    runner
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

    // T66: the dashboard was the half that was missing — a destroyed stack
    // left a panel behind showing a container that no longer exists, which
    // reads as "everything is down" rather than "this is gone".
    runner
        .step("remove grafana dashboard", || async {
            let Some(dir) = ctx.grafana_dashboards_dir.as_deref() else {
                return Ok(StepOutcome::Unchanged);
            };
            let path = crate::ops::dashboard::dashboard_file(dir, stack_name);
            // In GRAFANA'S container, not on the Proxmox host. The directory
            // is a path inside that container — the deploy writes it with
            // `pct push` — and a bare `rm -f` here ran on the host, where it
            // does not exist, and exited 0. The step reported "changed" and
            // removed nothing for as long as it existed (F162). `rm -f` on a
            // missing path SUCCEEDS, so the step could not have discovered
            // this on its own. That container was the gateway until fix-90
            // (2026-09-27) moved Grafana to the metrics stack.
            let _ = exec
                .run(&Cmd::new(
                    "pct",
                    &[
                        "exec",
                        &ctx.safety.grafana_vmid.to_string(),
                        "--",
                        "rm",
                        "-f",
                        &path,
                    ],
                    30,
                ))
                .await;
            Ok(StepOutcome::Changed)
        })
        .await
        .map_err(|e| ("remove grafana dashboard", e))?;

    runner
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
    runner
        .step("update state", || async {
            let store = crate::state::StateStore::new(exec, &ctx.state_dir);
            let mut state = store.load().await?;
            let record = state.stacks.remove(stack_name);
            state.manual_checks.retain(|_, r| r.stack != stack_name);
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

    // T51 + T49: rendered from the fleet as it now is, AFTER the route and
    // the record are gone — both files are read from exactly those.
    runner
        .step("fleet files", || async {
            Ok(crate::ops::fleetfiles::regenerate_after_removal(ctx, exec).await)
        })
        .await
        .map_err(|e| ("fleet files", e))?;
    Ok(())
}

/// Drop a stale record, and everything registered for it, without touching
/// any container (`homelab forget`).
///
/// step-22: this used to remove the state record and nothing else, so a
/// stack forgotten after its container was lost kept its route, scrape
/// target, dashboard, manual checks, front-page tile and host monitor. It
/// now runs the same unregister steps a destroy runs.
///
/// The one guard it always had stays first: only a record whose container no
/// longer answers to that hostname may be forgotten. A live one being
/// forgotten would go silently unbacked-up.
pub async fn forget(ctx: &OpCtx<'_>, stack: &str) -> OperationReport {
    let op = format!("forget-{}", stack);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let mut entry: Option<crate::state::StackState> = None;

    step!(runner, "read record", {
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
        return runner.finish_err(
            "read record",
            &CoreError::Other(format!("no stack '{}' in host state", stack)),
        );
    };

    step!(runner, "live check", {
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

    if let Err((step, e)) = unregister(&mut runner, ctx, exec, stack, entry.vmid, None).await {
        return runner.finish_err(step, &e);
    }
    runner.log(
        Level::Info,
        format!(
            "[forget] forgot '{}' (was vmid {} as {}) — the container was not touched",
            stack, entry.vmid, entry.hostname
        ),
    );
    runner.finish_ok()
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
