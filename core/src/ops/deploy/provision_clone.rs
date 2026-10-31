//! B8: provisioning by cloning the golden template container.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};

use super::run::Run;

impl Run<'_, '_> {
    pub(super) async fn provision_clone(&self, tpl_vmid: &str) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        self.check_template_privilege(tpl_vmid).await?;
        self.clone_template(tpl_vmid).await?;
        self.configure_clone().await?;
        // Grow the rootfs to the requested size (template base is 4G).
        if m.resources.disk_gb > 4 {
            let size = format!("{}G", m.resources.disk_gb);
            run_ok(
                exec,
                &Cmd::new("pct", &["resize", &vm, "rootfs", &size], 120),
            )
            .await?;
        }
        self.attach_mounts().await?;
        self.attach_devices().await?;
        // Same rule as the create path: protection only after every drive
        // change (see `provision_container`).
        if m.lxc.protection {
            self.set_protection().await?;
        }
        run_ok(exec, &Cmd::new("pct", &["start", &vm], 120)).await?;
        Ok(())
    }

    /// O5: `pct clone` has no --unprivileged; a clone always
    /// inherits the template's privilege level. Asking for one the
    /// template cannot give used to produce the other silently,
    /// and an app that then fails on permissions gives no hint why.
    /// CT 105 and 106 are privileged and must stay so.
    async fn check_template_privilege(&self, tpl_vmid: &str) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let tpl_cfg = exec
            .run(&Cmd::new("pct", &["config", tpl_vmid], 30))
            .await?;
        let tpl_unpriv = tpl_cfg
            .stdout
            .lines()
            .find_map(|l| l.strip_prefix("unprivileged:"))
            .map(|v| v.trim() == "1")
            .unwrap_or(false);
        if tpl_unpriv == m.lxc.unprivileged {
            return Ok(());
        }
        let word = |unpriv: bool| if unpriv { "unprivileged" } else { "privileged" };
        Err(CoreError::SafetyAbort(format!(
            "template {} is {}, but stack '{}' asks for {} — pct clone cannot change this, it always inherits the template",
            tpl_vmid,
            word(tpl_unpriv),
            m.stack_name,
            word(m.lxc.unprivileged),
        )))
    }

    async fn clone_template(&self, tpl_vmid: &str) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "clone",
                    tpl_vmid,
                    &vm,
                    "--hostname",
                    &m.hostname,
                    "--full",
                    "1",
                    "--storage",
                    &m.resources.storage,
                ],
                600,
            ),
        )
        .await?;
        Ok(())
    }

    /// Clones inherit the template's config — apply this stack's.
    async fn configure_clone(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        let mem = m.resources.memory_mb.to_string();
        let swap = m.resources.swap_mb.to_string();
        let cores = m.resources.cores.to_string();
        let desc = format!("managed by homelab v2 :: stack {}", m.stack_name);
        let mut set_args: Vec<String> = vec![
            "set".into(),
            vm.clone(),
            "--net0".into(),
            self.net0_config(),
            "--memory".into(),
            mem,
            "--swap".into(),
            swap,
            "--cores".into(),
            cores,
            "--onboot".into(),
            if m.boot.onboot { "1" } else { "0" }.into(),
            "--description".into(),
            desc,
            "--tags".into(),
            "homelab".into(),
            // T62: declared (default `host`), same as the `pct
            // create` path.
            "--timezone".into(),
            m.lxc.timezone.clone(),
        ];
        if let Some(order) = m.boot.order {
            set_args.push("--startup".into());
            set_args.push(format!("order={}", order));
        }
        let refs: Vec<&str> = set_args.iter().map(|s| s.as_str()).collect();
        run_ok(exec, &Cmd::new("pct", &refs, 60)).await?;
        Ok(())
    }
}
