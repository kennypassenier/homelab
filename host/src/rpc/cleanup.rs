//! RPC handlers: cleanup.

use crate::*;
use homelab_proto::*;

/// `PruneOrphans`.
pub(crate) async fn prune_orphans(
    state: &AppState,
    req_id: u64,
    manifest: Box<StackManifest>,
    spec: Box<DeploySpec>,
    confirm: String,
) -> RpcResponse {
    let exec = RealExecutor;
    if confirm != manifest.stack_name {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "confirmation '{}' does not match stack '{}' — nothing removed",
                confirm, manifest.stack_name
            ),
            deferred: None,
        };
    }
    // A1/A2: the same gate every mutating operation passes. Removing
    // files reaches into a container, so the no-touch list and the
    // hostname check apply exactly as they do to a deploy.
    if let Err(e) =
        homelab_core::safety::check_deploy_target(&exec, &state.config.safety, &manifest).await
    {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("{}", e),
            deferred: None,
        };
    }
    // The same exclusion the deploy applies: the gateway holds other
    // stacks' routes.
    let keep = homelab_core::ops::deploy::generated_dirs(&state.config.safety, manifest.vmid);
    let orphans =
        homelab_core::ops::deploy::orphan_files_keeping(&exec, &manifest, &spec, &keep).await;
    if orphans.is_empty() {
        return RpcResponse {
            id: req_id,
            ok: true,
            message: format!(
                "nothing to remove — everything under /opt/{} is in the repository",
                manifest.stack_name
            ),
            deferred: None,
        };
    }
    let mut removed = 0usize;
    for o in &orphans {
        let path = format!("/opt/{}/{}", manifest.stack_name, o);
        // -f, never -r: this removes FILES the repository dropped.
        // A directory would take whatever else is under it, which is
        // exactly the surprise this whole feature exists to avoid.
        if exec
            .run(&homelab_core::executor::Cmd::new(
                "pct",
                &["exec", &manifest.vmid.to_string(), "--", "rm", "-f", &path],
                60,
            ))
            .await
            .is_ok()
        {
            removed += 1;
            tracing::info!("[prune] removed {}", path);
        }
    }
    RpcResponse {
        id: req_id,
        ok: removed == orphans.len(),
        message: format!("removed {} of {} orphan file(s)", removed, orphans.len()),
        deferred: None,
    }
}

/// step-22: forget runs the same unregister steps destroy runs
/// (route, scrape target, dashboard, manual checks, front page, host
/// monitors) and records what the stack left behind (ask-9) — under
/// the op-lock like every other operation that writes state.
pub(crate) async fn forget_stack(state: &AppState, req_id: u64, stack: String) -> RpcResponse {
    let exec = RealExecutor;
    run_mutating_op(state, &exec, req_id, "forget", |ctx| {
        Box::pin(async move { homelab_core::ops::destroy::forget(ctx, &stack).await })
    })
    .await
}

/// ask-8: `homelab apply` destroys a stack whose directory is gone,
/// from the manifest recorded in state. Every gate of a destroy holds.
pub(crate) async fn destroy_recorded(
    state: &AppState,
    req_id: u64,
    stack: String,
    confirm: String,
    skip_backup: bool,
) -> RpcResponse {
    let exec = RealExecutor;
    run_mutating_op(state, &exec, req_id, "destroy", |ctx| {
        Box::pin(async move {
            homelab_core::ops::destroy::destroy_recorded(ctx, &stack, &confirm, skip_backup).await
        })
    })
    .await
}

/// ask-9: what a retired entry kept, listed or wiped.
pub(crate) async fn wipe_retired(
    state: &AppState,
    req_id: u64,
    name: String,
    confirm: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    match confirm {
        None => {
            let snapshot =
                match homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir)
                    .load()
                    .await
                {
                    Ok(s) => s,
                    Err(e) => {
                        return RpcResponse {
                            id: req_id,
                            ok: false,
                            message: format!("state unreadable: {}", e),
                            deferred: None,
                        };
                    }
                };
            match homelab_core::ops::retired::wipe_plan(&snapshot, &name, &state.config.state_dir) {
                Ok(plan) => RpcResponse {
                    id: req_id,
                    ok: true,
                    message: plan.render(&name),
                    deferred: None,
                },
                Err(why) => RpcResponse {
                    id: req_id,
                    ok: false,
                    message: why,
                    deferred: None,
                },
            }
        }
        Some(confirm) => {
            run_mutating_op(state, &exec, req_id, "wipe", |ctx| {
                Box::pin(
                    async move { homelab_core::ops::retired::wipe(ctx, &name, &confirm).await },
                )
            })
            .await
        }
    }
}

/// `ApplyGuards`.
pub(crate) async fn apply_guards(state: &AppState, req_id: u64, vmid: u16) -> RpcResponse {
    let exec = RealExecutor;
    // A1 still governs: the guards write files and restart docker, so
    // an untouchable guest is untouchable here too.
    if state.config.safety.no_touch.contains(&vmid) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "vmid {} is on the no-touch list :: this list is the one thing that is never worked around",
                vmid
            ),
            deferred: None,
        };
    }
    run_mutating_op(state, &exec, req_id, "apply-guards", |ctx| {
        Box::pin(async move {
            let mut runner =
                homelab_core::runner::Runner::new("apply-guards", ctx.sink, ctx.journal);
            // fix-171 round 3: one fixed step, always.
            runner.plan(homelab_core::ops::guards::APPLY_GUARDS_STEPS);
            match runner
                .step("guards", || async {
                    // gap-33: only a managed stack, after the A2
                    // hostname guard, docker guards only where it
                    // runs docker.
                    let store = homelab_core::state::StateStore::new(ctx.exec, &ctx.state_dir);
                    let snapshot = store.load().await.unwrap_or_default();
                    let managed: Vec<homelab_core::ops::guards::ManagedTarget> = snapshot
                        .stacks
                        .iter()
                        .map(|(name, st)| homelab_core::ops::guards::ManagedTarget {
                            name: name.clone(),
                            vmid: st.vmid,
                            // A stack with no stored manifest was adopted
                            // (`homelab adopt` stores none): a native
                            // service, so no docker guards (CT 118).
                            docker: st.manifest.as_ref().map(|m| !m.native_only).unwrap_or(false),
                        })
                        .collect();
                    homelab_core::ops::guards::apply_for_managed(
                        ctx.exec,
                        ctx.sink,
                        &ctx.safety,
                        &managed,
                        vmid,
                        ctx.registry_cache.as_ref(),
                    )
                    .await?;
                    Ok(homelab_core::runner::StepOutcome::Changed)
                })
                .await
            {
                Ok(_) => {
                    runner.log(
                        homelab_core::sink::Level::Info,
                        format!(
                            "[guards] {} — runaway guards applied (the docker ones only where the stack runs docker)",
                            vmid
                        ),
                    );
                    runner.finish_ok()
                }
                Err(e) => runner.finish_err("guards", &e),
            }
        })
    })
    .await
}
