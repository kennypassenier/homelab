//! Preparing a native unit: the account, the program, the files systemd
//! reads before the first line of the program runs.

use crate::error::CoreError;
use crate::executor::{pct_sh, shq};
use crate::native::UnitPrereqs;
use crate::ops::util::push_content;

use super::natives::NativeAcc;
use super::run::{Run, log_info};
use super::vault::{legacy_vault_key, vault_key};

impl Run<'_, '_> {
    /// 1 · the account systemd will run it as.
    pub(super) async fn ensure_user(
        &self,
        need: &UnitPrereqs,
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let Some(user) = &need.user else {
            return Ok(());
        };
        let has = pct_sh(exec, m.vmid, &format!("id -u {} 2>/dev/null", user), 30).await?;
        if has.stdout.trim().is_empty() {
            log_info(ctx, format!("[native] creating system user {}", user));
            pct_sh(
                exec,
                m.vmid,
                &format!(
                    "useradd --system --no-create-home --shell /usr/sbin/nologin {}",
                    shq(user)
                ),
                60,
            )
            .await?;
            acc.changed = true;
        }
        Ok(())
    }

    /// 2 · the program. Staged by the client from the service's own
    /// release; absent when the stack has no release_repo or GitHub
    /// could not be reached, and that is reported rather than fatal.
    pub(super) async fn install_binary(
        &self,
        unit: &str,
        need: &UnitPrereqs,
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        let (ctx, spec, m, exec) = self.parts();
        let Some(b64) = spec.native_binaries.get(unit) else {
            return Ok(());
        };
        let path = need
            .binary
            .clone()
            .unwrap_or_else(|| format!("/usr/local/bin/{}", unit));
        // fix-28: a deploy puts a program where there is none; it
        // never replaces one. The client stages the NEWEST release,
        // so replacing here upgraded every native on every deploy,
        // whatever its update_policy — an upgrade is the updater's
        // job, which reads the policy and checks the signature.
        let present = pct_sh(
            exec,
            m.vmid,
            &format!("test -f {} && echo yes || true", shq(&path)),
            30,
        )
        .await?;
        if present.stdout.trim() == "yes" {
            log_info(
                ctx,
                format!(
                    "[native] {} already installed, not shipped — upgrades go through the \
                     nightly updater or `homelab release-update-native`",
                    path
                ),
            );
            return Ok(());
        }
        if self.place_binary(&path, b64).await? {
            log_info(ctx, format!("[native] {} installed", path));
            acc.changed = true;
        }
        Ok(())
    }

    /// Staged beside the target, then moved: a transfer that dies
    /// half way must never leave a truncated program in place.
    async fn place_binary(&self, path: &str, b64: &str) -> Result<bool, CoreError> {
        let (_, _, m, exec) = self.parts();
        let b64_path = format!("{}.homelab-b64", path);
        if b64.is_empty() || !push_content(exec, m.vmid, &b64_path, b64, "600").await? {
            return Ok(false);
        }
        let script = format!(
            "base64 -d {b} > {p}.new && chmod 755 {p}.new && rm -f {b} \
             && test -s {p}.new && mv {p}.new {p}",
            b = shq(&b64_path),
            p = shq(path)
        );
        let out = pct_sh(exec, m.vmid, &script, 300).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "could not place {} on the container ({}) — nothing was replaced",
                path,
                out.stderr.trim()
            )));
        }
        Ok(true)
    }

    /// stale-path-binary (2026-09-30): a unit whose program lives
    /// elsewhere (/opt/kyu/bin/kyu) left an old copy at
    /// /usr/local/bin/<unit> from before the move; CT 109 ran kyu
    /// 4.1.0 while `kyu` on PATH answered 2.4.1. That copy becomes a
    /// link to the real program, so the name on PATH is always the
    /// version that runs. A link, not a removal: scripts may call it.
    pub(super) async fn link_path_copy(
        &self,
        unit: &str,
        need: &UnitPrereqs,
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let path_copy = format!("/usr/local/bin/{}", unit);
        let Some(real) = need.binary.as_deref().filter(|b| *b != path_copy) else {
            return Ok(());
        };
        let out = pct_sh(
            exec,
            m.vmid,
            &format!(
                "if [ -f {p} ] && [ ! -L {p} ] && [ -x {r} ]; then \
                 ln -sfn {r} {p} && echo linked; fi",
                p = shq(&path_copy),
                r = shq(real)
            ),
            30,
        )
        .await?;
        if out.stdout.trim() == "linked" {
            log_info(
                ctx,
                format!(
                    "[native] {} was a stale copy; now a link to {}",
                    path_copy, real
                ),
            );
            acc.changed = true;
        }
        Ok(())
    }

    /// 3 · the files systemd reads before the first line of the
    /// program runs. They cannot be invented: what the vault holds
    /// from an earlier deploy is restored, and anything else is named.
    pub(super) async fn restore_env_files(
        &self,
        need: &UnitPrereqs,
        written: &mut Vec<String>,
        acc: &mut NativeAcc,
    ) -> Result<Vec<String>, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let mut missing: Vec<String> = Vec::new();
        for f in need.env_files.iter().chain(need.credentials.iter()) {
            let there = pct_sh(
                exec,
                m.vmid,
                &format!("test -s {} && echo yes || true", shq(f)),
                30,
            )
            .await?;
            if there.stdout.trim() == "yes" {
                continue;
            }
            match self.vault_content(f, acc).await {
                Some(content) => {
                    push_content(exec, m.vmid, f, &content, "600").await?;
                    log_info(ctx, format!("[native] {} restored from the vault", f));
                    written.push(f.clone());
                    acc.changed = true;
                }
                None => missing.push(f.clone()),
            }
        }
        Ok(missing)
    }

    /// What the vault holds for `f`: the per-unit copy, else the flat copy
    /// an older deploy wrote while that name is unambiguous.
    async fn vault_content(&self, f: &str, acc: &NativeAcc) -> Option<String> {
        let (ctx, _, m, exec) = self.parts();
        let dir = format!("{}/secrets/{}", ctx.state_dir, m.stack_name);
        let from_vault = match exec.read_file(&format!("{}/{}", dir, vault_key(f))).await {
            Ok(c) if !c.trim().is_empty() => Ok(c),
            _ if !acc.ambiguous(f) => {
                exec.read_file(&format!("{}/{}", dir, legacy_vault_key(f)))
                    .await
            }
            other => other,
        };
        from_vault.ok().filter(|c| !c.trim().is_empty())
    }

    /// 4 · does the program exist at all?
    pub(super) async fn require_binary(
        &self,
        need: &UnitPrereqs,
        missing: &mut Vec<String>,
    ) -> Result<(), CoreError> {
        let (_, _, m, exec) = self.parts();
        let Some(bin) = &need.binary else {
            return Ok(());
        };
        let there = pct_sh(
            exec,
            m.vmid,
            &format!("test -x {} && echo yes || true", shq(bin)),
            30,
        )
        .await?;
        if there.stdout.trim() != "yes" {
            missing.push(bin.clone());
        }
        Ok(())
    }
}
