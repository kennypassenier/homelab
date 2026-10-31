//! A5 (Kenny, 2026-09-02): prepare, then start — in that order.
//!
//! The G13 drill measured what the old order did: it ran
//! `systemctl enable --now kyu` on a container where /usr/local/bin was
//! empty, the user did not exist and the env file was not there, and
//! systemd tried thirteen times before giving up. That worked everywhere
//! else only because every native container had been built by hand and
//! adopted afterwards — which means a lost one could not be rebuilt.
//!
//! Kenny's N1 (2026-08-31): a native service gets the same cycle a docker
//! app gets. The unit file comes from the repository — it did not exist
//! there before, so if CT 109 had been lost, nobody would have had the
//! four files that make its services exist.
//!
//! Deliberately gentle: the unit is written and reloaded, and the service
//! is started only if it is not already running. Adoption's rule holds —
//! a running production service is not restarted to take ownership of it.
//! Changing a unit file that is already in place is a deliberate act, not
//! a side effect of a deploy.

use std::collections::{HashMap, HashSet};

use crate::error::CoreError;
use crate::executor::pct_sh;
use crate::manifest::FileBlob;
use crate::ops::util::push_content;
use crate::runner::StepOutcome;

use super::run::{Run, log_info, log_warn};
use super::vault::legacy_vault_key;

/// What the walk over the native units accumulates.
pub(super) struct NativeAcc {
    pub changed: bool,
    /// fix-159: what the push step wrote into the container this run, plus
    /// (latch-files) the secret files it changed; a running unit whose
    /// drop-in or env file is among it restarts.
    pub pushed_paths: Vec<String>,
    /// latch-files: the secret files whose content this deploy changed.
    pub secret_paths: Vec<String>,
    /// fix-37: file names two or more units read. A flat vault copy written
    /// by an older deploy under such a name belongs to nobody in particular
    /// and is never restored.
    pub seen: HashMap<String, HashSet<String>>,
}

impl NativeAcc {
    pub fn ambiguous(&self, f: &str) -> bool {
        self.seen
            .get(&legacy_vault_key(f))
            .map(|paths| paths.len() > 1)
            .unwrap_or(false)
    }
}

impl<'a, 'c> Run<'a, 'c> {
    pub(super) async fn native_units(&self) -> Result<StepOutcome, CoreError> {
        let (_, _, m, _) = self.parts();
        if m.natives.is_empty() {
            return Ok(StepOutcome::Unchanged);
        }
        let mut acc = self.native_acc();
        for unit in &m.natives {
            self.native_unit(unit, &mut acc).await?;
        }
        self.sweep_flat_vault_copies(&mut acc).await?;
        Ok(if acc.changed {
            StepOutcome::Changed
        } else {
            StepOutcome::Unchanged
        })
    }

    fn native_acc(&self) -> NativeAcc {
        let (_, _, m, _) = self.parts();
        let mut pushed_paths: Vec<String> = self
            .pushed
            .written
            .lock()
            .map(|g| g.iter().map(|(p, _)| p.clone()).collect())
            .unwrap_or_default();
        // latch-files: a changed secret file counts as written too, so a unit
        // reading it with EnvironmentFile= restarts (fix-159).
        let secret_paths = self.pushed.secrets_changed.clone();
        pushed_paths.extend(secret_paths.iter().cloned());
        let mut seen: HashMap<String, HashSet<String>> = HashMap::new();
        for unit in &m.natives {
            if let Some(blob) = self.unit_blob(unit) {
                let need = crate::native::unit_prereqs(&blob.content);
                for f in need.env_files.iter().chain(need.credentials.iter()) {
                    seen.entry(legacy_vault_key(f))
                        .or_default()
                        .insert(f.clone());
                }
            }
        }
        NativeAcc {
            changed: false,
            pushed_paths,
            secret_paths,
            seen,
        }
    }

    /// The unit file the stack directory carries for `unit`.
    pub(super) fn unit_blob(&self, unit: &str) -> Option<&'a FileBlob> {
        self.spec
            .files
            .iter()
            .find(|f| f.path == format!("{}/{}.service", unit, unit))
    }

    async fn native_unit(&self, unit: &str, acc: &mut NativeAcc) -> Result<(), CoreError> {
        let (ctx, _, m, _) = self.parts();
        let Some(blob) = self.unit_blob(unit) else {
            return Err(CoreError::SafetyAbort(format!(
                "stack declares native unit '{}' but {}/{}.service is not in the stack \
                 directory :: without it a rebuild cannot recreate the service, which is \
                 the whole reason the unit file was brought into the repository",
                unit, unit, unit
            )));
        };
        // fix-159: every file this unit reads that the deploy wrote.
        let mut written = acc.pushed_paths.clone();
        self.write_unit_file(unit, blob, &mut written, acc).await?;
        let need = crate::native::unit_prereqs(&blob.content);
        self.ensure_user(&need, acc).await?;
        self.install_binary(unit, &need, acc).await?;
        self.link_path_copy(unit, &need, acc).await?;
        let mut missing = self.restore_env_files(&need, &mut written, acc).await?;
        self.require_binary(&need, &mut missing).await?;
        // 5 · only now. Starting a unit whose prerequisites are absent
        // produces a restart loop and an error about "resources" that
        // says nothing about the actual cause.
        if !missing.is_empty() {
            log_warn(
                ctx,
                format!(
                    "[native] {} NOT started — these are missing on the container: {}. \
                     A binary comes from `homelab install-native stacks/{}/{}`; an env or \
                     credential file has never been on this host and cannot be invented.",
                    unit,
                    missing.join(", "),
                    m.stack_name,
                    unit
                ),
            );
            return Ok(());
        }
        self.start_or_refresh(unit, blob, &need, &written, acc)
            .await
    }

    async fn write_unit_file(
        &self,
        unit: &str,
        blob: &FileBlob,
        written: &mut Vec<String>,
        acc: &mut NativeAcc,
    ) -> Result<(), CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let dest = format!("/etc/systemd/system/{}.service", unit);
        if push_content(exec, m.vmid, &dest, &blob.content, "644").await? {
            log_info(ctx, format!("[native] {} written", dest));
            pct_sh(exec, m.vmid, "systemctl daemon-reload", 60).await?;
            written.push(dest);
            acc.changed = true;
        }
        Ok(())
    }
}
