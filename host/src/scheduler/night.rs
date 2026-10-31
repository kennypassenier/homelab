//! One night's plan: what is due, read once from the state at the start of
//! the tick, and the context every phase of the night shares.

use crate::*;

use super::gate::Gate;

/// The shared context of one night: the daemon, the state store, the moment,
/// and what the plan read from the state before the stacks are consumed.
pub(super) struct Night<'a> {
    pub(super) state: &'a AppState,
    pub(super) exec: &'a RealExecutor,
    pub(super) store: homelab_core::state::StateStore<'a>,
    pub(super) now: u64,
    pub(super) tiers: Vec<homelab_proto::RetentionTier>,
    pub(super) plan: Vec<NightlyTask>,
    /// G14: every repository a drill could rehearse, and whose turn it is.
    pub(super) drill_repos: Vec<String>,
    pub(super) drill_pick: Option<String>,
    /// fix-62: every stack's vmid, mounts and name, for the Postgres check.
    pub(super) pg_stacks: Vec<(u16, Vec<homelab_core::manifest::MountSpec>, String)>,
    /// fix-96: every repository with the retention its source keeps.
    pub(super) copy_policies: Vec<homelab_core::ops::secondcopy::RepoPolicy>,
}

impl<'a> Night<'a> {
    /// Plan the night from the state as it is now.
    pub(super) fn plan(
        state: &'a AppState,
        exec: &'a RealExecutor,
        gate: Gate,
        snapshot: &homelab_core::state::HostState,
    ) -> Self {
        let Gate {
            hour,
            local_hour,
            now,
            tiers,
        } = gate;
        let store = homelab_core::state::StateStore::new(exec, &state.config.state_dir);
        // H10: one plan per tick — stacks that are due, then the host's own
        // crown jewels. Decided by a pure function so it is unit-tested.
        let stack_inputs = nightly_stack_inputs(snapshot);
        // G14: taken before the loop below consumes `snapshot.stacks`.
        let drill_repos: Vec<String> = homelab_core::ops::restoredrill::all_drill_repos(
            &snapshot
                .stacks
                .iter()
                .map(|(name, st)| {
                    (
                        st.manifest
                            .as_ref()
                            .map(|m| m.storage.clone())
                            .unwrap_or_default(),
                        name.clone(),
                        // fix-115: only units that have a repository.
                        homelab_core::ops::restoredrill::backed_up_units(&st.natives),
                    )
                })
                .collect::<Vec<_>>(),
            &state
                .config
                .device_backups
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
        );
        // fix-62: whose turn it is, read before the loop below consumes the
        // snapshot's stacks.
        let drill_pick = homelab_core::ops::restoredrill::pick(snapshot, &drill_repos);
        // fix-62: every stack's vmid and mounts, for `postgres_check` to
        // find whether tonight's repository is a Postgres data directory —
        // also read before the loop below consumes `snapshot.stacks`.
        let pg_stacks: Vec<(u16, Vec<homelab_core::manifest::MountSpec>, String)> = snapshot
            .stacks
            .iter()
            .map(|(name, st)| {
                (
                    st.vmid,
                    st.manifest
                        .as_ref()
                        .map(|m| m.storage.clone())
                        .unwrap_or_default(),
                    name.clone(),
                )
            })
            .collect();
        // fix-96: every repository with the retention its source keeps, for
        // the second copy and the rotating restic check.
        let copy_policies = homelab_core::ops::secondcopy::repo_policies(
            snapshot,
            &state
                .config
                .device_backups
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>(),
            &tiers,
            state.config.backup.local_dataset.is_some(),
        );
        let plan = nightly_plan(
            hour,
            local_hour,
            now,
            &stack_inputs,
            &NightlyState {
                last_host_meta: snapshot.last_host_meta,
                last_zfs: snapshot.last_zfs,
                last_restore_drill: snapshot.last_restore_drill,
                restore_drill_interval_s: state.config.restore_drill_interval_s,
                zfs_configured: !state.config.zfs_jobs.is_empty(),
                devices_configured: !state.config.device_backups.is_empty(),
                second_copy_configured: state.config.second_copy_dataset.is_some(),
                last_second_copy: snapshot.last_second_copy,
                last_integrity_check: snapshot.last_integrity_check,
            },
        );
        Night {
            state,
            exec,
            store,
            now,
            tiers,
            plan,
            drill_repos,
            drill_pick,
            pg_stacks,
            copy_policies,
        }
    }
}
