//! The state record of a finished deploy.

use crate::error::CoreError;
use crate::manifest;
use crate::native::NativeServiceManifest;
use crate::runner::StepOutcome;
use crate::state::{HostState, StackState, StateStore};

use super::run::{Run, log_info};

impl Run<'_, '_> {
    pub(super) async fn record_state(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, _, m, exec) = self.parts();
        let store = StateStore::new(exec, &ctx.state_dir);
        let mut state = store.load().await?;
        // Preserve last_backup and the H8 enabled flag across redeploys —
        // parking is an explicit operator choice; refresh everything else.
        let previous = state
            .stacks
            .get(&m.stack_name)
            .map(|s| (s.last_backup, s.enabled));
        let kept_natives = self.kept_natives(&state);
        self.retire_departed(&mut state);
        let (mut last_backup, enabled) = previous.unwrap_or((0, true));
        // A C4 replacement destroys the record along with the container, so
        // there is nothing left to preserve and the rebuilt stack claims it
        // has never been backed up — which the fleet check then reports as
        // broken while the snapshots sit untouched in the repository. The
        // same happens to every stack at once on a rebuilt host. Ask the
        // repository instead: it is the thing that actually knows.
        if previous.is_none()
            && let Some(t) = crate::ops::backup::newest_snapshot_unix(exec, m, &ctx.backup).await
        {
            last_backup = t;
            log_info(
                ctx,
                format!(
                    "[state] no record for '{}' — last backup recovered from the repository",
                    m.stack_name
                ),
            );
        }
        // fix-195 round 2: while the previous record is still here, move
        // its old-rule check answers onto the digests they were given
        // against, so this deploy reopens only what it really changed.
        crate::ops::manualchecks::pin_pre_digest_answers(&mut state, &m.stack_name);
        let record = self.new_record(last_backup, enabled, kept_natives);
        state.stacks.insert(m.stack_name.clone(), record);
        store.save(state).await?;
        Ok(StepOutcome::Changed)
    }

    /// C7: the native services registered on this stack are NOT the
    /// deploy's to forget. Writing an empty list here unregistered kyu,
    /// kyu-runner, http-switchboard and almanac the moment their
    /// containers got a manifest — so the nightly backup of four services
    /// would simply have stopped, quietly, with nothing to see.
    ///
    /// ask-8: except a unit the stack FILE dropped from `natives:` — the
    /// step "retire dropped" stopped it, and it leaves the record here.
    fn kept_natives(&self, state: &HostState) -> Vec<NativeServiceManifest> {
        state
            .stacks
            .get(&self.spec.manifest.stack_name)
            .map(|s| {
                s.natives
                    .iter()
                    .filter(|n| !self.dropped_natives.contains(&n.unit))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// ask-8 / ask-9: an app or unit that left the files leaves its
    /// backups, /appdata and vault copies behind, KEPT until `homelab
    /// wipe`; recorded here with the date it left. A stack or app that is
    /// back is no longer retired.
    fn retire_departed(&self, state: &mut HostState) {
        let (ctx, _, m, _) = self.parts();
        crate::ops::retired::unretire(state, m);
        if let Some(pm) = self.prior_manifest.as_ref() {
            let before = state
                .stacks
                .get(&m.stack_name)
                .map(|s| s.apps.clone())
                .unwrap_or_default();
            // fix-186: an app this deploy still declares is obviously not
            // dropped (`m.apps.contains`) — but neither is one that moved to
            // `natives:` under the same name (a docker-to-native conversion,
            // exactly what the 3.70.0 admin/almanac deploy did). Only a name
            // declared NOWHERE in the new manifest actually left the stack.
            for app in before
                .iter()
                .filter(|a| !m.apps.contains(a) && !m.natives.contains(a))
            {
                crate::ops::retired::retire_app(state, pm, app, &ctx.state_dir, ctx.now_unix);
            }
        }
        for unit in &self.dropped_natives {
            crate::ops::retired::retire_unit(
                state,
                &m.stack_name,
                m.vmid,
                unit,
                self.unit_vault.get(unit).cloned().unwrap_or_default(),
                ctx.now_unix,
            );
        }
    }

    fn new_record(
        &self,
        last_backup: u64,
        enabled: bool,
        natives: Vec<NativeServiceManifest>,
    ) -> StackState {
        let (ctx, spec, m, _) = self.parts();
        StackState {
            // fix-141: which commit this is, for `homelab status`.
            applied_source: spec.source.as_ref().map(|s| s.summary()),
            vmid: m.vmid,
            hostname: m.hostname.clone(),
            apps: m.apps.clone(),
            applied_at: ctx.now_unix,
            last_backup,
            applied_hash: manifest::intent_hash(spec),
            manifest: Some(m.clone()),
            enabled,
            natives,
            incomplete_step: None,
            // fix-41: the route this deploy wrote, the only one a later
            // deploy may retire.
            route_file: spec.gateway_route.as_ref().map(|r| r.filename.clone()),
            // fix-91: the extra route files it wrote, for the same reason.
            extra_route_files: spec
                .extra_routes
                .iter()
                .map(|r| r.filename.clone())
                .collect(),
            // fix-142: ground truth for the nightly in-container hash
            // comparison (`evaluate_container_drift`).
            pushed_file_hashes: self.pushed.file_hashes.clone(),
            // fix-192: recorded from the same `spec` `intent_hash` just
            // hashed above, so the two can never disagree about what
            // this deploy's intent actually was.
            component_digests: manifest::component_digests(spec),
        }
    }
}
