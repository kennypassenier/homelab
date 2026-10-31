//! B1: the orchestrator's own update of a native service from a signed
//! GitHub release, and the dashboard's install of a chosen release.
//!
//! The client's `install-native` fetches through `gh`; the host has no `gh`
//! and needs none for a public repository — `curl` against the GitHub API
//! reaches it from pve (measured 200 on 2026-09-20). The decision is made
//! on checksums, not versions: SHA256SUMS is fetched first (a few hundred
//! bytes), and only when the listed sum differs from the installed binary's
//! is the asset downloaded, verified on the host, and handed to
//! `install_native` — the same staged-beside, glibc-checked, rollback-armed
//! path a client install takes. The unit file comes from the container
//! itself: it is the one systemd is running, and a rebuild put it there
//! from the repository.
//!
//! The plans live in `release_plan`, the verification half in
//! `release_verify`, the individual steps in `release_steps`.

use super::install::install_native_impl;
use super::release::{ReleaseRefs, valid_tag};
use super::release_plan::release_install_plan_names;
use super::release_steps::{Wanted, download_asset, read_unit_file};
use super::release_verify::verify_phase;
use super::steps::guard_step;
use crate::error::CoreError;
use crate::executor::{Executor, TracingExecutor};
use crate::native::NativeServiceManifest;
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Scope, StepFailure};
use crate::sink::Level;

/// Update `m` to the latest signed release of its `release_repo`.
pub async fn release_update(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    release_install(ctx, m, None, None).await
}

/// TUI parity round (dash-install-native): `install-native` of a chosen
/// release from the dashboard, which has no `gh`. The host downloads the
/// release named `tag` itself, as [`release_update`] downloads the latest:
/// the signature over SHA256SUMS (refused, not skipped, when the release is
/// unsigned: a person asked for this one), the binary against it, and the
/// unit file from the repository (`unit_file`, sent by the dashboard from
/// its working copy, as the CLI sends it). Installed through
/// [`install_native`](super::install_native), staged beside, glibc-checked,
/// rollback armed.
pub async fn install_release(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    tag: &str,
    unit_file: &str,
) -> OperationReport {
    release_install(ctx, m, Some(tag), Some(unit_file)).await
}

async fn release_install(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    pinned: Option<&str>,
    unit_from_repo: Option<&str>,
) -> OperationReport {
    let op = match pinned {
        None => format!("release-update-{}", m.unit),
        Some(_) => format!("install-native-{}", m.unit),
    };
    let mut scope = Scope::top(&op, ctx.sink, ctx.journal);
    if m.release_repo.is_none() {
        // fix-171 round 3: nothing is ever attempted here, so there is
        // nothing to plan either — the same "zero marks, no plan needed"
        // shape as `backup_native`'s stateless early return.
        scope.log(
            Level::Info,
            format!(
                "[release] {} declares no release_repo — nothing to fetch",
                m.unit
            ),
        );
        return scope.finish(Ok(()));
    }
    // fix-step-plan-nested: the FULL plan, including the `install_native` +
    // `adopt` this always ends with when it gets that far — one source
    // (`release_install_plan_names`) shared with `admin`'s batch sum, so the
    // two can never drift apart the way `RELEASE_UPDATE_STEPS.len() * units`
    // once did (18 announced for kyu where 69 marks actually ran, LIVE
    // 2026-10-02).
    scope.plan_if_top(&release_install_plan_names(m, pinned, unit_from_repo));
    let result = release_install_impl(ctx, m, (pinned, unit_from_repo), &mut scope).await;
    scope.finish(result)
}

/// Download the verified asset, take the unit file and install both through
/// `install_native_impl`, nested in this run's scope.
async fn install_phase<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    (w, unit_from_repo): (&Wanted<'_>, Option<&str>),
    (refs, wanted): (&ReleaseRefs, &str),
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let staged = format!(
        "{}/staged/{}/{}.release",
        ctx.state_dir, m.stack_name, m.unit
    );
    let mut b64 = String::new();
    scoped_step!(
        scope,
        "download and verify the asset",
        download_asset(exec, w, (refs, wanted, &staged), &mut b64).await
    );
    let mut unit_file = unit_from_repo.map(str::to_string).unwrap_or_default();
    if unit_from_repo.is_none() {
        scoped_step!(
            scope,
            "read the unit file from the container",
            read_unit_file(exec, m, &mut unit_file).await
        );
    }
    // fix-step-plan-nested: nested through THIS scope (flattened to the one
    // underlying `Runner`, qualified "install-<unit> :: …" — `install_native`
    // itself goes on to nest `adopt` the same way), not a second and third
    // standalone op each announcing their own plan.
    let mut install_scope = scope.child(format!("install-{}", m.unit));
    install_native_impl(ctx, m, &b64, &unit_file, &mut install_scope).await
}

/// A pinned tag must be a release tag as a URL path segment may carry it.
fn check_tag(pinned: Option<&str>) -> Result<(), StepFailure> {
    match pinned {
        Some(tag) if !valid_tag(tag) => Err(StepFailure {
            step: "check the tag".to_string(),
            err: CoreError::Other(format!(
                "{tag:?} is not a release tag (letters, digits, dots and dashes)"
            )),
        }),
        _ => Ok(()),
    }
}

/// fix-step-plan-nested: the step logic behind `release_install`, run
/// through the `Scope` its own wrapper (above) already planned — it ends by
/// composing `install_native` (which itself composes `adopt`) as NESTED
/// calls sharing this same scope's `Runner`, rather than two more
/// standalone ops each announcing their own plan.
async fn release_install_impl<'a>(
    ctx: &OpCtx<'a>,
    m: &NativeServiceManifest,
    (pinned, unit_from_repo): (Option<&str>, Option<&str>),
    scope: &mut Scope<'_, 'a>,
) -> Result<(), StepFailure> {
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    // Checked by the wrapper before this is even called — a plan was
    // already announced, so a repository is known to exist.
    let repo = m.release_repo.clone().expect("checked by release_install");
    let asset = m.release_asset.clone().unwrap_or_else(|| m.unit.clone());
    let w = Wanted {
        repo: &repo,
        asset: &asset,
        pinned,
    };

    scoped_step!(scope, "guard target", guard_step(ctx, exec, m).await);
    check_tag(pinned)?;
    let Some((refs, wanted)) = verify_phase(exec, m, (&w, unit_from_repo), scope).await? else {
        return Ok(());
    };
    install_phase(ctx, m, (&w, unit_from_repo), (&refs, &wanted), scope).await?;
    scope.log(
        Level::Info,
        format!(
            "[release] {} updated to {} of {} — installed under the armed rollback",
            m.unit, refs.tag, repo
        ),
    );
    Ok(())
}
