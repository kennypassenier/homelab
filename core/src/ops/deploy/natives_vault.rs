//! The host vault copies of the files a native unit reads.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};
use crate::native::UnitPrereqs;

use super::natives::NativeAcc;
use super::run::{Run, log_warn};
use super::vault::vault_key;

impl Run<'_, '_> {
    /// 6 · keep the vault current, so the NEXT rebuild can restore
    /// what this container has. The same reasoning as the per-app
    /// .env copies: a file that exists in exactly one place is one
    /// disk failure from being gone.
    pub(super) async fn refresh_vault(&self, need: &UnitPrereqs) {
        let (ctx, _, m, exec) = self.parts();
        for f in need.env_files.iter().chain(need.credentials.iter()) {
            // fix-39: a secret, read without echoing it.
            let got = crate::executor::pct_sh_secret(
                exec,
                m.vmid,
                &format!("cat {} 2>/dev/null || true", crate::ops::util::shq(f)),
                60,
            )
            .await;
            if let Ok(out) = got
                && !out.stdout.trim().is_empty()
            {
                let vault = format!(
                    "{}/secrets/{}/{}",
                    ctx.state_dir,
                    m.stack_name,
                    vault_key(f)
                );
                let _ = exec.write_file(&vault, &out.stdout, 0o600).await;
            }
        }
    }

    /// Expert panel 2026-09-27 (stale-secret-residue-on-pve): flat vault
    /// copies from before fix-37 stayed on pve with live tokens nothing
    /// restores. A flat copy is kept only while it is the one a unit would
    /// still be restored from: a single unit reads that name and its
    /// per-unit copy does not exist yet. Anything else is removed, by name.
    pub(super) async fn sweep_flat_vault_copies(
        &self,
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let dir = format!("{}/secrets/{}", ctx.state_dir, m.stack_name);
        let listing = exec
            .run(&Cmd::new(
                "find",
                &[dir.as_str(), "-maxdepth", "1", "-type", "f"],
                30,
            ))
            .await;
        let Ok(out) = listing else {
            return Ok(());
        };
        for flat in out.stdout.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let name = flat.rsplit('/').next().unwrap_or(flat);
            if self.flat_copy_still_needed(&dir, name, acc).await {
                continue;
            }
            run_ok(exec, &Cmd::new("rm", &["-f", flat], 30)).await?;
            log_warn(
                ctx,
                format!(
                    "[native] removed stale flat vault copy {} — no unit restores from it",
                    flat
                ),
            );
            acc.changed = true;
        }
        Ok(())
    }

    async fn flat_copy_still_needed(&self, dir: &str, name: &str, acc: &NativeAcc) -> bool {
        match acc.seen.get(name) {
            Some(paths) if paths.len() == 1 => {
                let only = paths.iter().next().cloned().unwrap_or_default();
                let per_unit = format!("{}/{}", dir, vault_key(&only));
                !matches!(self.exec.read_file(&per_unit).await, Ok(c) if !c.trim().is_empty())
            }
            _ => false,
        }
    }
}
