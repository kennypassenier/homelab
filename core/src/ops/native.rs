//! C7 adoption: take over a hand-built native-service container without
//! restarting it. Verification first — the container must really be what
//! the manifest claims — then the stack is recorded in state so the
//! nightly machinery (backup, update supervision) picks it up.

use crate::error::CoreError;
use crate::executor::{Executor, TracingExecutor};
use crate::native::NativeServiceManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::{util_pct_sh, OpCtx};

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// Adopt an existing container as a managed native-service stack. Never
/// starts, stops or restarts anything — a running production service is
/// exactly what must stay untouched while the homelab takes ownership.
pub async fn adopt(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    let op = format!("adopt-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    step!(runner, "validate manifest", {
        crate::native::validate_native(m)
            .map_err(|p| CoreError::SafetyAbort(format!("native manifest: {}", p.join("; "))))?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    // The unit must be running AND wired the way the manifest claims:
    // adopting a half-truth would make every later backup and update act on
    // the wrong paths.
    step!(runner, "verify service", {
        let unit = format!("{}.service", m.unit);
        let active =
            util_pct_sh(exec, m.vmid, &format!("systemctl is-active {}", unit), 30).await?;
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
        let show = util_pct_sh(
            exec,
            m.vmid,
            &format!("systemctl show {} -p ExecStart -p EnvironmentFiles", unit),
            30,
        )
        .await?;
        if !show.stdout.contains(&m.binary) {
            return Err(CoreError::SafetyAbort(format!(
                "unit {} does not exec '{}' (systemd says: {}) — fix the stack file to match \
                 reality, not the other way around",
                unit,
                m.binary,
                show.stdout.trim()
            )));
        }
        if let Some(env_file) = &m.env_file {
            if !show.stdout.contains(env_file.as_str()) {
                return Err(CoreError::SafetyAbort(format!(
                    "unit {} does not read EnvironmentFile {} — fix the stack file to match \
                     reality",
                    unit, env_file
                )));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "verify paths", {
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
    });

    step!(runner, "record state", {
        let store = crate::state::StateStore::new(exec, &ctx.state_dir);
        let mut state = store.load().await?;
        if let Some(existing) = state.stacks.get(&m.stack_name) {
            if existing.vmid != m.vmid {
                return Err(CoreError::SafetyAbort(format!(
                    "stack '{}' already exists on vmid {} — refusing to re-point it to {}",
                    m.stack_name, existing.vmid, m.vmid
                )));
            }
        }
        state.stacks.insert(
            m.stack_name.clone(),
            crate::state::StackState {
                vmid: m.vmid,
                hostname: m.hostname.clone(),
                apps: vec![m.unit.clone()],
                applied_at: ctx.now_unix,
                last_backup: state
                    .stacks
                    .get(&m.stack_name)
                    .map(|s| s.last_backup)
                    .unwrap_or(0),
                applied_hash: String::new(),
                manifest: None,
                native: Some(m.clone()),
                enabled: true,
            },
        );
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!(
            "[adopt] {} ({}) is now managed — service untouched, nightly backup + update \
             supervision from tonight",
            m.stack_name, m.hostname
        ),
    );
    runner.finish_ok()
}

fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
