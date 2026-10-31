//! Adoption's read-only checks: the unit is running and wired the way the
//! manifest claims, and the paths it names exist.

use crate::error::CoreError;
use crate::executor::Executor;
use crate::native::NativeServiceManifest;
use crate::ops::util::shq;
use crate::ops::util_pct_sh;
use crate::runner::StepOutcome;

/// "verify service": the unit must be running AND wired the way the
/// manifest claims: adopting a half-truth would make every later backup and
/// update act on the wrong paths.
pub(super) async fn verify_service(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let unit = format!("{}.service", m.unit);
    ensure_active(exec, m, &unit).await?;
    let show = util_pct_sh(
        exec,
        m.vmid,
        &format!("systemctl show {} -p ExecStart -p EnvironmentFiles", unit),
        30,
    )
    .await?;
    ensure_wiring(m, &unit, &show.stdout)?;
    Ok(StepOutcome::Unchanged)
}

async fn ensure_active(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    unit: &str,
) -> Result<(), CoreError> {
    let active = util_pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
    // Exact match: "inactive" and "failed" must not sneak through a
    // suffix check ("inactive".ends_with("active") is true — the test
    // caught exactly that).
    if active.stdout.trim() != "active" {
        return Err(CoreError::SafetyAbort(format!(
            "unit {} is not active ('{}') — adoption never starts services; start it \
             yourself and re-run",
            unit,
            active.stdout.trim()
        )));
    }
    Ok(())
}

/// The `systemctl show` output must name the manifest's binary and env file.
fn ensure_wiring(m: &NativeServiceManifest, unit: &str, shown: &str) -> Result<(), CoreError> {
    if !shown.contains(&m.binary) {
        return Err(CoreError::SafetyAbort(format!(
            "unit {} does not exec '{}' (systemd says: {}) — fix the stack file to match \
             reality, not the other way around",
            unit,
            m.binary,
            shown.trim()
        )));
    }
    if let Some(env_file) = &m.env_file
        && !shown.contains(env_file.as_str())
    {
        return Err(CoreError::SafetyAbort(format!(
            "unit {} does not read EnvironmentFile {} — fix the stack file to match \
                 reality",
            unit, env_file
        )));
    }
    Ok(())
}

/// "verify paths": binary, data dirs and env file all exist in the CT.
pub(super) async fn verify_paths(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
) -> Result<StepOutcome, CoreError> {
    let mut script = format!("test -x {}", shq(&m.binary));
    for d in &m.data_dirs {
        script.push_str(&format!(" && test -d {}", shq(d)));
    }
    if let Some(env_file) = &m.env_file {
        script.push_str(&format!(" && test -f {}", shq(env_file)));
    }
    let out = util_pct_sh(exec, m.vmid, &script, 30).await?;
    if !out.success() {
        return Err(CoreError::SafetyAbort(format!(
            "binary/data/env paths do not all exist in CT {} (checked: {} {:?} {:?})",
            m.vmid, m.binary, m.data_dirs, m.env_file
        )));
    }
    Ok(StepOutcome::Unchanged)
}
