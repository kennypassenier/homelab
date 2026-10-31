//! RPC handlers: stacks.

use crate::*;
use homelab_proto::*;

/// `DeployStack`.
pub(crate) async fn deploy_stack(
    state: &AppState,
    req_id: u64,
    mut spec: Box<DeploySpec>,
) -> RpcResponse {
    let exec = RealExecutor;
    // T85: the binaries this deploy installs were staged one per
    // message; fill them in here, and clear the staging area after
    // the deploy whatever its outcome — a retry stages again.
    let dir = staged_binaries_dir(&state.config.state_dir, &spec.manifest.stack_name);
    let merged = homelab_core::ops::native::merge_staged_binaries(
        &spec.manifest.natives,
        &mut spec.native_binaries,
        |unit| std::fs::read_to_string(format!("{}/{}.b64", dir, unit)).ok(),
    );
    if !merged.is_empty() {
        info!(
            "deploy {}: {} staged binar{} taken up ({})",
            spec.manifest.stack_name,
            merged.len(),
            if merged.len() == 1 { "y" } else { "ies" },
            merged.join(", ")
        );
    }
    let vmid = spec.manifest.vmid;
    let stack_name = spec.manifest.stack_name.clone();
    let resp =
        run_mutating_op_for_stack(state, &exec, req_id, "deploy", Some(&stack_name), |ctx| {
            Box::pin(async move { deploy(ctx, &spec).await })
        })
        .await;
    let _ = std::fs::remove_dir_all(&dir);
    // fix-94: every gateway deploy re-reads the house's address and
    // keeps CrowdSec's whitelist equal to it. Its own operation, so
    // its lines reach the client before this deploy's answer does,
    // and a failure there is reported as itself rather than as a
    // failed deploy.
    if home_address_after_deploy(vmid, resp.ok, state.config.safety.gateway_vmid) {
        run_mutating_op(state, &exec, req_id, "home-address-whitelist", |ctx| {
            Box::pin(async move { homelab_core::ops::homeaddress::sync_home_address(ctx).await })
        })
        .await;
    }
    resp
}

/// `DestroyStack`.
pub(crate) async fn destroy_stack(
    state: &AppState,
    req_id: u64,
    manifest: Box<StackManifest>,
    confirm: String,
    skip_backup: bool,
) -> RpcResponse {
    let exec = RealExecutor;
    run_mutating_op(state, &exec, req_id, "destroy", |ctx| {
        Box::pin(async move {
            homelab_core::ops::destroy::destroy(ctx, &manifest, &confirm, skip_backup).await
        })
    })
    .await
}

/// `UpdateStack`.
pub(crate) async fn update_stack(
    state: &AppState,
    req_id: u64,
    manifest: Box<StackManifest>,
    app: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    let stack_name = manifest.stack_name.clone();
    run_mutating_op_for_stack(state, &exec, req_id, "update", Some(&stack_name), |ctx| {
        Box::pin(async move {
            homelab_core::ops::update::update(ctx, &manifest, app.as_deref(), false).await
        })
    })
    .await
}

/// `PatchFleet`.
pub(crate) async fn patch_fleet(state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    // Targets come from state.json — only stacks we deployed.
    let store = homelab_core::state::StateStore::new(&RealExecutor, &state.config.state_dir);
    let snapshot = store.load().await.unwrap_or_default();
    let targets: Vec<(String, u16)> = snapshot
        .stacks
        .iter()
        .map(|(name, st)| (name.clone(), st.vmid))
        .collect();
    run_mutating_op(state, &exec, req_id, "patch", |ctx| {
        Box::pin(async move { homelab_core::ops::patch::patch_fleet(ctx, &targets).await })
    })
    .await
}

/// `ApplyResources`.
pub(crate) async fn apply_resources(
    state: &AppState,
    req_id: u64,
    manifest: Box<StackManifest>,
) -> RpcResponse {
    let exec = RealExecutor;
    let stack_name = manifest.stack_name.clone();
    run_mutating_op_for_stack(state, &exec, req_id, "resize", Some(&stack_name), |ctx| {
        Box::pin(async move { homelab_core::ops::resize::hot_apply(ctx, &manifest).await })
    })
    .await
}

/// redesign-flows-6: one stack's containers and `manual` images,
/// read now (the dashboard's Update flow verifies a deploy with it).
pub(crate) async fn stack_runtime(state: &AppState, req_id: u64, stack: String) -> RpcResponse {
    let exec = RealExecutor;
    let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
    let st = store.load().await.unwrap_or_default();
    let read = match st.stacks.get(&stack) {
        None => Err(format!("the host records no stack {stack}")),
        Some(s) => homelab_core::ops::pins::read_runtime(&exec, s.vmid, unix_now()).await,
    };
    match read {
        Ok(rt) => RpcResponse {
            id: req_id,
            ok: true,
            message: serde_json::json!({ "stack": stack, "runtime": rt }).to_string(),
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

/// `SetStackEnabled`.
pub(crate) async fn set_stack_enabled(
    state: &AppState,
    req_id: u64,
    stack: String,
    enabled: bool,
) -> RpcResponse {
    let exec = RealExecutor;
    run_mutating_op(state, &exec, req_id, "set-enabled", |ctx| {
        Box::pin(async move { homelab_core::ops::enable::set_enabled(ctx, &stack, enabled).await })
    })
    .await
}

/// `ListTemplates`.
pub(crate) async fn list_templates(_state: &AppState, req_id: u64) -> RpcResponse {
    let exec = RealExecutor;
    // C5: discovery instead of hardcoded strings. Two sources: OS
    // tarballs (pveam) and clonable golden template containers.
    let tarballs = exec
        .run(&Cmd::new("pveam", &["list", "local"], 60))
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
    let clones = exec
        .run(&Cmd::new(
            "sh",
            &["-c", "grep -l '^template: 1' /etc/pve/lxc/*.conf 2>/dev/null | while read f; do v=$(basename $f .conf); h=$(grep '^hostname:' $f | cut -d' ' -f2); echo \"clone:$v  $h\"; done"],
            30,
        ))
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
    let mut msg = String::from("clonable golden templates (fast):\n");
    msg.push_str(if clones.trim().is_empty() {
        "  (none — run 'homelab template-build')\n"
    } else {
        &clones
    });
    msg.push_str("\nOS templates (full bootstrap):\n");
    for line in tarballs.lines().skip(1) {
        if let Some(name) = line.split_whitespace().next() {
            msg.push_str(&format!("  {}\n", name));
        }
    }
    RpcResponse {
        id: req_id,
        ok: true,
        message: msg,
        deferred: None,
    }
}

/// `BuildTemplate`.
pub(crate) async fn build_template(
    state: &AppState,
    req_id: u64,
    temp_vmid: u16,
    version: u32,
    unprivileged: bool,
    base_template: Option<String>,
) -> RpcResponse {
    let exec = RealExecutor;
    run_mutating_op(state, &exec, req_id, "template-build", |ctx| {
        Box::pin(async move {
            let defaults = homelab_core::ops::template::TemplateCfg::default();
            let cfg = homelab_core::ops::template::TemplateCfg {
                temp_vmid,
                version,
                unprivileged,
                base_template: base_template.unwrap_or(defaults.base_template),
                ..defaults
            };
            homelab_core::ops::template::build_template(ctx, &cfg).await
        })
    })
    .await
}
