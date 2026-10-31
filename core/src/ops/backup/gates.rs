//! The checks a stack's backup makes before it stops or writes anything.

use crate::error::CoreError;
use crate::executor::shq;
use crate::runner::{Scope, StepOutcome};
use crate::sink::Level;

use super::job::Job;
use super::owners::{conflicting_owner, owner_groups, repository_keeper};
use super::shell::sh;

impl Job<'_, '_> {
    /// A1/A2: same gate as every mutating op (quiesce/resume reach into the
    /// container).
    pub(super) async fn safety_gates(&self) -> Result<StepOutcome, CoreError> {
        crate::manifest::validate_manifest(self.m)?;
        crate::ops::guard_target(self.exec, &self.ctx.safety, self.m.vmid, &self.m.hostname)
            .await?;
        Ok(StepOutcome::Unchanged)
    }

    /// F285: does another stack already own one of these repositories?
    ///
    /// The repository is named after the owning APP (D25), so two stacks that
    /// name the same owner write into one repository — and the retention pass
    /// that follows a snapshot then applies THIS stack's tiers to the other
    /// stack's history. The G13 drill did exactly that and deleted a live
    /// service's most recent backup.
    ///
    /// Fail closed and before anything runs: a backup that quietly writes into
    /// somebody else's repository is worse than no backup, because the
    /// repository still looks healthy afterwards. `Some((owner, other))` is
    /// the newcomer case to warn about; the incumbent's own backup goes on.
    pub(super) async fn owner_conflict(&self) -> Result<Option<(String, String)>, CoreError> {
        let store = crate::state::StateStore::new(self.ctx.exec, &self.ctx.state_dir);
        let Ok(snapshot) = store.load().await else {
            // No state file is a first deploy, not a conflict.
            return Ok(None);
        };
        let others: Vec<(String, Vec<String>)> = snapshot
            .stacks
            .iter()
            .filter_map(|(name, st)| {
                let man = st.manifest.as_ref()?;
                Some((
                    name.clone(),
                    owner_groups(man).into_iter().map(|(o, _)| o).collect(),
                ))
            })
            .collect();
        let owners: Vec<String> = self.groups.iter().map(|(o, _)| o.clone()).collect();
        let Some((owner, other)) = conflicting_owner(&self.m.stack_name, &owners, &others) else {
            return Ok(None);
        };
        // T82: the incumbent keeps the repository; the newcomer is refused.
        let this_at = snapshot
            .stacks
            .get(&self.m.stack_name)
            .map(|s| s.applied_at);
        let other_at = snapshot.stacks.get(&other).map(|s| s.applied_at);
        if repository_keeper(this_at, other_at) {
            return Ok(Some((owner, other)));
        }
        Err(CoreError::SafetyAbort(format!(
            "stack '{}' would back up into the repository '{}-config', which stack '{}' \
             already owns :: repositories are named after the owning app, so both \
             stacks write into ONE history and the retention pass afterwards applies this \
             stack's tiers to the other stack's snapshots. The stack recorded first keeps \
             the repository; rename the app in this stack file",
            self.m.stack_name, owner, other
        )))
    }

    pub(super) fn warn_newcomer(&self, scope: &Scope<'_, '_>, owner: &str, other: &str) {
        scope.log(
            Level::Warn,
            format!(
                "[owner] stack '{}' also claims '{}' but was recorded later — it is the \
                 newcomer and ITS backup is refused, not this one",
                other, owner
            ),
        );
    }

    /// O10, second caller: ask before stopping anything.
    ///
    /// On 2026-09-04 at 04:17 the nightly round ran `docker stop bazarr
    /// prowlarr jellyfin seerr radarr sonarr` on CT 106 while Kenny was
    /// watching an episode. The backup path stops the same containers every
    /// single night and never asked (F280).
    ///
    /// Standing aside is not a failure and not a success. It returns
    /// `CoreError::Deferred`, which leaves `ok` false so no backup timestamp
    /// is recorded for work that did not happen, and carries `deferred` so the
    /// nightly round does not count it as a failed night and park the stack.
    /// Tomorrow it runs. If it keeps standing aside, the backup staleness
    /// check in `fleetcheck` is what says so.
    ///
    /// It is the whole stack that defers, not one app: the apps share a
    /// container and the snapshot is taken of the stack's paths in one pass.
    pub(super) async fn refuse_when_in_use(&self) -> Result<StepOutcome, CoreError> {
        for app in &self.m.apps {
            let Some(verdict) =
                // ctx.exec, not the tracing one: see `busy::app_busy`.
                crate::ops::busy::app_busy(
                    self.ctx.exec,
                    &self.ctx.state_dir,
                    self.m.vmid,
                    &self.m.stack_name,
                    app,
                )
                .await?
            else {
                continue;
            };
            if verdict.may_update() {
                continue;
            }
            return Err(CoreError::Deferred(format!(
                "{} is in use, so nothing was stopped and no snapshot was taken: {}",
                app,
                crate::ops::busy::reason(&verdict)
            )));
        }
        Ok(StepOutcome::Unchanged)
    }

    /// The other half of `no_data`: a declaration is only worth having if it
    /// is checked. An app that says it keeps nothing and then keeps something
    /// has quietly opted its data out of every backup, which is a worse
    /// failure than the one the flag was added to fix.
    pub(super) async fn check_no_data_empty(&self) -> Result<StepOutcome, CoreError> {
        let mut wrong = Vec::new();
        for mount in self.m.storage.iter().filter(|s| s.no_data) {
            let script = format!(
                "find {} -mindepth 1 -maxdepth 1 2>/dev/null | head -5 | wc -l",
                shq(&mount.host_path)
            );
            let out = self.exec.run(&sh(&script, 60)).await?;
            if out.stdout.trim() != "0" {
                wrong.push(mount.host_path.clone());
            }
        }
        if !wrong.is_empty() {
            return Err(CoreError::Validation(format!(
                "these paths are declared `no_data: true` and are not empty: {} — \
                 nothing in them is being backed up, by declaration. Either the \
                 declaration is stale or something started writing there",
                wrong.join(", ")
            )));
        }
        Ok(StepOutcome::Unchanged)
    }

    /// Z5 (Kenny, form Z5): a declared path that does not exist yet is not a
    /// broken backup, it is a stack that has not been deployed since its file
    /// changed. restic's own answer to it —
    /// `Fatal: all source directories/files do not exist` — names the wrong
    /// problem (F170).
    pub(super) async fn check_paths_exist(&self) -> Result<StepOutcome, CoreError> {
        let mut missing = Vec::new();
        for mount in self
            .m
            .storage
            .iter()
            .filter(|s| !s.no_data && s.no_backup.is_none())
        {
            let script = format!("test -d {} && echo yes || echo no", shq(&mount.host_path));
            let out = self.exec.run(&sh(&script, 30)).await?;
            if out.stdout.trim() != "yes" {
                missing.push(mount.host_path.clone());
            }
        }
        if !missing.is_empty() {
            return Err(CoreError::Validation(format!(
                "these paths are declared in the stack file and do not exist on the host: {} :: \
                 this is almost always a stack whose file gained a mount and was not deployed \
                 afterwards — `homelab deploy stacks/{}` creates them. It is NOT a broken \
                 backup, and restic's own message for it says the opposite",
                missing.join(", "),
                self.m.stack_name
            )));
        }
        Ok(StepOutcome::Unchanged)
    }
}
