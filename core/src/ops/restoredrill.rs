//! G14 · the recurring restore drill.
//!
//! B3 asked for a quarterly trial restore and it was never built; the Phase-7
//! gate offered "write it down as a known limitation" and Kenny said no —
//! close it. Rightly: a backup nobody has restored is a hypothesis, and the
//! four drills this project HAS run were each one-off, done by hand, on a day
//! somebody happened to think of it.
//!
//! The design follows from what the drills themselves taught. On 2026-09-02 a
//! restore was declared identical to live by comparing two md5 sums that both
//! belonged to a zero-byte file (F217, F219) — so the verdict here refuses to
//! be satisfied by empty files, and looks at the LARGEST file that came back
//! rather than the first. And it is round-robin rather than "always the
//! biggest repository", so over a year every repository gets its turn instead
//! of one being proven twelve times.

use crate::ops::fleetcheck::{Finding, Severity};
use crate::state::HostState;

/// How long a passed drill counts for. B3 says quarterly; 90 days is that.
///
/// Configurable per standing rule 27 — every caller passes it, and the host
/// reads it from `host.toml`.
pub const DEFAULT_DRILL_INTERVAL_S: u64 = 90 * 24 * 3600;

/// Is a drill due? A drill that has never run is always due — that is the
/// state this project was in for its whole life.
pub fn due(last: u64, now: u64, interval_s: u64) -> bool {
    last == 0 || now.saturating_sub(last) >= interval_s
}

/// Whose turn it is. Round-robin over the repositories, sorted so the order
/// is the same on every host and does not depend on a map's iteration.
///
/// Returns None when there is nothing to drill, which is not an error: a host
/// with no backups configured has no restore to rehearse.
/// G14: which repositories the nightly drill should rotate over.
///
/// It used to be each stack's `apps` list, and that is the wrong list in two
/// directions at once — found on 2026-09-04 while answering whether a newly
/// deployed service was covered:
///
/// * **It misses every native service.** A native stack has an EMPTY `apps`
///   list by design; its services live in `natives`. So `kyu`, `kyu-runner`,
///   `http-switchboard` and `almanac` were never rehearsed — the four whose
///   backups had silently been broken until two days earlier (F179).
/// * **It includes apps that have no repository at all.** `flaresolverr`,
///   `recyclarr` and `registry` keep nothing under `/appdata`, so their
///   `-config` repository does not exist and a drill night spent on one of
///   them proves nothing while looking like a failure.
///
/// The right list is the one the BACKUP uses: the owning apps of the mounts
/// it actually snapshots, plus the native units, which back themselves up
/// under their own names.
pub fn drill_repos(
    stacks: &[(Vec<crate::manifest::MountSpec>, String, Vec<String>)],
) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (mounts, stack, natives) in stacks {
        for m in mounts {
            if m.no_data || m.no_backup.is_some() {
                continue;
            }
            let owner = m.owner(stack).to_string();
            if !out.contains(&owner) {
                out.push(owner);
            }
        }
        for n in natives {
            if !out.contains(n) {
                out.push(n.clone());
            }
        }
    }
    out.sort();
    out
}

pub fn next_repo(repos: &[String], index: usize) -> Option<(String, usize)> {
    if repos.is_empty() {
        return None;
    }
    let mut sorted = repos.to_vec();
    sorted.sort();
    sorted.dedup();
    let i = index % sorted.len();
    Some((sorted[i].clone(), (i + 1) % sorted.len()))
}

/// What a finished drill amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Files came back and at least one of them has content.
    Passed { files: usize, largest_bytes: u64 },
    /// It ran and what came back proves nothing.
    Failed(String),
}

/// Judge a restore by what came back, not by whether the command exited 0.
///
/// The empty-file rule is the whole point. A restic restore of a directory
/// full of zero-byte placeholders exits 0, reports files restored, and
/// demonstrates nothing about whether the data is recoverable — and this
/// project has already published one "GELIJK" verdict on exactly that.
pub fn verdict(files: usize, largest_bytes: u64) -> Outcome {
    if files == 0 {
        return Outcome::Failed("the restore returned no files at all".into());
    }
    if largest_bytes == 0 {
        return Outcome::Failed(format!(
            "{} file(s) came back and every one of them is empty — a restore of \
             zero-byte files proves nothing about whether the data is recoverable",
            files
        ));
    }
    Outcome::Passed {
        files,
        largest_bytes,
    }
}

/// What the state says about the drill: overdue, or failed and left that way.
pub fn evaluate_drill(state: &HostState, now: u64, interval_s: u64) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(err) = &state.last_restore_drill_error {
        out.push(Finding {
            severity: Severity::Broken,
            subject: format!(
                "restore drill{}",
                if state.last_restore_drill_repo.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", state.last_restore_drill_repo)
                }
            ),
            what: format!("the last drill did not prove a restore: {}", err),
            remedy: "the backup for this repository is a hypothesis until one comes back \
                     with content in it — check the repository and the password file"
                .into(),
        });
        return out;
    }
    if due(state.last_restore_drill, now, interval_s) {
        let days = if state.last_restore_drill == 0 {
            "never".to_string()
        } else {
            format!(
                "{} days ago",
                now.saturating_sub(state.last_restore_drill) / 86400
            )
        };
        out.push(Finding {
            severity: Severity::Drift,
            subject: "restore drill".into(),
            what: format!("no restore has been rehearsed since: {}", days),
            remedy: "the nightly round takes one automatically; if this stands, the round \
                     is not reaching it — a backup nobody has restored is a hypothesis"
                .into(),
        });
    }
    out
}
