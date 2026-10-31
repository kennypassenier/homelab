//! Which app owns which restic repository (D25), and who keeps it.

use crate::manifest::StackManifest;

/// D25: group the manifest's storage paths by the app that owns them, in
/// manifest order. A path with no declared owner belongs to the stack, which
/// keeps host-level paths (and every manifest written before the field
/// existed) working exactly as they did.
/// Public because the disaster-recovery runbook must name the SAME
/// repositories the backup actually writes to. It used to derive them itself,
/// from the stack name, and so printed `media-config` for a stack whose
/// repositories are `jellyfin-config`, `sonarr-config`, `radarr-config` and
/// three more. That document is read exactly once — when everything else is
/// gone — and it would have said the backups were not there.
/// D25 names a restic repository after the OWNING APP, not the stack — so an
/// app that moves between stacks keeps its history. The other side of that
/// coin: two stacks that name the same owner share one repository, and
/// nothing said so.
///
/// Found by running the G13 drill (F285). A throwaway stack called `drill`
/// declared a native unit `http-switchboard`, which is also the name of a live
/// service on CT 109. Its destroy took the mandatory backup-before-destroy —
/// into `http-switchboard-config`, the live repository — and then applied that
/// throwaway stack's retention to it, which DELETED the real service's most
/// recent snapshot. The drill's own snapshot was then `latest`, so a restore
/// of the real service would have handed back the drill's fake configuration.
///
/// Returns the first conflict as `(owner, the other stack)`. Nothing is
/// reported for the stack's own name, nor for an owner no other stack claims.
pub fn conflicting_owner(
    stack: &str,
    owners: &[String],
    others: &[(String, Vec<String>)],
) -> Option<(String, String)> {
    for owner in owners {
        for (other_stack, other_owners) in others {
            if other_stack == stack {
                continue;
            }
            if other_owners.iter().any(|o| o == owner) {
                return Some((owner.clone(), other_stack.clone()));
            }
        }
    }
    None
}

/// T82 (F292): who keeps a repository two stacks both claim.
///
/// The F285 guard was symmetric: it saw two stacks naming one owner and
/// refused both, so a throwaway drill stack that borrowed the name
/// `jobtracker` switched off the REAL JobTracker's backup for six minutes.
/// Ownership has an order: the stack that was recorded first keeps the
/// repository, and only the newcomer is refused. A stack with no state
/// entry at all is by definition the newcomer. Equal timestamps favour the
/// caller — that is one deploy, not two stacks.
pub fn repository_keeper(this_applied_at: Option<u64>, other_applied_at: Option<u64>) -> bool {
    match (this_applied_at, other_applied_at) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(a), Some(b)) => a <= b,
    }
}

/// fix-218 round 2: every restic repository one recorded stack owns — the
/// single answer `homelab snapshots`, the dashboard's Backups page,
/// `homelab doctor` and the fleet check all read, so they can never disagree
/// about whether a stack has been backed up.
///
/// It mirrors what the nightly round writes (`night::backup_work`): a stack
/// with native services backs up each unit whole into the unit's own
/// repository, whatever manifest it also records; only a compose stack's
/// repositories come from its manifest's mounts. Since fix-145 a native
/// stack records BOTH a manifest and its natives; `GetBackups` took the
/// manifest whenever one existed, so inbox (manifest `storage: []`) showed
/// "no repositories" while its `inbox` repository took a snapshot every night.
pub fn stack_repo_owners(st: &crate::state::StackState) -> Vec<String> {
    if st.is_native() {
        return st.natives.iter().map(|n| n.unit.clone()).collect();
    }
    st.manifest
        .as_ref()
        .map(|m| {
            owner_groups(m)
                .into_iter()
                .map(|(owner, _)| owner)
                .collect()
        })
        .unwrap_or_default()
}

pub fn owner_groups(m: &StackManifest) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for mount in &m.storage {
        // An app that declares it keeps nothing gets no repository at all —
        // there is then nothing for the empty-snapshot guard to refuse, and
        // nothing that can stop the rest of the stack (F154, Kenny's B4).
        if mount.no_data {
            continue;
        }
        // Z3: declared reproducible, so no repository either. The difference
        // from `no_data` is what it means, not what it does here: that one
        // holds nothing, this one holds something nobody needs to keep. The
        // reason travels with the flag and is surfaced by the fleet check and
        // the runbook, because a directory that is silently unprotected and
        // one that is deliberately unprotected must not look the same.
        if mount.no_backup.is_some() {
            continue;
        }
        let owner = mount.owner(&m.stack_name).to_string();
        match groups.iter_mut().find(|(o, _)| *o == owner) {
            Some((_, paths)) => paths.push(mount.host_path.clone()),
            None => groups.push((owner, vec![mount.host_path.clone()])),
        }
    }
    groups
}
