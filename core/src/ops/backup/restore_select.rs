//! Which repositories and which snapshots a restore takes.

use crate::error::CoreError;
use crate::manifest::StackManifest;

use super::night_pick::{NightChoice, resolve_night};
use super::owners::owner_groups;
use super::restic::restic;
use super::restore_app::RestoreJob;
use super::snapshot_parse::{SnapRun, parse_snapshot_runs};

/// fix-112: the repositories one app's data lives in, or the whole stack's
/// when `app` is None. The same rule as the pre-update copy: a path names its
/// app, and a path without one belongs to the stack's only app.
pub(super) fn restore_groups(
    m: &StackManifest,
    app: Option<&str>,
) -> Result<Vec<(String, Vec<String>)>, CoreError> {
    let all = owner_groups(m);
    let Some(a) = app else {
        return Ok(all);
    };
    if !m.apps.iter().any(|x| x == a) {
        return Err(CoreError::Validation(format!(
            "stack '{}' has no app '{}' (it has: {}) — nothing has been stopped",
            m.stack_name,
            a,
            m.apps.join(", ")
        )));
    }
    let owners: Vec<String> = m
        .storage
        .iter()
        .filter(|s| !s.no_data && s.no_backup.is_none())
        .filter(|s| match s.app.as_deref() {
            Some(x) => x == a,
            None => m.apps.len() == 1,
        })
        .map(|s| s.owner(&m.stack_name).to_string())
        .collect();
    let groups: Vec<(String, Vec<String>)> = all
        .into_iter()
        .filter(|(o, _)| owners.contains(o))
        .collect();
    if groups.is_empty() {
        return Err(CoreError::Validation(format!(
            "app '{}' of stack '{}' keeps no backed-up data, so there is nothing to restore — \
             nothing has been stopped",
            a, m.stack_name
        )));
    }
    Ok(groups)
}

impl RestoreJob<'_, '_> {
    /// G5 of the Phase-7 gate: this step was called "validate snapshot" and
    /// never looked at the snapshot. It must ask every repository whether it
    /// holds what was asked for, or a typo'd id passes here, the stack is
    /// composed down, and restic then fails on something that does not exist.
    ///
    /// With several repositories this also picks one night across all of them
    /// (`Some`); with one, the caller's id is kept as typed (`None`).
    pub(super) async fn validate_snapshot(&self) -> Result<Option<NightChoice>, CoreError> {
        if self.groups.len() > 1 {
            let listings = self.list_all().await?;
            let choice = resolve_night(&listings, self.snapshot).map_err(CoreError::Other)?;
            return Ok(Some(choice));
        }
        for (owner, _) in &self.groups {
            self.check_in_repository(owner).await?;
        }
        Ok(None)
    }

    async fn list_all(&self) -> Result<Vec<(String, Vec<SnapRun>)>, CoreError> {
        let mut listings = Vec::new();
        for (owner, _) in &self.groups {
            let out = self.snapshots_of(owner, &["snapshots", "--json"]).await?;
            listings.push((owner.clone(), parse_snapshot_runs(&out)));
        }
        Ok(listings)
    }

    /// `restic <args>` on `owner`'s repository; its stdout, or "unreachable".
    async fn snapshots_of(&self, owner: &str, args: &[&str]) -> Result<String, CoreError> {
        let cfg = self.cfg;
        let cmd = restic(&cfg.restic_base, owner, &cfg.password_file, args, 120);
        let out = self.exec.run(&cmd).await?;
        if !out.success() {
            return Err(CoreError::Other(format!(
                "restic repo for '{}' unreachable",
                owner
            )));
        }
        Ok(out.stdout)
    }

    async fn check_in_repository(&self, owner: &str) -> Result<(), CoreError> {
        let listing = self.snapshots_of(owner, &["snapshots"]).await?;
        let snapshot = self.snapshot;
        // `latest` is restic's own word for "whatever the newest is" and
        // is always valid as long as the repository holds anything.
        if snapshot == "latest" {
            if listing.lines().filter(|l| !l.trim().is_empty()).count() < 2 {
                return Err(CoreError::Other(format!(
                    "repository for '{}' holds no snapshots at all, so \
                     'latest' means nothing",
                    owner
                )));
            }
            return Ok(());
        }
        // restic abbreviates ids in its listing, so a prefix match is the
        // honest comparison — and it is what the user typed anyway.
        if !listing
            .split_whitespace()
            .any(|w| w.starts_with(snapshot) || snapshot.starts_with(w) && w.len() >= 8)
        {
            return Err(CoreError::Other(format!(
                "snapshot '{}' is not in the repository for '{}' — nothing \
                 has been stopped",
                snapshot, owner
            )));
        }
        Ok(())
    }
}
