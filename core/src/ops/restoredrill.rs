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
//! rather than the first. And it rotates rather than always taking the
//! biggest repository, so every repository gets its turn.
//!
//! fix-62 (restore-drill-covers-almost-nothing, 2026-09-27): "over a year"
//! was never true. One repository per 90 days over about thirty repositories
//! is one rotation every 7.4 years, and one drill had run in total. A failure
//! was cleared by the next repository's pass, host-meta and the device
//! repositories were never drilled, and a torn tar passed the size rule. Now
//! one repository is drilled every night (a rotation in about a month), each
//! repository keeps its own record, the host's own repositories are in the
//! rotation, and a native unit's tar has to list.

use crate::ops::fleetcheck::{Finding, Severity};
use crate::state::HostState;

/// How long a passed drill counts for: one night. Twenty hours rather than
/// twenty-four for the same reason as the backup's own check — the nightly
/// tick lands a few minutes earlier some nights, and a 24-hour rule would
/// then skip every other night.
///
/// Configurable per standing rule 27 — every caller passes it, and the host
/// reads it from `host.toml`.
pub const DEFAULT_DRILL_INTERVAL_S: u64 = 20 * 3600;

/// The repository that holds the host's own vault, state and TLS material.
pub const HOST_META_REPO: &str = "host-meta";

/// fix-62: where a repository is restored to for the drill. A data pool, not
/// the root disk — same reasoning, and the same neighbourhood, as
/// `backup::DEFAULT_STAGING_DIR`. The drill used to restore under the state
/// dir on pve-root (47 GB free when this was found; the largest repository,
/// jellyfin-config, is 8.8 GB).
pub const DEFAULT_DRILL_SCRATCH_DIR: &str = "/appdata/.restore-scratch";

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

/// fix-115 (drill-includes-stateless-native, 2026-09-27): the native units
/// that have a repository. A unit that keeps nothing (`stateless: true`, no
/// `data_dirs`) is never backed up, so a drill night spent on it failed on a
/// repository that does not exist — the drill stack's `drillsvc` whenever
/// that stack was present.
pub fn backed_up_units(natives: &[crate::native::NativeServiceManifest]) -> Vec<String> {
    natives
        .iter()
        .filter(|n| !n.stateless && !n.data_dirs.is_empty())
        .map(|n| n.unit.clone())
        .collect()
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

/// fix-62: the whole rotation — the stacks' repositories plus the host's own
/// (`host-meta`: the vault holding the restic password, state.json, TLS) and
/// each device configuration (`<name>-config`, route A). Those two kinds were
/// never drilled, and host-meta is the one a full-host rebuild starts from.
pub fn all_drill_repos(
    stacks: &[(Vec<crate::manifest::MountSpec>, String, Vec<String>)],
    devices: &[String],
) -> Vec<String> {
    let mut out = drill_repos(stacks);
    out.push(HOST_META_REPO.to_string());
    out.extend(devices.iter().cloned());
    out.sort();
    out.dedup();
    out
}

/// fix-62: whose turn it is tonight — the repository drilled longest ago,
/// never-drilled ones first, ties by name. A cursor into a sorted list used
/// to decide this, and a repository that failed then waited a whole
/// rotation; ordering by the last attempt keeps the rotation and needs no
/// cursor that a changed list can shift.
pub fn pick(state: &HostState, repos: &[String]) -> Option<String> {
    let mut sorted = repos.to_vec();
    sorted.sort();
    sorted.dedup();
    sorted.into_iter().min_by_key(|r| {
        state
            .restore_drills
            .get(r)
            .map(|d| d.last_attempt)
            .unwrap_or(0)
    })
}

/// fix-62: fold one finished drill into the state. The failure is kept on
/// the repository it belongs to until that repository passes; another
/// repository's pass no longer clears it. Records of repositories that left
/// the rotation are dropped, so a retired stack's last error does not stand
/// forever.
pub fn record(state: &mut HostState, repos: &[String], repo: &str, outcome: &Outcome, now: u64) {
    state
        .restore_drills
        .retain(|name, _| repos.iter().any(|r| r == name));
    let rec = state.restore_drills.entry(repo.to_string()).or_default();
    rec.last_attempt = now;
    match outcome {
        Outcome::Passed { .. } => {
            rec.last_pass = now;
            rec.last_error = None;
            state.last_restore_drill = now;
        }
        Outcome::Failed(why) => rec.last_error = Some(why.clone()),
    }
    state.last_restore_drill_repo = repo.to_string();
    // The per-repository record carries the error now; the single field is
    // left for state written before it and cleared by the first new drill.
    state.last_restore_drill_error = None;
}

/// fix-62: a restore that brought back a tar which `tar -tf` cannot read
/// proves nothing about that unit's data, however large the file is.
pub fn with_archives(outcome: Outcome, unreadable: &[String]) -> Outcome {
    match outcome {
        Outcome::Passed { .. } if !unreadable.is_empty() => Outcome::Failed(format!(
            "{} archive(s) came back that tar cannot read: {} — a torn archive has content \
             and proves nothing",
            unreadable.len(),
            unreadable.join(", ")
        )),
        other => other,
    }
}

/// fix-62: where and how to run a throwaway Postgres restore check for the
/// repository the drill just restored — the container to run it in and the
/// image to run, both read off the stack file's own declaration
/// (`MountSpec::postgres_check_image`). `None` when the repository's owner
/// is not a Postgres data directory (the common case).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostgresCheck {
    pub vmid: u16,
    /// The absolute host path of the PGDATA directory, the same one the
    /// drill already restored under its scratch target — so the check reads
    /// what the drill itself proved came back, not a second restore.
    pub host_path: String,
    pub image: String,
}

/// fix-62: does the repository just drilled need a throwaway Postgres
/// check, and with what? Scans every stack's mounts for one whose owner is
/// this repository and that declares `postgres_check_image` — generic by
/// content (what the mount says about itself), not by app name.
pub fn postgres_check(
    stacks: &[(u16, Vec<crate::manifest::MountSpec>, String)],
    repo: &str,
) -> Option<PostgresCheck> {
    for (vmid, mounts, stack_name) in stacks {
        for m in mounts {
            if m.owner(stack_name) == repo
                && let Some(image) = &m.postgres_check_image
            {
                return Some(PostgresCheck {
                    vmid: *vmid,
                    host_path: m.host_path.clone(),
                    image: image.clone(),
                });
            }
        }
    }
    None
}

/// fix-62: a throwaway Postgres container that never reaches "ready to
/// accept connections" against the restored data proves the backup copied
/// files Postgres itself cannot still open — the same shape as
/// [`with_sqlite_checks`], for the one engine a magic-byte sniff cannot
/// identify (a Postgres data directory is a folder of many files, not one
/// file with a header).
pub fn with_postgres_check(outcome: Outcome, ready: Option<bool>) -> Outcome {
    match (outcome, ready) {
        (Outcome::Passed { .. }, Some(false)) => Outcome::Failed(
            "a throwaway Postgres container never reached \"ready to accept connections\" \
             against the restored data"
                .into(),
        ),
        (other, _) => other,
    }
}

/// fix-62: a restored SQLite database that fails its own `PRAGMA
/// integrity_check` proves the backup copied a corrupt file, not a usable
/// one — found generically, by content (every file's own magic bytes), not
/// by name, so it covers every app's database without naming one.
/// `(path, what integrity_check said)`.
pub fn with_sqlite_checks(outcome: Outcome, bad: &[(String, String)]) -> Outcome {
    match outcome {
        Outcome::Passed { .. } if !bad.is_empty() => Outcome::Failed(format!(
            "{} SQLite database(s) failed their own integrity check: {} — a file that restic \
             restored without error is not the same thing as a database SQLite can still read",
            bad.len(),
            bad.iter()
                .map(|(p, why)| format!("{} ({})", p, why))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        other => other,
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
    }
    // fix-62: one finding per repository whose own last drill failed, until
    // that repository passes.
    for (repo, rec) in &state.restore_drills {
        let Some(err) = &rec.last_error else {
            continue;
        };
        out.push(Finding {
            severity: Severity::Broken,
            subject: format!("restore drill · {}", repo),
            what: format!(
                "the last drill of this repository did not prove a restore: {}{}",
                err,
                if rec.last_pass == 0 {
                    " (it has never passed)".to_string()
                } else {
                    format!(" (last passed {})", crate::state::ymd(rec.last_pass))
                }
            ),
            remedy: "the backup for this repository is a hypothesis until one comes back \
                     with content in it — check the repository and the password file; the \
                     nightly round tries it again in its turn"
                .into(),
        });
    }
    if !out.is_empty() {
        return out;
    }
    // A night's grace on top of the interval: a drill that ran at 04:05
    // yesterday is not overdue at 10:00 today.
    if due(
        state.last_restore_drill,
        now,
        interval_s.saturating_add(24 * 3600),
    ) {
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
