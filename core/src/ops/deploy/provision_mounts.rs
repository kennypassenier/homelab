//! A container that already existed has its MOUNTS compared with the stack
//! file too, not just its boot policy.
//!
//! Proxmox will not change a drive while protection is on, so the flag comes
//! off for exactly as long as the writes take and goes straight back.
//!
//! The gap this closes was found the hard way: the downloader was
//! provisioned without its two data disks (F118), the mounts were put
//! back by hand, and a redeploy would happily have reported success
//! while leaving a hand-made fix as the only thing holding them there.
//! A repair that lives only outside the repo is not a repair.

use std::collections::BTreeSet;

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};

use super::run::{Run, log_info};

/// `(pct key, value)` pairs, e.g. `("-mp0", "/appdata/x,mp=/data")`.
type Mounts = Vec<(String, String)>;

impl Run<'_, '_> {
    pub(super) async fn reconcile_mounts(&self) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if !cfg.success() {
            return Ok(());
        }
        let want = self.wanted_mounts();
        let stale = self.stale_mounts(&cfg.stdout, &want);
        let missing: Mounts = want
            .into_iter()
            .filter(|(key, val)| {
                let line = format!("{}: {}", key.trim_start_matches('-'), val);
                !cfg.stdout.lines().any(|l| l.trim() == line)
            })
            .collect();
        if missing.is_empty() && stale.is_empty() {
            return Ok(());
        }
        let protected = cfg.stdout.lines().any(|l| l.trim() == "protection: 1");
        self.apply_mount_changes(protected, &stale, &missing).await
    }

    fn wanted_mounts(&self) -> Mounts {
        let (_, _, m, _) = self.parts();
        let mut want: Mounts = Vec::new();
        for (i, mo) in m.storage.iter().enumerate() {
            want.push((
                format!("-mp{}", i),
                format!("{},mp={}", mo.host_path, mo.mount_point),
            ));
        }
        for (i, dm) in m.data_mounts.iter().enumerate() {
            want.push((
                format!("-mp{}", m.storage.len() + i),
                format!("{},mp={}", dm.host_path, dm.mount_point),
            ));
        }
        want
    }

    /// ask-8 (`Loskoppelen`): a mount the stack no longer declares
    /// is detached. Which ones: an `mpN` the stack file does not
    /// ask for whose host directory this stack declared before
    /// (or declares now under another number — a duplicate left
    /// by a renumbering). A mount no version of the stack file
    /// ever named was attached by hand, and is not this deploy's
    /// to take away. The host directory is never touched.
    fn stale_mounts(&self, live_config: &str, want: &Mounts) -> Mounts {
        let ctx = self.ctx;
        let wanted_keys: BTreeSet<String> = want
            .iter()
            .map(|(k, _)| k.trim_start_matches('-').to_string())
            .collect();
        let ours = self.declared_host_paths();
        let mut stale: Mounts = Vec::new();
        for line in live_config.lines() {
            let Some((key, val)) = line.trim().split_once(':') else {
                continue;
            };
            let is_mp = key
                .strip_prefix("mp")
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()));
            if !is_mp || wanted_keys.contains(key) {
                continue;
            }
            let host_path = val.trim().split(',').next().unwrap_or("").to_string();
            if ours.contains(&host_path) {
                stale.push((key.to_string(), host_path));
            } else {
                log_info(
                    ctx,
                    format!(
                        "[mounts] {} ({}) is in no version of this stack file — attached by \
                         hand, left alone",
                        key, host_path
                    ),
                );
            }
        }
        stale
    }

    /// Host directories this stack declares now or declared before.
    fn declared_host_paths(&self) -> BTreeSet<String> {
        let m = &self.spec.manifest;
        let mut ours: BTreeSet<String> = m
            .storage
            .iter()
            .map(|s| s.host_path.clone())
            .chain(m.data_mounts.iter().map(|d| d.host_path.clone()))
            .collect();
        if let Some(pm) = self.prior_manifest.as_ref() {
            ours.extend(pm.storage.iter().map(|s| s.host_path.clone()));
            ours.extend(pm.data_mounts.iter().map(|d| d.host_path.clone()));
        }
        ours
    }

    async fn apply_mount_changes(
        &self,
        protected: bool,
        stale: &Mounts,
        missing: &Mounts,
    ) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        if protected {
            run_ok(
                exec,
                &Cmd::new("pct", &["set", &vm, "--protection", "0"], 60),
            )
            .await?;
        }
        // Detached first, so a renumbered mount never sits on two keys at once.
        for (key, host_path) in stale {
            run_ok(exec, &Cmd::new("pct", &["set", &vm, "--delete", key], 60)).await?;
            log_info(
                ctx,
                format!(
                    "[mounts] {} ({}) is no longer declared — detached; the directory \
                     on the host is kept",
                    key, host_path
                ),
            );
        }
        for (key, val) in missing {
            log_info(ctx, format!("[mounts] {} was not attached — {}", key, val));
            run_ok(exec, &Cmd::new("pct", &["set", &vm, key, val], 60)).await?;
        }
        if protected {
            self.set_protection().await?;
        }
        // A mount only appears inside a running container after a
        // restart, so saying so is part of doing it.
        log_info(
            ctx,
            "[mounts] changed — a running container sees it after a reboot".into(),
        );
        Ok(())
    }
}
