//! ask-8 (`Stoppen, data bewaren`): a native unit dropped from `natives:` is
//! stopped, disabled, its unit file and program removed. Its data dirs inside
//! the container and its restic repository are kept.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};

use super::run::{Run, log_info};
use super::vault::vault_key;

impl Run<'_, '_> {
    /// Stops and removes one native unit that left the stack file; returns
    /// the vault copies it leaves behind, for its retired record (ask-9).
    pub(super) async fn retire_native(&self, unit: &str) -> Result<Vec<String>, CoreError> {
        let ctx = self.ctx;
        let need = self.old_unit_prereqs(unit).await;
        // The program's path: the registration knows it; the unit file
        // the last deploy placed says it too; the convention otherwise.
        let binary = self
            .registered_binary(unit)
            .or_else(|| need.as_ref().and_then(|n| n.binary.clone()))
            .unwrap_or_else(|| format!("/usr/local/bin/{}", unit));
        let vault = self.retired_vault_files(unit, need.as_ref());
        self.disable_now(unit).await?;
        log_info(
            ctx,
            format!(
                "[native] {} left the stack file — stopped and disabled",
                unit
            ),
        );
        self.remove_native_files(unit, binary).await?;
        log_info(
            ctx,
            format!(
                "[native] {}'s data directories and its restic repository are kept",
                unit
            ),
        );
        Ok(vault)
    }

    /// What the unit file the last deploy placed said it needs.
    async fn old_unit_prereqs(&self, unit: &str) -> Option<crate::native::UnitPrereqs> {
        let old_unit = self
            .exec
            .read_file(&format!(
                "{}/{}/{}.service",
                self.repo_stack_dir(),
                unit,
                unit
            ))
            .await
            .ok();
        old_unit.as_deref().map(crate::native::unit_prereqs)
    }

    async fn remove_native_files(&self, unit: &str, binary: String) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        for path in [format!("/etc/systemd/system/{}.service", unit), binary] {
            run_ok(
                exec,
                &Cmd::new("pct", &["exec", &vm, "--", "rm", "-f", &path], 60),
            )
            .await?;
            log_info(ctx, format!("[native] removed {}", path));
        }
        Ok(())
    }

    /// The program path the last record registered for `unit`, if any.
    fn registered_binary(&self, unit: &str) -> Option<String> {
        self.prior
            .as_ref()
            .and_then(|p| p.natives.iter().find(|n| n.unit == unit))
            .map(|n| n.binary.clone())
            .filter(|b| !b.is_empty())
    }

    /// Vault copies of the env and credential files a retired unit read.
    fn retired_vault_files(
        &self,
        unit: &str,
        need: Option<&crate::native::UnitPrereqs>,
    ) -> Vec<String> {
        let (ctx, _, m, _) = self.parts();
        let in_vault = |f: &str| {
            format!(
                "{}/secrets/{}/{}",
                ctx.state_dir,
                m.stack_name,
                vault_key(f)
            )
        };
        let mut vault: Vec<String> = need
            .map(|n| {
                n.env_files
                    .iter()
                    .chain(n.credentials.iter())
                    .map(|f| in_vault(f))
                    .collect()
            })
            .unwrap_or_default();
        if let Some(env) = self
            .prior
            .as_ref()
            .and_then(|p| p.natives.iter().find(|n| n.unit == unit))
            .and_then(|n| n.env_file.clone())
        {
            vault.push(in_vault(&env));
        }
        vault
    }
}
