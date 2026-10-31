//! How recently each stack was really backed up, and which gaps are
//! deliberate.

use serde::{Deserialize, Serialize};

use super::{Finding, LiveFacts, Severity};
use crate::manifest::MountSpec;
use crate::state::StackState;

/// How stale a backup may be before it counts as a finding. A nightly run
/// that missed one night is noise; two is a pattern (standing rule 27 — a
/// number about patience belongs in configuration, and this is its default).
pub const DEFAULT_BACKUP_MAX_AGE_S: u64 = 48 * 3600;

/// fix-218: one stack's real backup freshness, as read from the host's
/// `SnapshotCache`. See `doctor::StackProbe` for the same shape and the
/// reasoning behind keeping "no snapshot yet" and "not read yet" apart.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackSnapshotFact {
    pub name: String,
    /// Unix seconds of the newest real snapshot across every repository this
    /// stack owns, or `None` when none of them holds one.
    pub newest_snapshot_unix: Option<u64>,
    /// Whether every owning repository's cache entry has been read at least
    /// once. `false` means "don't know yet".
    pub snapshot_read: bool,
    /// Mirrors `doctor::StackProbe::nothing_to_back_up`.
    pub nothing_to_back_up: bool,
    pub is_native: bool,
    /// inbox-dup-finding-1: the owning apps (`keep`, `registry`) whose
    /// repository was read and holds no snapshot. Names the app in the
    /// finding instead of only the stack. Empty from an older host.
    #[serde(default)]
    pub empty_owners: Vec<String>,
}

/// The finding for a stack with no snapshot at all. When the empty
/// repositories belong to apps other than the stack's own name, those apps
/// are named, so "registry" does not hide that it is `keep` that has none.
pub(super) fn no_snapshot_words(stack: &str, empty_owners: &[String]) -> String {
    let apps: Vec<&str> = empty_owners
        .iter()
        .map(String::as_str)
        .filter(|o| *o != stack)
        .collect();
    if apps.is_empty() {
        "has data to back up but no snapshot exists yet".into()
    } else {
        format!(
            "has data to back up but no snapshot exists yet (app {})",
            apps.join(", ")
        )
    }
}

/// Z3: a stack that keeps nothing worth keeping, by declaration, is not a
/// stack that was forgotten. Saying "never been backed up" about a decision
/// trains the reader to ignore the line — and that line is the one that has
/// to be believed when it IS real.
pub(super) fn unkept_notes(name: &str, st: &StackState) -> Vec<Finding> {
    let unkept: Vec<&MountSpec> = st
        .manifest
        .as_ref()
        .map(|m| m.storage.iter().filter(|s| s.no_backup.is_some()).collect())
        .unwrap_or_default();
    unkept
        .iter()
        .map(|mount| Finding {
            severity: Severity::Noted,
            subject: name.to_string(),
            what: format!(
                "{} is deliberately not backed up — {}",
                mount.host_path,
                mount.no_backup.as_deref().unwrap_or("")
            ),
            remedy: "nothing to do; listed so a deliberate gap never looks like a forgotten one"
                .into(),
        })
        .collect()
}

/// Every mount is either empty by design or declared unkept.
fn keeps_nothing(st: &StackState) -> bool {
    st.manifest
        .as_ref()
        .map(|m| {
            !m.storage.is_empty() && m.storage.iter().all(|s| s.no_data || s.no_backup.is_some())
        })
        .unwrap_or(false)
}

/// fix-218: the REAL backup age, off the same `SnapshotCache` `homelab
/// snapshots` reads — not `StackState::last_backup`, which a vacuous "no
/// services" nightly success can advance with no repository ever created
/// (see `StackSnapshotFact`). A caller that never gathered
/// `live.backup_snapshots` (e.g. the nightly round, which reads only what
/// it needs) falls back to the cached timestamp, same shape this check
/// always had.
pub(super) fn evaluate_backup_age(
    name: &str,
    st: &StackState,
    live: &LiveFacts,
    now_unix: u64,
    max_age_s: u64,
) -> Option<Finding> {
    match live.backup_snapshots.iter().find(|f| f.name == name) {
        // Deliberate, already said per mount (or nothing to declare at all
        // for a native stack).
        Some(f) if f.nothing_to_back_up => None,
        // fix-218: the cache has not read this stack's repository yet (cold
        // right after a host restart) — unknown is not a finding, so this
        // stays silent rather than guess either way.
        Some(f) if !f.snapshot_read => None,
        Some(f) => match f.newest_snapshot_unix {
            None => Some(no_snapshot(name, f)),
            Some(t) => stale(name, now_unix.saturating_sub(t), max_age_s),
        },
        None if keeps_nothing(st) => None,
        None if st.last_backup == 0 => Some(Finding {
            severity: Severity::Broken,
            subject: name.to_string(),
            what: "has never been backed up".into(),
            remedy: "run a backup now and check why the nightly one never did".into(),
        }),
        None => stale(name, now_unix.saturating_sub(st.last_backup), max_age_s),
    }
}

fn no_snapshot(name: &str, f: &StackSnapshotFact) -> Finding {
    Finding {
        severity: Severity::Broken,
        subject: name.to_string(),
        what: no_snapshot_words(name, &f.empty_owners),
        remedy: if f.is_native {
            format!(
                "run `homelab backup-native {}` and check why the nightly one never did",
                name
            )
        } else {
            format!(
                "run `homelab backup stacks/{}` and check why the nightly one never did",
                name
            )
        },
    }
}

/// A backup older than the patience the caller allows.
fn stale(name: &str, age: u64, max_age_s: u64) -> Option<Finding> {
    (age > max_age_s).then(|| Finding {
        severity: Severity::Broken,
        subject: name.to_string(),
        what: format!("last backup was {} hours ago", age / 3600),
        remedy: "check the nightly run and the backup target".into(),
    })
}
