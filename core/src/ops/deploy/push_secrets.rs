//! D1: env over the secrets channel (A5), sealed into the host vault.

use crate::error::CoreError;
use crate::executor::{Cmd, run_ok};
use crate::ops::util::push_content;

use super::run::{Run, log_info};
use super::vault::vault_key;

impl Run<'_, '_> {
    /// The `.env` of every app the client sent, plus its vault copy for
    /// redeploys — outside the git repo.
    pub(super) async fn push_env_files(&self) -> Result<(), CoreError> {
        let (ctx, spec, m, exec) = self.parts();
        for (app, env) in &spec.env {
            let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
            push_content(exec, m.vmid, &dest, env, "600").await?;
            let vault = format!("{}/secrets/{}/{}.env", ctx.state_dir, m.stack_name, app);
            exec.write_file(&vault, env, 0o600).await?;
            log_info(ctx, format!("[vault] {} sealed (values not logged)", dest));
        }
        Ok(())
    }

    /// latch-files (2026-09-30): secret files from latch, at their
    /// absolute path, then sealed into the vault under the same key a
    /// native unit's env file uses, so the native step's vault copy and
    /// this one are one file. Values never logged.
    pub(super) async fn push_secret_files(&mut self) -> Result<(), CoreError> {
        let (ctx, spec, m, exec) = self.parts();
        let vm = m.vmid.to_string();
        for f in &spec.secret_files {
            let changed = push_content(exec, m.vmid, &f.path, &f.content, &f.mode).await?;
            if let Some(owner) = &f.owner {
                run_ok(
                    exec,
                    &Cmd::new("pct", &["exec", &vm, "--", "chown", owner, &f.path], 60),
                )
                .await?;
            }
            let vault = format!(
                "{}/secrets/{}/{}",
                ctx.state_dir,
                m.stack_name,
                vault_key(&f.path)
            );
            exec.write_file(&vault, &f.content, 0o600).await?;
            if changed {
                self.pushed.secrets_changed.push(f.path.clone());
                log_info(
                    ctx,
                    format!("[latch] {} written and sealed (values not logged)", f.path),
                );
            }
        }
        Ok(())
    }

    /// A5/E3: apps whose env the client did NOT send fall back to the
    /// vault — a wiped container gets its .env back on redeploy.
    pub(super) async fn restore_missing_env(&self) -> Result<(), CoreError> {
        let (ctx, spec, m, exec) = self.parts();
        for app in &m.apps {
            if spec.env.contains_key(app) {
                continue;
            }
            let vault = format!("{}/secrets/{}/{}.env", ctx.state_dir, m.stack_name, app);
            let dest = format!("/opt/{}/{}/.env", m.stack_name, app);
            if let Ok(env) = exec.read_file(&vault).await {
                push_content(exec, m.vmid, &dest, &env, "600").await?;
                log_info(ctx, format!("[vault] {} restored from vault", dest));
            } else {
                self.seal_container_env(&dest, &vault).await?;
            }
        }
        Ok(())
    }

    /// gap-27: an .env that exists only on the container (no
    /// client copy, no vault copy) is sealed into the vault,
    /// so a lost container gets it back. Read, never echoed.
    async fn seal_container_env(&self, dest: &str, vault: &str) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let got = crate::executor::pct_sh_secret(
            exec,
            m.vmid,
            &format!("cat {} 2>/dev/null || true", crate::ops::util::shq(dest)),
            60,
        )
        .await?;
        if !got.stdout.trim().is_empty() {
            exec.write_file(vault, &got.stdout, 0o600).await?;
            log_info(
                ctx,
                format!(
                    "[vault] {} sealed from the container (values not logged)",
                    dest
                ),
            );
        }
        Ok(())
    }
}
