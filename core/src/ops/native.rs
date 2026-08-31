//! C7 adoption: take over a hand-built native-service container without
//! restarting it. Verification first — the container must really be what
//! the manifest claims — then the stack is recorded in state so the
//! nightly machinery (backup, update supervision) picks it up.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
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

    // The `homelab` tag is what makes "managed" visible in the Proxmox list.
    // It was applied only where a container is CREATED, so the two adopted
    // ones carried no tag and a filter on it silently missed them — a signal
    // that means "built by the orchestrator" while it reads as "managed by
    // the orchestrator" (Kenny's question, 2026-08-31). Adoption is the other
    // way in, so it sets the tag too. Additive: any tag somebody else put
    // there stays.
    step!(runner, "tag as managed", {
        let vm = m.vmid.to_string();
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        let tags: Vec<String> = cfg
            .stdout
            .lines()
            .find_map(|l| l.strip_prefix("tags:"))
            .map(|v| v.split(';').map(|t| t.trim().to_string()).collect())
            .unwrap_or_default();
        if tags.iter().any(|t| t == "homelab") {
            return Ok(StepOutcome::Unchanged);
        }
        let mut all: Vec<String> = tags.into_iter().filter(|t| !t.is_empty()).collect();
        all.push("homelab".into());
        let joined = all.join(";");
        run_ok(exec, &Cmd::new("pct", &["set", &vm, "--tags", &joined], 30)).await?;
        Ok(StepOutcome::Changed)
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
        // T5: several native services share one container, so adoption adds
        // to the list rather than replacing it. Re-adopting the same unit
        // replaces just that entry — which is how a manifest correction, like
        // the mailbox→kyu rename, lands without disturbing its neighbours.
        let previous = state.stacks.get(&m.stack_name);
        let last_backup = previous.map(|s| s.last_backup).unwrap_or(0);
        let mut natives: Vec<crate::native::NativeServiceManifest> =
            previous.map(|s| s.natives.clone()).unwrap_or_default();
        match natives.iter_mut().find(|n| n.unit == m.unit) {
            Some(slot) => *slot = m.clone(),
            None => natives.push(m.clone()),
        }
        let mut apps: Vec<String> = natives.iter().map(|n| n.unit.clone()).collect();
        apps.sort();
        apps.dedup();
        state.stacks.insert(
            m.stack_name.clone(),
            crate::state::StackState {
                vmid: m.vmid,
                hostname: m.hostname.clone(),
                apps,
                applied_at: ctx.now_unix,
                last_backup,
                applied_hash: String::new(),
                manifest: None,
                native: None,
                natives,
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

/// C7 nightly backup for a native stack. The data lives INSIDE the
/// container (adoption never restarts a service, so a bind-mount to
/// /appdata was never an option); the snapshot therefore streams
/// `pct exec tar` straight into `restic --stdin` — one host-side pipeline,
/// nothing written in between. Repo naming and tiered retention are the
/// same as every compose stack: `<base>/<stack>-config`.
pub async fn backup_native(
    ctx: &OpCtx<'_>,
    m: &NativeServiceManifest,
    cfg: &crate::ops::backup::BackupCfg,
) -> OperationReport {
    let op = format!("backup-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "init repo", {
        // Fails harmlessly when the repo already exists — same as host-meta.
        let _ = ctx
            .exec
            .run(&crate::ops::backup::restic_cmd(
                cfg,
                &m.unit,
                &["init"],
                120,
            ))
            .await;
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "snapshot", {
        let dirs = m
            .data_dirs
            .iter()
            .map(|d| shq(d))
            .collect::<Vec<_>>()
            .join(" ");
        // pipefail is load-bearing: without it a dead `pct exec tar` still
        // yields a "successful" empty snapshot — a backup that lies.
        let script = format!(
            "set -o pipefail; pct exec {} -- tar -cf - {} | \
             env RESTIC_REPOSITORY={}/{}-config RESTIC_PASSWORD_FILE={} \
             restic backup --stdin --stdin-filename {}-data.tar",
            // D25: named after the SERVICE, not the stack. T5 puts several
            // services on one container, and a per-stack repository would
            // fold them into one — so moving any of them elsewhere would
            // leave its history behind, which is what D25 exists to prevent.
            m.vmid,
            dirs,
            cfg.restic_base,
            m.unit,
            cfg.password_file,
            m.unit
        );
        crate::executor::run_ok(
            exec,
            &Cmd::new("sh", &["-c", &script], cfg.snapshot_timeout_s),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "retention", {
        let out = crate::executor::run_ok(
            exec,
            &crate::ops::backup::restic_cmd(cfg, &m.unit, &["snapshots", "--json"], 300),
        )
        .await?;
        let snapshots = crate::ops::backup::parse_snapshots_json(&out.stdout);
        let doomed = crate::retention::forget_list(&snapshots, &cfg.tiers, ctx.now_unix);
        if doomed.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut args: Vec<&str> = vec!["forget"];
        args.extend(doomed.iter().map(|s| s.as_str()));
        args.push("--prune");
        crate::executor::run_ok(
            exec,
            &crate::ops::backup::restic_cmd(cfg, &m.unit, &args, 900),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!(
            "[backup] {} (native, in-container) snapshot complete",
            m.stack_name
        ),
    );
    runner.finish_ok()
}

/// C7 update supervision — the safety net the app's own self-update cannot
/// be. The app updates itself (`update_cmd`, Kenny's route C+); around it
/// the homelab preserves the running binary, restarts into the new one only
/// when the binary actually changed, verifies health, and rolls back from
/// OUTSIDE the app when the new version does not come up.
pub async fn update_native(ctx: &OpCtx<'_>, m: &NativeServiceManifest) -> OperationReport {
    let op = format!("update-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let unit = format!("{}.service", m.unit);
    let prev = format!("{}.homelab-prev", m.binary);

    let Some(update_cmd) = m.update_cmd.clone() else {
        runner.log(
            Level::Info,
            format!(
                "[update] {} has no update_cmd — skipped by decision",
                m.stack_name
            ),
        );
        return runner.finish_ok();
    };

    step!(runner, "guard target", {
        super::guard_target(exec, &ctx.safety, m.vmid, &m.hostname).await?;
        Ok(StepOutcome::Unchanged)
    });

    let mut before = String::new();
    step!(runner, "preserve binary", {
        let sum = util_pct_sh(
            exec,
            m.vmid,
            &format!("sha256sum {} | cut -d' ' -f1", shq(&m.binary)),
            60,
        )
        .await?;
        before = sum.stdout.trim().to_string();
        let out = util_pct_sh(
            exec,
            m.vmid,
            &format!("cp -p {} {}", shq(&m.binary), shq(&prev)),
            60,
        )
        .await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "cannot preserve {} — refusing to update without a rollback copy",
                m.binary
            )));
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "run self-update", {
        let out = util_pct_sh(exec, m.vmid, &update_cmd, 900).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "'{}' failed: {} — binary untouched, service still on the old version",
                update_cmd,
                out.stderr.trim()
            )));
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "restart if changed", {
        let sum = util_pct_sh(
            exec,
            m.vmid,
            &format!("sha256sum {} | cut -d' ' -f1", shq(&m.binary)),
            60,
        )
        .await?;
        if sum.stdout.trim() == before {
            // Already current: no restart, no nightly service blip.
            return Ok(StepOutcome::Unchanged);
        }
        let health = format!(
            "systemctl restart {u} && for i in 1 2 3 4 5; do \
             [ \"$(systemctl is-active {u})\" = active ] && exit 0; sleep 2; done; exit 1",
            u = unit
        );
        let out = util_pct_sh(exec, m.vmid, &health, 120).await?;
        if out.success() {
            return Ok(StepOutcome::Changed);
        }
        // The armed rollback: restore the preserved binary from OUTSIDE the
        // (dead) app, restart, and report the failure loudly either way.
        let rollback = format!(
            "cp -p {prev} {bin} && systemctl restart {u} && sleep 2 && \
             [ \"$(systemctl is-active {u})\" = active ]",
            prev = shq(&prev),
            bin = shq(&m.binary),
            u = unit
        );
        let rb = util_pct_sh(exec, m.vmid, &rollback, 120).await?;
        Err(CoreError::Other(format!(
            "new {} version did not come up healthy — rolled back to the previous binary ({}); \
             investigate before the next nightly run",
            m.stack_name,
            if rb.success() {
                "service restored and active"
            } else {
                "ROLLBACK ALSO FAILED — service needs hands NOW"
            }
        )))
    });

    runner.log(
        Level::Info,
        format!("[update] {} self-update supervised — healthy", m.stack_name),
    );
    runner.finish_ok()
}
