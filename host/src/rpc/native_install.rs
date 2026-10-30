//! RPC handlers: installing a native service (staging a binary, adopting, installing a build or a release).

use crate::*;
use homelab_proto::*;

/// T85: a native binary arrives on its own, before the deploy that
/// installs it. Kept under the state directory, root-only, until the
/// deploy consumes it; a name that is not a plain stack or unit name
/// is refused before it can become a path.
pub(crate) async fn stage_native_binary(
    state: &AppState,
    req_id: u64,
    stack: String,
    unit: String,
    binary_b64: String,
) -> RpcResponse {
    let exec = RealExecutor;
    let plain = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    };
    if !plain(&stack) || !plain(&unit) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "refusing to stage '{}' for '{}': stack and unit names are lowercase \
                 [a-z0-9-] and nothing else",
                unit, stack
            ),
            deferred: None,
        };
    }
    let dir = staged_binaries_dir(&state.config.state_dir, &stack);
    let path = format!("{}/{}.b64", dir, unit);
    // fix-51: through the executor's atomic write (own temp name,
    // 0600 from creation, off the async worker) rather than a plain
    // `std::fs::write` of tens of MB that a deploy reading the file at
    // the same moment could see half-written.
    let written = exec.write_file(&path, &binary_b64, 0o600).await;
    match written {
        Ok(()) => RpcResponse {
            id: req_id,
            ok: true,
            message: format!(
                "staged {} for {} ({} KiB of base64) — the next deploy of this stack \
                 installs it",
                unit,
                stack,
                binary_b64.len() / 1024
            ),
            deferred: None,
        },
        Err(e) => RpcResponse {
            id: req_id,
            ok: false,
            message: format!("could not stage {} for {}: {}", unit, stack, e),
            deferred: None,
        },
    }
}

/// `AdoptService`.
pub(crate) async fn adopt_service(
    state: &AppState,
    req_id: u64,
    m: Box<NativeServiceManifest>,
) -> RpcResponse {
    let exec = RealExecutor;
    let stack_name = m.stack_name.clone();
    run_mutating_op_for_stack(state, &exec, req_id, "adopt", Some(&stack_name), |ctx| {
        Box::pin(async move { homelab_core::ops::native::adopt(ctx, &m).await })
    })
    .await
}

/// `InstallNative`.
pub(crate) async fn install_native(
    state: &AppState,
    req_id: u64,
    manifest: Box<NativeServiceManifest>,
    binary_b64: String,
    unit_file: String,
) -> RpcResponse {
    let exec = RealExecutor;
    let stack_name = manifest.stack_name.clone();
    run_mutating_op_for_stack(
        state,
        &exec,
        req_id,
        "install-native",
        Some(&stack_name),
        |ctx| {
            Box::pin(async move {
                homelab_core::ops::native::install_native(ctx, &manifest, &binary_b64, &unit_file)
                    .await
            })
        },
    )
    .await
}

/// TUI parity round: the dashboard's install-native; the host fetches
/// and verifies the release itself (CT 120 has no gh).
pub(crate) async fn install_native_release(
    state: &AppState,
    req_id: u64,
    manifest: Box<NativeServiceManifest>,
    unit_file: String,
    tag: String,
) -> RpcResponse {
    let exec = RealExecutor;
    if let Err(problems) = homelab_core::native::validate_native(&manifest) {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!("service.yml invalid: {}", problems.join("; ")),
            deferred: None,
        };
    }
    if manifest.release_repo.is_none() {
        return RpcResponse {
            id: req_id,
            ok: false,
            message: format!(
                "{} declares no release_repo — there is no release to install; add \
                 release_repo to its service.yml",
                manifest.unit
            ),
            deferred: None,
        };
    }
    let stack_name = manifest.stack_name.clone();
    run_mutating_op_for_stack(
        state,
        &exec,
        req_id,
        "install-native",
        Some(&stack_name),
        |ctx| {
            Box::pin(async move {
                homelab_core::ops::native::install_release(ctx, &manifest, &tag, &unit_file).await
            })
        },
    )
    .await
}
