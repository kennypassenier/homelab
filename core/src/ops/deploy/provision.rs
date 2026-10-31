//! C1: create or reuse the container; C3 boot policy at create.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};
use crate::runner::StepOutcome;

use super::run::{Run, log_info};

impl Run<'_, '_> {
    pub(super) async fn provision_container(&mut self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        if !self.exists {
            // B8: template "clone:<vmid>" provisions by cloning the golden
            // template container instead of a full create — seconds, not
            // minutes, because docker + guards are already baked in. The
            // bootstrap steps below still run and simply skip everything.
            if let Some(tpl_vmid) = m.lxc.template.strip_prefix("clone:") {
                self.provision_clone(tpl_vmid).await?;
                self.created = true;
                return Ok(StepOutcome::Changed);
            }
            self.provision_create().await?;
            self.created = true;
        }
        if !self.created {
            self.reconcile_mounts().await?;
            self.reconcile_boot_policy().await?;
            self.restore_protection().await?;
        }
        // Protection is deliberately the LAST provisioning act: Proxmox
        // refuses drive changes ("can't update CT ... drive 'mp0' -
        // protection mode enabled") once the flag is set, so it must land
        // after the rootfs resize and every mountpoint. Live-found on the
        // first protected stack with a bind mount (metrics, 2026-08-29).
        // Gated destroy (C2) lifts it deliberately before removal.
        if self.created && m.lxc.protection {
            self.set_protection().await?;
        }
        let status = run_ok(exec, &Cmd::new("pct", &["status", &vm], 30)).await?;
        if !status.stdout.contains("running") {
            run_ok(exec, &Cmd::new("pct", &["start", &vm], 120)).await?;
            return Ok(StepOutcome::Changed);
        }
        Ok(if self.created {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    /// The NIC definition a new container is created with.
    pub(super) fn net0_config(&self) -> String {
        let (_, _, m, _) = self.parts();
        let fw_enabled = m.firewall.as_ref().is_some_and(|f| f.enabled);
        let mut net = format!(
            "name=eth0,bridge={},firewall={},ip={},gw={}",
            m.network.bridge,
            if fw_enabled { 1 } else { 0 },
            m.network.ip,
            m.network.gateway
        );
        if let Some(tag) = m.network.vlan {
            net.push_str(&format!(",tag={}", tag));
        }
        net
    }

    pub(super) async fn set_protection(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        run_ok(
            exec,
            &Cmd::new("pct", &["set", &vm, "--protection", "1"], 60),
        )
        .await?;
        Ok(())
    }

    /// W3: a container that already existed has its boot policy compared
    /// with the stack file and put back. Set at creation and never looked
    /// at again means that after a power cut the fleet boots in the order
    /// somebody typed years ago — and the rule that everything behind the
    /// edge waits for Traefik lives only in a file nothing reads.
    /// Resources are deliberately NOT touched here: raising them is
    /// `homelab resize`, lowering them is a rebuild, and neither belongs
    /// in an ordinary deploy. The fleet check reports those instead.
    async fn reconcile_boot_policy(&self) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if !cfg.success() {
            return Ok(());
        }
        let live = crate::ops::reconcile::parse(&cfg.stdout);
        let args = crate::ops::reconcile::boot_set_args(m, &live);
        if args.is_empty() {
            return Ok(());
        }
        log_info(
            ctx,
            format!(
                "[w3] boot policy drifted — {}",
                crate::ops::reconcile::divergences(m, &live).join("; ")
            ),
        );
        let mut argv: Vec<&str> = vec!["set", &vm];
        argv.extend(args.iter().map(|a| a.as_str()));
        run_ok(exec, &Cmd::new("pct", &argv, 60)).await?;
        Ok(())
    }

    /// The protection flag is intent like any other. It was set only on
    /// the run that CREATED the container, so a container whose flag had
    /// been turned off — by hand, or by an operation that lifted it and
    /// did not put it back — stayed unprotected and nothing said so. Found
    /// immediately after the mount reconciliation: that step lifts the flag
    /// to do its work, and the deploy that followed left it off.
    async fn restore_protection(&self) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        if !m.lxc.protection {
            return Ok(());
        }
        let vm = m.vmid.to_string();
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if cfg.success() && !cfg.stdout.lines().any(|l| l.trim() == "protection: 1") {
            log_info(
                ctx,
                "[protection] was off — the stack file asks for it".into(),
            );
            self.set_protection().await?;
        }
        Ok(())
    }
}
