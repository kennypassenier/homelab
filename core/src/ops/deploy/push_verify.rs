//! After the files are in: systemd learns about them, and S2 checks them.

use std::sync::{Arc, Mutex};

use crate::error::CoreError;
use crate::executor::{Executor, pct_sh, shq};

use super::run::{Run, log_info};

/// S2: every file this deploy actually wrote into the container, with the
/// hash it should now have. Checked once at the end of the step — a push
/// that reports success and lands somewhere else, or on top of something
/// else, is the shape F124 took.
pub(super) struct PushCheck<'a> {
    pub exec: &'a dyn Executor,
    pub vmid: u16,
    pub written: Arc<Mutex<Vec<(String, String)>>>,
}

impl PushCheck<'_> {
    /// Ask the container what those files now hash to. One command for all
    /// of them, because a round trip per file is what makes a check like
    /// this get switched off later. `.env` files are deliberately absent
    /// from the list: their content is not ours to echo back, and the
    /// vault copy is the record that matters for them.
    pub async fn run(&self) -> Result<(), String> {
        let want = self.written.lock().map(|g| g.clone()).unwrap_or_default();
        if want.is_empty() {
            return Ok(());
        }
        let paths = want
            .iter()
            .map(|(p, _)| shq(p))
            .collect::<Vec<_>>()
            .join(" ");
        let out = pct_sh(
            self.exec,
            self.vmid,
            &format!("sha256sum {} 2>&1", paths),
            120,
        )
        .await
        .map(|o| o.stdout)
        .unwrap_or_default();
        let mut bad: Vec<String> = Vec::new();
        for (path, hash) in &want {
            let line = out.lines().find(|l| l.trim_end().ends_with(path.as_str()));
            match line {
                Some(l) if l.split_whitespace().next() == Some(hash.as_str()) => {}
                Some(_) => bad.push(format!("{} has different content", path)),
                None => bad.push(format!("{} is not there", path)),
            }
        }
        if bad.is_empty() {
            Ok(())
        } else {
            Err(bad.join("; "))
        }
    }
}

impl<'a, 'c> Run<'a, 'c> {
    pub(super) fn push_check(&self) -> PushCheck<'a> {
        PushCheck {
            exec: self.exec,
            vmid: self.spec.manifest.vmid,
            written: self.pushed.written.clone(),
        }
    }

    /// ask-2: systemd only reads a unit it has been told about, and a
    /// timer file on disk fires nothing until it is enabled. A changed
    /// unit or timer reloads the manager; a changed timer is enabled
    /// and started — starting a TIMER is not starting a service.
    /// Nothing here touches a `.service` beyond the reload; a native
    /// unit whose drop-in or env file changed here is restarted by
    /// the "native units" step (fix-159), health-checked.
    pub(super) async fn activate_rootfs(&self, rootfs_changed: &[String]) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        if rootfs_changed
            .iter()
            .any(|d| d.starts_with("/etc/systemd/system/"))
        {
            pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
            log_info(
                ctx,
                "[rootfs] systemd reloaded for the unit files written above".to_string(),
            );
        }
        for dest in rootfs_changed {
            let Some(timer) = dest
                .strip_prefix("/etc/systemd/system/")
                .filter(|n| n.ends_with(".timer"))
            else {
                continue;
            };
            let out = pct_sh(
                exec,
                m.vmid,
                &format!("systemctl enable --now {}", timer),
                60,
            )
            .await?;
            if !out.success() {
                return Err(CoreError::Command {
                    rendered: format!("systemctl enable --now {}", timer),
                    detail: out.stderr.trim().to_string(),
                });
            }
            log_info(ctx, format!("[rootfs] {} enabled and started", timer));
        }
        Ok(())
    }
}
