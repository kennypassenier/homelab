//! H8 (light variant): per-stack enabled flag. Disabling a stack (a) makes
//! the nightly scheduler skip it and (b) clears onboot so a parked service
//! stays parked across a host reboot — the one thing a manual `pct stop`
//! cannot give you. Enabling restores onboot to what the manifest wants.
//! The flag NEVER starts or stops containers: manual Proxmox actions are
//! always respected.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, TracingExecutor, run_ok};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

pub async fn set_enabled(ctx: &OpCtx<'_>, stack_name: &str, enabled: bool) -> OperationReport {
    let op = format!(
        "{}-{}",
        if enabled { "enable" } else { "disable" },
        stack_name
    );
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let store = crate::state::StateStore::new(exec, &ctx.state_dir);

    let mut vmid = 0u16;
    let mut hostname = String::new();
    let mut want_onboot = true;

    step!(runner, "load state", {
        let state = store.load().await?;
        let rec = state.stacks.get(stack_name).ok_or_else(|| {
            CoreError::Other(format!("stack '{}' is not in host state", stack_name))
        })?;
        vmid = rec.vmid;
        hostname = rec.hostname.clone();
        // On enable, onboot goes back to whatever the manifest declares.
        want_onboot = rec.manifest.as_ref().map(|m| m.boot.onboot).unwrap_or(true);
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, vmid, &hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "set onboot", {
        let vm = vmid.to_string();
        let onboot = if enabled && want_onboot { "1" } else { "0" };
        run_ok(
            exec,
            &Cmd::new("pct", &["set", &vm, "--onboot", onboot], 30),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "persist flag", {
        let mut state = store.load().await?;
        let rec = state.stacks.get_mut(stack_name).ok_or_else(|| {
            CoreError::Other(format!("stack '{}' vanished from host state", stack_name))
        })?;
        let flag_changed = rec.enabled != enabled;
        rec.enabled = enabled;
        // fix-59: enabling is also how a person lifts an automatic update
        // park; the stack was never disabled for it.
        let unparked = enabled && state.updates_parked.remove(stack_name).is_some();
        if !flag_changed && !unparked {
            return Ok(StepOutcome::Unchanged);
        }
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        if enabled {
            format!(
                "[enable] {} back in the nightly rotation (onboot restored)",
                stack_name
            )
        } else {
            format!(
                "[disable] {} parked — nightly runs skip it, onboot off; containers left as they are",
                stack_name
            )
        },
    );
    runner.finish_ok()
}

/// What Kenny is told when a failed nightly run parks a stack.
///
/// gap-22: it said "no onboot until re-enabled", but the automatic park is
/// state-only on purpose: onboot and the running containers stay as they
/// were, so a transient failure can never keep a stack down after a host
/// reboot. Only `homelab disable` clears onboot.
///
/// fix-59: only the automatic updates are parked; the nightly backup goes on.
pub const AUTO_PARK_NOTICE: &str = "nightly update failed — automatic updates parked (H8): the \
     nightly backup still runs, updates wait until `homelab enable`; onboot and the running \
     containers are left as they were";

/// H8: fold one stack's night into the state. Returns true when this night
/// parked something that was not parked before, so the caller sends one
/// notice rather than one every night.
///
/// fix-59 (failed-update-parks-backups, 2026-09-27): a failed night used to
/// set `enabled = false`, and a disabled stack gets no nightly backup either,
/// so one bad upstream image or one Drive error at the backup hour switched
/// the stack's backups off until somebody noticed. Now a failed update parks
/// the automatic updates only, and a failed backup parks nothing: it is
/// tried again the next night, its own failure notification goes out every
/// night it fails, and the backup-age check in `fleetcheck` escalates it.
pub fn after_night(
    state: &mut crate::state::HostState,
    stack: &str,
    update_ok: bool,
    now: u64,
) -> bool {
    if update_ok || !state.stacks.contains_key(stack) {
        return false;
    }
    if state.updates_parked.contains_key(stack) {
        return false;
    }
    state.updates_parked.insert(stack.to_string(), now);
    true
}
