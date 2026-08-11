//! Backup (E1) and restore (E2). Restic runs on the HOST against the stack's
//! `/appdata` paths (data survives container recreation). Repo per stack:
//! `<base>:<stack>-config`. Stateful containers are quiesced during the
//! snapshot via the `com.homelab.backup.pause` label (E4). Restore is a
//! first-class, gated operation.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// Build a Cmd that runs restic with the repo env inline (via `env`).
fn restic(base: &str, stack: &str, password_ref: &str, args: &[&str], timeout: u64) -> Cmd {
    // The host wraps this so RESTIC_PASSWORD comes from its secret store; here
    // we pass a reference the host resolves. In tests the MockExecutor just
    // records the argv.
    let repo = format!("{}:{}-config", base, stack);
    let mut full = vec![
        "env".to_string(),
        format!("RESTIC_REPOSITORY={}", repo),
        format!("RESTIC_PASSWORD_FILE={}", password_ref),
        "restic".to_string(),
    ];
    full.extend(args.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    Cmd::new(refs[0], &refs[1..], timeout)
}

pub struct BackupCfg {
    pub restic_base: String,
    /// Path to the restic password file on the host (from the secret store).
    pub password_file: String,
    /// keep-daily / keep-weekly / keep-monthly.
    pub keep_daily: u32,
    pub keep_weekly: u32,
    pub keep_monthly: u32,
}

impl Default for BackupCfg {
    fn default() -> Self {
        Self {
            restic_base: "rclone:gdrive:homelab".into(),
            password_file: "/var/lib/homelab/secrets/restic.pw".into(),
            keep_daily: 7,
            keep_weekly: 4,
            keep_monthly: 3,
        }
    }
}

/// E1: snapshot a stack's /appdata paths, quiescing paused containers.
pub async fn backup(ctx: &OpCtx<'_>, m: &StackManifest, cfg: &BackupCfg) -> OperationReport {
    let op = format!("backup-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let paths: Vec<String> = m.storage.iter().map(|s| s.host_path.clone()).collect();

    step!(runner, "init repo", {
        // Idempotent: init fails harmlessly if the repo already exists.
        let _ = exec
            .run(&restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["init"],
                120,
            ))
            .await?;
        Ok(StepOutcome::Unchanged)
    });

    // Quiesce: stop containers labeled com.homelab.backup.pause=true.
    step!(runner, "quiesce", {
        let script = "for c in $(docker ps -q --filter label=com.homelab.backup.pause=true); do docker stop $c; done; true";
        let _ = super::util_pct_sh(exec, m.vmid, script, 120).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "snapshot", {
        if paths.is_empty() {
            // No /appdata paths — nothing to snapshot (surfaced as a no-change
            // step; the transcript makes it visible).
            return Ok(StepOutcome::Unchanged);
        }
        let mut args = vec!["backup"];
        for p in &paths {
            args.push(p.as_str());
        }
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &args,
                1800,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    // Resume the paused containers.
    step!(runner, "resume", {
        let dir_cmds = m
            .apps
            .iter()
            .map(|a| format!("cd /opt/{}/{} && docker compose up -d", m.stack_name, a))
            .collect::<Vec<_>>()
            .join("; ");
        let _ = super::util_pct_sh(exec, m.vmid, &format!("{}; true", dir_cmds), 300).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "retention", {
        let (d, w, mo) = (
            cfg.keep_daily.to_string(),
            cfg.keep_weekly.to_string(),
            cfg.keep_monthly.to_string(),
        );
        let args = vec![
            "forget",
            "--keep-daily",
            &d,
            "--keep-weekly",
            &w,
            "--keep-monthly",
            &mo,
            "--prune",
        ];
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &args,
                600,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!("[backup] {} snapshot complete", m.stack_name),
    );
    runner.finish_ok()
}

/// E2: restore a stack's /appdata from a snapshot (default: latest).
/// validate → quiesce → restore → resume → verify.
pub async fn restore(
    ctx: &OpCtx<'_>,
    m: &StackManifest,
    cfg: &BackupCfg,
    snapshot: &str,
) -> OperationReport {
    let op = format!("restore-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    runner.log(
        Level::Warn,
        format!("[restore] {} from snapshot '{}'", m.stack_name, snapshot),
    );

    step!(runner, "validate snapshot", {
        let out = exec
            .run(&restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["snapshots", "--last"],
                120,
            ))
            .await?;
        if !out.success() {
            return Err(CoreError::Other("restic repo unreachable".into()));
        }
        Ok(StepOutcome::Unchanged)
    });

    // Stop the whole stack for a consistent restore.
    step!(runner, "quiesce stack", {
        for a in &m.apps {
            let _ = super::util_pct_sh(
                exec,
                m.vmid,
                &format!("cd /opt/{}/{} && docker compose down", m.stack_name, a),
                120,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "restore data", {
        run_ok(
            exec,
            &restic(
                &cfg.restic_base,
                &m.stack_name,
                &cfg.password_file,
                &["restore", snapshot, "--target", "/"],
                1800,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "resume stack", {
        for a in &m.apps {
            super::util_pct_sh(
                exec,
                m.vmid,
                &format!("cd /opt/{}/{} && docker compose up -d", m.stack_name, a),
                300,
            )
            .await?;
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "verify health", {
        for a in &m.apps {
            let out = super::util_pct_sh(
                exec,
                m.vmid,
                &format!(
                    "cd /opt/{}/{} && docker compose ps --status running --services",
                    m.stack_name, a
                ),
                60,
            )
            .await?;
            if out.stdout.trim().is_empty() {
                return Err(CoreError::Other(format!("{} not running after restore", a)));
            }
        }
        Ok(StepOutcome::Unchanged)
    });

    runner.log(
        Level::Info,
        format!("[restore] {} restored and verified", m.stack_name),
    );
    runner.finish_ok()
}
