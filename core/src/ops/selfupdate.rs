//! HOST self-update (H5). The client ships a new binary over the TLS line;
//! this op verifies it (`--selfcheck`), backs up the running binary, installs,
//! arms a rollback marker, and schedules a systemd restart of itself. The
//! rollback half lives in systemd: `OnFailure=` runs a script that — if the
//! marker is still present, meaning the new binary never came up healthy —
//! restores the backup. The freshly started binary clears the marker once it
//! is serving, which is what makes an update "accepted".

use crate::error::CoreError;
use crate::executor::{Cmd, Executor, TracingExecutor, run_ok};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

pub struct SelfUpdateCfg {
    /// Where the uploaded candidate binary was staged (mode 0755).
    pub staged: String,
    /// The live binary path systemd starts.
    pub current: String,
    /// Backup of the previous binary, used by the rollback unit.
    pub prev: String,
    /// Marker whose presence at failure time means "roll back"; cleared by
    /// the new binary once it serves.
    pub marker: String,
    /// The systemd service to restart.
    pub service: String,
}

impl Default for SelfUpdateCfg {
    fn default() -> Self {
        Self {
            staged: "/var/lib/homelab/staged-host".into(),
            current: "/usr/local/bin/homelab-host".into(),
            prev: "/usr/local/bin/homelab-host.prev".into(),
            marker: "/var/lib/homelab/selfupdate.pending".into(),
            service: "homelab-host".into(),
        }
    }
}

pub async fn self_update(ctx: &OpCtx<'_>, cfg: &SelfUpdateCfg) -> OperationReport {
    let mut runner = Runner::new("self-update", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;

    let mut new_version = String::new();

    // Gate: the candidate must prove it can run at all before it replaces
    // anything. A truncated upload or wrong-arch binary dies here.
    step!(runner, "selfcheck candidate", {
        let out = exec
            .run(&Cmd::new(&cfg.staged, &["--selfcheck"], 30))
            .await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "staged binary failed selfcheck (exit {}): {}",
                out.code,
                out.stderr.trim()
            )));
        }
        new_version = out.stdout.trim().to_string();
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "backup current", {
        run_ok(exec, &Cmd::new("cp", &["-a", &cfg.current, &cfg.prev], 30)).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "install candidate", {
        run_ok(
            exec,
            &Cmd::new("install", &["-m", "755", &cfg.staged, &cfg.current], 30),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    // The units that supervise the daemon ship with it (expert panel,
    // daemon-units-outside-repo): the watchdog and the rollback below only
    // exist if these files do. Written before the marker is armed, so the
    // restart already runs under them.
    step!(runner, "install host units", {
        let mut changed = false;
        let mut journald = false;
        for u in crate::hostunits::UNITS {
            let live = exec.read_file(u.path).await.ok();
            if live.as_deref() != Some(u.content) {
                if let Some(dir) = std::path::Path::new(u.path).parent() {
                    let dir = dir.to_string_lossy().to_string();
                    run_ok(exec, &Cmd::new("mkdir", &["-p", &dir], 30)).await?;
                }
                exec.write_file(u.path, u.content, u.mode).await?;
                changed = true;
                journald |= u.path == crate::hostunits::JOURNALD_CAP;
            }
        }
        if changed {
            run_ok(exec, &Cmd::new("systemctl", &["daemon-reload"], 30)).await?;
            if journald {
                run_ok(
                    exec,
                    &Cmd::new("systemctl", &["restart", "systemd-journald"], 60),
                )
                .await?;
            }
            Ok(StepOutcome::Changed)
        } else {
            Ok(StepOutcome::Unchanged)
        }
    });

    // rule-20 (disk-audit, 2026-10-01): pve gets the same apt-cache and
    // logrotate hygiene `guards::apply` already pushes to every managed
    // container — before this it never got `APT_AUTOCLEAN` at all (only the
    // CTs did, via `apply`/`apply_for_managed`), so its apt cache grew
    // (1.9G, measured) on every `apt upgrade` and nothing ever cleaned it.
    // Run from self-update, like the rest of `hostunits::UNITS`, so a
    // self-update is also how pve picks these up — no separate "apply pve
    // guards" path to forget to run.
    step!(runner, "pve apt + logrotate hygiene", {
        let mut changed = false;
        let apt_path = "/etc/apt/apt.conf.d/60homelab-clean";
        if exec.read_file(apt_path).await.ok().as_deref() != Some(crate::ops::guards::APT_AUTOCLEAN)
        {
            exec.write_file(apt_path, crate::ops::guards::APT_AUTOCLEAN, 0o644)
                .await?;
            changed = true;
        }
        // Same double-ingestion-logrotate guard CTs get: pve may carry its
        // own rsyslog package with its own `/etc/logrotate.d/rsyslog`
        // fragment, and logrotate refuses two fragments naming one path.
        let rsyslog_present = exec
            .run(&Cmd::new("test", &["-f", "/etc/logrotate.d/rsyslog"], 10))
            .await
            .map(|o| o.success())
            .unwrap_or(false);
        let logrotate_path = "/etc/logrotate.d/homelab";
        match crate::ops::guards::logrotate_policy(rsyslog_present) {
            Some(policy) => {
                if exec.read_file(logrotate_path).await.ok().as_deref() != Some(policy) {
                    exec.write_file(logrotate_path, policy, 0o644).await?;
                    changed = true;
                }
            }
            None => {
                if exec.read_file(logrotate_path).await.is_ok() {
                    run_ok(exec, &Cmd::new("rm", &["-f", logrotate_path], 10)).await?;
                    changed = true;
                }
            }
        }
        Ok(if changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    });

    // Armed BEFORE the restart: if the new binary never comes up, the
    // OnFailure unit sees this marker and restores `prev`.
    step!(runner, "arm rollback marker", {
        let content = format!(
            "{{\"to_version\":\"{}\",\"armed_at\":{}}}\n",
            new_version, ctx.now_unix
        );
        exec.write_file(&cfg.marker, &content, 0o644).await?;
        Ok(StepOutcome::Changed)
    });

    // Restart via a transient unit so the reply to the client still goes out
    // before this process is killed.
    step!(runner, "schedule restart", {
        let unit = format!("homelab-restart-{}", ctx.now_unix);
        run_ok(
            exec,
            &Cmd::new(
                "systemd-run",
                &[
                    "--unit",
                    &unit,
                    "--on-active=2",
                    "systemctl",
                    "restart",
                    &cfg.service,
                ],
                30,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Warn,
        format!(
            "[self-update] installing '{}' — restarting in 2s; rollback armed until the new binary reports healthy",
            new_version
        ),
    );
    runner.finish_ok()
}
