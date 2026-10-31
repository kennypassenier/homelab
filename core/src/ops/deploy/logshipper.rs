//! C1/C2 (Kenny, 2026-09-02): one log shipper per container, and it is
//! Alloy, because promtail reached end of life on 2026-03-02.
//!
//! Runs for EVERY stack, native or compose. The old arrangement gave a
//! promtail sidecar to stacks that run containers and nothing at all to
//! the two that do not — so kyu, the hub every notification travels
//! through, shipped no logs for as long as it has existed (F245).

use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::ops::logshipper as ls;
use crate::ops::util::push_content;
use crate::runner::StepOutcome;

use super::run::{Run, log_info, log_warn};

impl Run<'_, '_> {
    pub(super) async fn log_shipper(&self, loki: &str) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let out = pct_sh(exec, m.vmid, &ls::install_script(), 900).await?;
        if !out.success() {
            // Not fatal: a stack that cannot install a log shipper is
            // still a stack that should come up. Silence about it would
            // be the fault this whole migration is about, so it is loud.
            log_warn(
                ctx,
                format!(
                    "[logs] Alloy could not be installed on {} ({}) — this container \
                     is shipping NO logs until that is fixed",
                    m.hostname,
                    out.stderr.trim()
                ),
            );
            return Ok(StepOutcome::Unchanged);
        }
        let fresh = out.stdout.contains("installed") && !out.stdout.contains("already");
        let changed = self.push_alloy_config(loki).await?;
        let drift = self.sweep_single_file_mode().await?;
        let access = self.grant_read_access().await?;
        let moved = changed || drift || access;
        if (fresh || moved) && !self.restart_alloy().await? {
            return Ok(StepOutcome::Unchanged);
        }
        // Asked on every deploy: "Alloy is running" said nothing for 24
        // days while it could not open a single container log.
        self.report_readability().await?;
        Ok(if fresh || moved {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    async fn push_alloy_config(&self, loki: &str) -> Result<bool, CoreError> {
        let (_, _, m, exec) = self.parts();
        let body = ls::config(
            &m.stack_name,
            &m.hostname,
            loki,
            &m.syslog_receivers,
            &m.log_files,
        );
        push_content(exec, m.vmid, ls::CONFIG_PATH, &body, "644").await
    }

    /// gap-11: Alloy reads ONE file, the one just pushed. A directory
    /// mode left behind by hand, or a stray `.alloy` beside the
    /// rendered one, is undone here and said out loud — every line the
    /// script prints is a change the operator did not make through
    /// the repository. Before the restart, because with the receiver
    /// rendered above a stray copy of it would make Alloy refuse to
    /// start on a duplicate component.
    async fn sweep_single_file_mode(&self) -> Result<bool, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let swept = pct_sh(exec, m.vmid, &ls::single_file_mode_script(), 60).await?;
        let mut drift = false;
        for line in swept
            .stdout
            .lines()
            .filter(|l| !l.trim().is_empty() && *l != "done")
        {
            drift = true;
            log_warn(
                ctx,
                format!(
                    "[logs] Alloy on {} was not reading the rendered file alone: {}",
                    m.hostname, line
                ),
            );
        }
        Ok(drift)
    }

    /// Every deploy, not only when the config moved: the read access
    /// Alloy needs for docker's 0710 directory was missing on every
    /// container for 24 days while their configs never changed
    /// (expert panel, container-logs-missing-in-loki).
    async fn grant_read_access(&self) -> Result<bool, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let perms = pct_sh(exec, m.vmid, &ls::permissions_script(), 60).await?;
        let access = perms.stdout.contains(ls::DROPIN_WRITTEN);
        if access {
            log_info(
                ctx,
                format!(
                    "[logs] gave Alloy read access to the container logs on {}",
                    m.hostname
                ),
            );
        }
        Ok(access)
    }

    /// Enables and restarts Alloy, then asks what it delivered. `false`
    /// when it did not start (said loudly).
    async fn restart_alloy(&self) -> Result<bool, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let r = pct_sh(
            exec,
            m.vmid,
            "systemctl enable alloy >/dev/null 2>&1; systemctl restart alloy",
            120,
        )
        .await?;
        if !r.success() {
            log_warn(
                ctx,
                format!(
                    "[logs] Alloy did not start on {} ({})",
                    m.hostname,
                    r.stderr.trim()
                ),
            );
            return Ok(false);
        }
        // "It started" is not "it is shipping". The first live run
        // of this step printed exactly that line while every batch
        // was dropped with a 404 and nothing reached Loki, so the
        // step now asks Alloy what it managed to deliver.
        exec.sleep_ms(12_000).await;
        let metrics = pct_sh(
            exec,
            m.vmid,
            &format!("curl -s -m 5 {} 2>/dev/null || true", ls::METRICS_URL),
            30,
        )
        .await?;
        self.report_delivery(&metrics.stdout);
        Ok(true)
    }

    fn report_delivery(&self, metrics: &str) {
        let (ctx, _, m, _) = self.parts();
        match ls::delivery(metrics) {
            ls::Delivery::Shipping { sent } => log_info(
                ctx,
                format!(
                    "[logs] Alloy delivered {} bytes to Loki for {}",
                    sent, m.stack_name
                ),
            ),
            ls::Delivery::Dropping { dropped } => log_warn(
                ctx,
                format!(
                    "[logs] Alloy is DROPPING {}'s logs ({} bytes) — Loki refused them; \
                     check loki_url in host.toml and `journalctl -u alloy` on {}",
                    m.stack_name, dropped, m.hostname
                ),
            ),
            ls::Delivery::Quiet => log_info(
                ctx,
                format!(
                    "[logs] Alloy started on {} and has shipped nothing yet — normal on a \
                     quiet container, wrong on a busy one; check Loki for stack=\"{}\"",
                    m.hostname, m.stack_name
                ),
            ),
            ls::Delivery::Unknown(why) => log_warn(
                ctx,
                format!(
                    "[logs] Alloy on {} could not be asked whether it is shipping ({})",
                    m.hostname, why
                ),
            ),
        }
    }

    async fn report_readability(&self) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let probe = pct_sh(exec, m.vmid, &ls::readability_script(), 30).await?;
        match ls::readability(&probe.stdout) {
            ls::Readability::Denied => log_warn(
                ctx,
                format!(
                    "[logs] Alloy on {} cannot read /var/lib/docker/containers — NO container \
                     log from {} reaches Loki; check {} and `systemctl status alloy`",
                    m.hostname,
                    m.stack_name,
                    ls::READ_DROPIN_PATH
                ),
            ),
            ls::Readability::Unknown(why) => log_warn(
                ctx,
                format!(
                    "[logs] could not ask whether Alloy on {} can read the container logs ({})",
                    m.hostname, why
                ),
            ),
            _ => {}
        }
        Ok(())
    }
}
