//! Install, first half: validate the request, keep the running binary,
//! stage the new one beside it and prove the container's glibc can run it.
//! Nothing here replaces the running binary.

use super::glibc::{glibc_probe_script, glibc_verdict};
use super::install::Paths;
use super::scripts::{preserve_script, restore_set_aside_script};
use super::steps::validate_manifest;
use crate::error::CoreError;
use crate::executor::Executor;
use crate::native::NativeServiceManifest;
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

/// "validate manifest": the manifest passes, and the unit file is the one
/// that makes the service exist.
pub(super) fn validate_install(
    m: &NativeServiceManifest,
    unit_file: &str,
) -> Result<StepOutcome, CoreError> {
    validate_manifest(m)?;
    if unit_file.trim().is_empty() {
        return Err(CoreError::Validation(format!(
            "no unit file for {} — the file that makes the service exist is not in the \
             repository, and installing a binary without it produces a container with a \
             program on it and nothing to run it",
            m.unit
        )));
    }
    if !unit_file.contains(&m.binary) {
        return Err(CoreError::Validation(format!(
            "the unit file does not exec '{}' — the same mismatch adoption refuses, caught \
             before it is written rather than after",
            m.binary
        )));
    }
    Ok(StepOutcome::Unchanged)
}

/// "preserve previous binary": whether there is something to fall back to
/// is discovered, not assumed. Sets `had_previous`.
pub(super) async fn preserve_previous(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    had_previous: &mut bool,
) -> Result<StepOutcome, CoreError> {
    let probe = util_pct_sh(
        exec,
        m.vmid,
        &format!("test -f {} && echo yes || echo no", shq(&m.binary)),
        30,
    )
    .await?;
    *had_previous = probe.stdout.trim() == "yes";
    if !*had_previous {
        return Ok(StepOutcome::Unchanged);
    }
    let out = util_pct_sh(exec, m.vmid, &preserve_script(&m.binary, &p.prev), 120).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "cannot preserve the running {} — refusing to replace a binary with no way back",
            m.binary
        )));
    }
    Ok(StepOutcome::Changed)
}

/// "stage binary": the binary travels as base64 text because that is what
/// `pct push` carries reliably; it is decoded on the container. The decoded
/// file is staged BESIDE the target rather than over it, so a transfer that
/// dies half way leaves the running service on its own binary.
pub(super) async fn stage_binary(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    binary_b64: &str,
) -> Result<StepOutcome, CoreError> {
    let b64_path = format!("{}.b64", p.staged);
    crate::ops::util::push_content(exec, m.vmid, &b64_path, binary_b64, "600").await?;
    let script = format!(
        "base64 -d {b64} > {new} && chmod 755 {new} && rm -f {b64} && test -s {new}",
        b64 = shq(&b64_path),
        new = shq(&p.staged)
    );
    let out = util_pct_sh(exec, m.vmid, &script, 300).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "could not decode the binary into {} ({}) — nothing was replaced",
            p.staged,
            out.stderr.trim()
        )));
    }
    Ok(StepOutcome::Changed)
}

/// "check the glibc the binary needs": T87: the one reading that tells a
/// crash loop from a working install, taken while the running service is
/// still untouched. On refusal the staged file goes too, so a retry starts
/// clean and nothing on the container looks half-installed. Sets `note`.
pub(super) async fn check_glibc(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    had_previous: bool,
    note: &mut String,
) -> Result<StepOutcome, CoreError> {
    let out = util_pct_sh(exec, m.vmid, &glibc_probe_script(&p.staged), 60).await?;
    match glibc_verdict(&out.stdout) {
        Ok(fine) => {
            *note = fine;
            Ok(StepOutcome::Unchanged)
        }
        Err(why) => {
            let rm = format!("rm -f {}", shq(&p.staged));
            let _ = util_pct_sh(exec, m.vmid, &rm, 30).await;
            // fix-114: nothing is installed, so the kept previous binary
            // from before this run is the kept one again.
            if had_previous {
                let back = restore_set_aside_script(&p.prev);
                let _ = util_pct_sh(exec, m.vmid, &back, 30).await;
            }
            Err(CoreError::SafetyAbort(format!(
                "{} :: the staged copy was removed; the running {} is untouched",
                why, m.unit
            )))
        }
    }
}
