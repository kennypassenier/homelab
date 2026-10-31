//! T11: install a native service's binary and unit file into a container
//! the orchestrator has already created.
//!
//! This is the half of C7 that never existed. `stacks/kyu/lxc-compose.yml`
//! has said since 2026-08-31 that a rebuild goes "1. this manifest recreates
//! the container; 2. restore puts the data back; 3. the three binaries are
//! installed the way C7 installs them" — and step 3 was a sentence, not a
//! verb. A stack that can only be finished by the person who remembers how
//! it was built is not managed, which is the exact reason that container
//! manifest was written in the first place.
//!
//! The container is NOT created here: `homelab deploy` already does that for
//! a `native_only` stack, skipping the docker bootstrap. Duplicating it would
//! give two provisioning paths that drift.

use super::adopt::{adopt_impl, adopt_plan_names};
use super::install_activate::{activate, keep_one_previous, own_program_dir};
use super::install_config::{create_declared_dirs, install_unit_file, seed_env};
use super::install_scripts::unit_user;
use super::install_stage::{check_glibc, preserve_previous, stage_binary, validate_install};
use super::steps::guard_step;
use crate::executor::{Executor, TracingExecutor};
use crate::native::NativeServiceManifest;
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Scope, StepFailure};
use crate::sink::Level;

/// fix-171 round 2: install-native's own step plan — fixed, every run,
/// because every step here is already unconditional (a precondition that
/// does not hold, like no previous binary to preserve, makes the step a
/// no-op, never an absent one).
pub const INSTALL_STEPS: &[&str] = &[
    "validate manifest",
    "guard target",
    "preserve previous binary",
    "stage binary",
    "check the glibc the binary needs",
    "install unit file",
    "create declared directories",
    "seed the env file",
    "activate",
    "own program directory",
    "keep one previous binary",
];

/// fix-step-plan-nested: `install_native`'s own full plan — its own
/// `INSTALL_STEPS` followed by the `adopt` it always ends with, as a flat
/// sibling (`adopt_plan_names`), never double-prefixed. `own_prefix` is
/// `None` when `install_native` is the outermost call (its own names stay
/// bare, same as before this fix) or `Some("install-<unit>")` when a
/// composing caller (`release_install`) is folding this whole plan into its
/// own — the one source `install_native`'s wrapper and that composer both
/// call, so they cannot drift apart the way the admin dashboard's separate
/// constant-times-units sum once did.
pub fn install_plan_names(m: &NativeServiceManifest, own_prefix: Option<&str>) -> Vec<String> {
    let mut all: Vec<String> = match own_prefix {
        None => INSTALL_STEPS.iter().map(|s| s.to_string()).collect(),
        Some(p) => INSTALL_STEPS
            .iter()
            .map(|s| format!("{p} :: {s}"))
            .collect(),
    };
    all.extend(adopt_plan_names(&m.stack_name));
    all
}

/// The names an install derives from its manifest: the unit, where the
/// running binary is kept aside, where the new one is staged.
pub(super) struct Paths {
    pub unit: String,
    pub prev: String,
    pub staged: String,
    pub unit_path: String,
}

impl Paths {
    pub(super) fn new(m: &NativeServiceManifest) -> Self {
        let unit = format!("{}.service", m.unit);
        Paths {
            unit_path: format!("/etc/systemd/system/{}", unit),
            prev: format!("{}.homelab-prev", m.binary),
            staged: format!("{}.homelab-new", m.binary),
            unit,
        }
    }
}

/// Validate, guard, keep the running binary, stage the new one and check
/// its glibc. Returns whether there was a previous binary to fall back to.
async fn stage_phase<'a>(
    ctx: &OpCtx<'a>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    binary_b64: &str,
    unit_file: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<bool, StepFailure> {
    scoped_step!(scope, "validate manifest", validate_install(m, unit_file));
    scoped_step!(scope, "guard target", guard_step(ctx, exec, m).await);
    let mut had_previous = false;
    scoped_step!(
        scope,
        "preserve previous binary",
        preserve_previous(exec, m, p, &mut had_previous).await
    );
    scoped_step!(
        scope,
        "stage binary",
        stage_binary(exec, m, p, binary_b64).await
    );
    let mut glibc_note = String::new();
    scoped_step!(
        scope,
        "check the glibc the binary needs",
        check_glibc(exec, m, p, had_previous, &mut glibc_note).await
    );
    scope.log(Level::Info, format!("[install] {}", glibc_note));
    Ok(had_previous)
}

/// The unit file, the declared directories and the first env file.
async fn configure_phase<'a>(
    ctx: &OpCtx<'a>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    unit_file: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    scoped_step!(
        scope,
        "install unit file",
        install_unit_file(exec, m, p, unit_file).await
    );
    let service_user = unit_user(unit_file);
    scoped_step!(
        scope,
        "create declared directories",
        create_declared_dirs(exec, m, service_user.as_deref()).await
    );
    let mut seeded_keys: Vec<String> = Vec::new();
    scoped_step!(
        scope,
        "seed the env file",
        seed_env(ctx, exec, m, service_user.as_deref(), &mut seeded_keys).await
    );
    if !seeded_keys.is_empty() {
        scope.log(
            Level::Info,
            format!(
                "[install] generated the env file for {} with its first_secret_cmd; keys: {} \
                 (values not logged)",
                m.unit,
                seeded_keys.join(", ")
            ),
        );
    }
    Ok(())
}

/// Switch to the new binary, hand the service its program directory and
/// keep exactly one previous binary.
async fn activate_phase<'a>(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    unit_file: &str,
    had_previous: bool,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    scoped_step!(scope, "activate", activate(exec, m, p, had_previous).await);
    scoped_step!(
        scope,
        "own program directory",
        own_program_dir(exec, m, unit_file).await
    );
    let mut stale_kept = false;
    scoped_step!(
        scope,
        "keep one previous binary",
        keep_one_previous(exec, m, p, had_previous, &mut stale_kept).await
    );
    if stale_kept {
        scope.log(
            Level::Warn,
            format!(
                "could not settle the kept previous binary at {} — check it and {}.old by hand",
                p.prev, p.prev
            ),
        );
    }
    Ok(())
}

/// fix-step-plan-nested: the step logic behind `install_native`, written to
/// run through a `Scope` — as the outermost call or nested inside
/// `release_install`'s own composed plan.
pub(super) async fn install_native_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    binary_b64: &str,
    unit_file: &str,
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let p = Paths::new(m);

    let had_previous = stage_phase(ctx, exec, m, &p, binary_b64, unit_file, scope).await?;
    configure_phase(ctx, exec, m, &p, unit_file, scope).await?;
    activate_phase(exec, m, &p, unit_file, had_previous, scope).await?;

    // Installed and healthy: the same record adoption writes, so a service
    // built this way and one taken over by hand are indistinguishable
    // afterwards — which is the point. fix-step-plan-nested: nested through
    // THIS scope (flattened — `adopt_impl`'s marks land on whichever
    // `Runner` this call chain is ultimately `Top` for, qualified
    // "adopt-<stack> :: …", a sibling of "install-<unit> :: …" rather than
    // compounded under it), not a second standalone op with its own plan.
    let mut adopt_scope = scope.child(format!("adopt-{}", m.stack_name));
    adopt_impl(ctx, m, &mut adopt_scope).await?;

    scope.log(
        Level::Info,
        format!(
            "[install] {} on {} — binary installed, unit active, stack recorded",
            m.unit, m.hostname
        ),
    );
    Ok(())
}

/// Install `binary_b64` and `unit_file` into the container. `binary_b64` is
/// verified by the CLIENT against the release's SHA256SUMS before it is
/// sent, the same way H7 stages the host's own binary — the desktop has the
/// authenticated `gh`, so the private repository never needs a credential
/// on the Proxmox host.
///
/// A first install and a re-install are deliberately the same operation. The
/// difference that matters is whether there is a previous binary to roll back
/// to, and that is a fact about the container, not a flag the caller passes.
pub async fn install_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    binary_b64: &str,
    unit_file: &str,
) -> OperationReport {
    let mut scope = Scope::top(&format!("install-{}", m.unit), ctx.sink, ctx.journal);
    scope.plan_if_top(&install_plan_names(m, None));
    let result = install_native_impl(ctx, m, binary_b64, unit_file, &mut scope).await;
    scope.finish(result)
}
