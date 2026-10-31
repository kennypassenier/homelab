//! C1: a full `pct create`, plus the mounts and devices every new
//! container needs.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};

use super::run::Run;

impl Run<'_, '_> {
    pub(super) async fn provision_create(&self) -> Result<(), CoreError> {
        let (_, _, _, exec) = self.parts();
        let args = self.create_args();
        let refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        run_ok(exec, &Cmd::new("pct", &refs, 300)).await?;
        self.attach_mounts().await?;
        self.attach_devices().await?;
        Ok(())
    }

    fn create_args(&self) -> Vec<String> {
        let (_, _, m, _) = self.parts();
        let rootfs = format!("{}:{}", m.resources.storage, m.resources.disk_gb);
        let mut args: Vec<String> = vec![
            "create".into(),
            m.vmid.to_string(),
            m.lxc.template.clone(),
            "--hostname".into(),
            m.hostname.clone(),
            "--rootfs".into(),
            rootfs,
            "--net0".into(),
            self.net0_config(),
            "--memory".into(),
            m.resources.memory_mb.to_string(),
            "--swap".into(),
            m.resources.swap_mb.to_string(),
            "--cores".into(),
            m.resources.cores.to_string(),
            "--unprivileged".into(),
            if m.lxc.unprivileged { "1" } else { "0" }.into(),
            "--features".into(),
            m.lxc.features.clone(),
            "--onboot".into(),
            if m.boot.onboot { "1" } else { "0" }.into(),
            // Managed containers are recognizable in the Proxmox UI.
            "--description".into(),
            format!("managed by homelab v2 :: stack {}", m.stack_name),
            "--tags".into(),
            "homelab".into(),
            // T62: declared (default `host`), not hardcoded — a stack
            // that needs a different zone can say so.
            "--timezone".into(),
            m.lxc.timezone.clone(),
        ];
        if let Some(order) = m.boot.order {
            args.push("--startup".into());
            args.push(format!("order={}", order));
        }
        args
    }
}
