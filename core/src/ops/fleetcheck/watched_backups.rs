//! Backups this orchestrator does not make but does watch.

use super::{Finding, Severity};

/// O1 (Kenny, 2026-09-02): a backup this orchestrator does not MAKE but does
/// WATCH.
///
/// OPNsense is on the no-touch list and backs itself up: a plugin uploads an
/// encrypted copy of the router's configuration to Google Drive every night.
/// Nothing in this suite makes that backup and nothing should. But a backup
/// nobody watches is a backup you discover is broken on the day you need it,
/// so the nightly round that already asks "when was this last backed up" of
/// every stack asks it of that folder too.
///
/// Deliberately not a Kuma push monitor: that needs a script on the host
/// calling in on a timer, and a separately maintained file on a machine is
/// the thing Kenny ruled out on 2026-09-02. This reuses the round that
/// already runs, the finding shape that already exists, and the notification
/// path that already reaches him.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchedBackupFact {
    /// What to call it in the report.
    pub name: String,
    /// Age of the NEWEST file found, in seconds. None = nothing was found,
    /// which is not the same as "old" and is reported differently.
    pub newest_age_s: Option<u64>,
    /// Older than this and it is a finding.
    pub max_age_s: u64,
    /// The listing itself failed — no answer is not a healthy answer.
    pub error: Option<String>,
}

/// Broken rather than Drift throughout: unlike a missing metric, a backup
/// that stopped is not a slower kind of wrong. It is the thing itself.
pub fn evaluate_watched_backups(facts: &[WatchedBackupFact]) -> Vec<Finding> {
    facts.iter().filter_map(watched_finding).collect()
}

fn watched_finding(f: &WatchedBackupFact) -> Option<Finding> {
    if let Some(e) = &f.error {
        return Some(Finding {
            severity: Severity::Broken,
            subject: f.name.clone(),
            what: format!("could not be listed ({})", e),
            remedy: "check the rclone remote and the path in host.toml — a listing that \
                     does not answer says nothing about the backup either way"
                .into(),
        });
    }
    match f.newest_age_s {
        None => Some(Finding {
            severity: Severity::Broken,
            subject: f.name.clone(),
            what: "holds no files at all".into(),
            remedy: "the device that writes here has never succeeded, or writes somewhere \
                     else than host.toml says"
                .into(),
        }),
        Some(age) if age > f.max_age_s => Some(Finding {
            severity: Severity::Broken,
            subject: f.name.clone(),
            what: format!(
                "newest file is {} hours old, expected one within {}",
                age / 3600,
                f.max_age_s / 3600
            ),
            remedy: "the device stopped uploading — check its backup settings and its \
                     credentials before the next config change is the one you need back"
                .into(),
        }),
        Some(_) => None,
    }
}
