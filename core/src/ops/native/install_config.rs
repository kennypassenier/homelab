//! Install, second half before the switch: the unit file, the directories
//! the manifest declares and the first seeding of the env file. Every step
//! only ever ADDS what is missing; nothing existing is touched.
//!
//! native-first-install-1 (2026-10-23, CT 121 kitchen): a fresh container
//! has none of the paths the unit and the manifest declare, so the first
//! start failed on a missing EnvironmentFile and then on missing secrets.

use super::adopt_record::vault_path;
use super::install::Paths;
use super::install_scripts::{create_dirs_script, seed_env_script};
use super::seal::seal_one;
use crate::error::CoreError;
use crate::executor::{CmdOutput, Executor};
use crate::native::NativeServiceManifest;
use crate::ops::{OpCtx, util_pct_sh};
use crate::runner::StepOutcome;

/// "install unit file": write it and let systemd re-read its units.
pub(super) async fn install_unit_file(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    p: &Paths,
    unit_file: &str,
) -> Result<StepOutcome, CoreError> {
    crate::ops::util::push_content(exec, m.vmid, &p.unit_path, unit_file, "644").await?;
    let out = util_pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "systemctl daemon-reload failed: {}",
            out.stderr.trim()
        )));
    }
    Ok(StepOutcome::Changed)
}

/// "create declared directories": the data dirs and the env file's directory.
pub(super) async fn create_declared_dirs(
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    service_user: Option<&str>,
) -> Result<StepOutcome, CoreError> {
    let script = create_dirs_script(m, service_user);
    if script.is_empty() {
        return Ok(StepOutcome::Unchanged);
    }
    let out = util_pct_sh(exec, m.vmid, &script, 60).await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "could not create the declared directories in CT {} ({}) — check that the unit's \
             User= exists in the container and that the paths are writable, then re-run \
             the install",
            m.vmid,
            out.stderr.trim()
        )));
    }
    Ok(if out.stdout.contains("created") {
        StepOutcome::Changed
    } else {
        StepOutcome::Unchanged
    })
}

/// What the seeding script said: `None` when a non-empty env file was kept,
/// otherwise the names of the keys it wrote. Exit 10: the command failed;
/// 11: it printed no KEY=value lines; anything else: writing failed.
fn seed_verdict(
    out: &CmdOutput,
    m: &NativeServiceManifest,
    env: &str,
    cmd: &str,
) -> Result<Option<Vec<String>>, CoreError> {
    let first = out.stdout.lines().next().unwrap_or("").trim();
    match out.code {
        0 if first == "KEEP" => Ok(None),
        0 => Ok(Some(
            out.stdout
                .lines()
                .filter_map(|l| l.strip_prefix("KEY "))
                .map(str::to_string)
                .collect(),
        )),
        10 => Err(CoreError::Other(format!(
            "first_secret_cmd `{}` failed in CT {} — nothing was written to {}. Run it \
             by hand under a terminal (`script -qec '{}' /dev/null`) to see why, or \
             create {} yourself (mode 0600, owned by the service user), then re-run \
             the install",
            cmd, m.vmid, env, cmd, env
        ))),
        11 => Err(CoreError::Other(format!(
            "first_secret_cmd `{}` printed no KEY=value lines — nothing was written to \
             {}. Fix the command in the stack file (it must print KEY=value lines) or \
             create {} yourself, then re-run the install",
            cmd, env, env
        ))),
        _ => Err(CoreError::Other(format!(
            "could not write {} in CT {} while seeding it — nothing was kept; check the \
             directory and the service user, then re-run the install",
            env, m.vmid
        ))),
    }
}

/// "seed the env file": runs inside the container and writes there; the
/// values never reach the host, only the key names come back
/// (`pct_sh_secret` is quiet). Sets `seeded_keys`.
pub(super) async fn seed_env(
    ctx: &OpCtx<'_>,
    exec: &dyn Executor,
    m: &NativeServiceManifest,
    service_user: Option<&str>,
    seeded_keys: &mut Vec<String>,
) -> Result<StepOutcome, CoreError> {
    let (Some(env), Some(cmd)) = (&m.env_file, &m.first_secret_cmd) else {
        return Ok(StepOutcome::Unchanged);
    };
    let script = seed_env_script(env, cmd, service_user);
    let out = crate::executor::pct_sh_secret(exec, m.vmid, &script, 120).await?;
    match seed_verdict(&out, m, env, cmd)? {
        None => return Ok(StepOutcome::Unchanged),
        Some(keys) => *seeded_keys = keys,
    }
    seal_one(exec, m.vmid, env, &vault_path(ctx, m, env)).await?;
    Ok(StepOutcome::Changed)
}
