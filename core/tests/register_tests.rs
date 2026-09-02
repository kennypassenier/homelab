//! The register claims, the tests prove — and until now nothing held the two
//! together.
//!
//! On 2026-09-01 Kenny asked whether the day's faults had a structural,
//! proven fix. Answering meant working out, by hand, which test covered which
//! finding — and the answer was that fifteen register rows claimed "test
//! added" while exactly one test anywhere named the finding it covered. The
//! claims were true; they were simply unverifiable, in both directions. You
//! could not ask a test what it protects, and you could not ask the register
//! to prove itself.
//!
//! So there is a convention and this file enforces it:
//!
//! * a test that exists because of a finding carries `/// covers: F123` (or
//!   several, comma-separated) in its doc comment;
//! * a register row that claims a test names it as ``Test: `fn_name` `` in
//!   the measure column.
//!
//! Two rules, both mechanical, both failing loudly:
//!
//! 1. **No dangling marker.** A `covers:` naming a finding the register does
//!    not have is a rename or a typo, and it silently breaks the link it was
//!    written to create.
//! 2. **No unbacked claim.** A row naming a test must name one that exists
//!    and that carries the row's own id. This is the direction that was
//!    actually broken: the claim in the document, with nothing behind it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core/ has a parent")
        .to_path_buf()
}

/// Every `.rs` file in the workspace, skipping build output.
fn rust_sources(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                // target-debian is the cross-build directory; .git is large
                // and holds no sources.
                if name.starts_with("target") || name == ".git" || name == "node_modules" {
                    continue;
                }
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

/// finding id -> the files whose sources carry a marker for it.
fn markers(sources: &[(PathBuf, String)]) -> BTreeMap<String, Vec<PathBuf>> {
    let mut out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    for (path, text) in sources {
        for line in text.lines() {
            for id in marker_ids(line) {
                out.entry(id).or_default().push(path.clone());
            }
        }
    }
    out
}

/// The ids on one `/// covers: F1, F2` line; empty for any other line.
fn marker_ids(line: &str) -> Vec<String> {
    line.trim()
        .strip_prefix("/// covers:")
        .map(|rest| {
            rest.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Does the marker sit on THIS function, rather than merely somewhere in the
/// same file?
///
/// The first version of this check asked the file, and a deliberate sabotage
/// walked straight through it: `F75` is claimed by one test and marked on
/// two, so deleting the marker from the claimed one still found the other
/// and passed. A check that answers a question next to the one being asked
/// is the pattern this whole register is full of.
fn function_carries(text: &str, func: &str, id: &str) -> bool {
    let lines: Vec<&str> = text.lines().collect();
    let needle = format!("fn {}(", func);
    let Some(idx) = lines.iter().position(|l| l.contains(&needle)) else {
        return false;
    };
    // Walk up through the attribute-and-doc block that belongs to it, and
    // stop at the first line that is neither.
    let mut i = idx;
    while i > 0 {
        let prev = lines[i - 1].trim();
        if prev.starts_with("#[") || prev.starts_with("///") || prev.starts_with("//") {
            if marker_ids(prev).iter().any(|m| m == id) {
                return true;
            }
            i -= 1;
        } else {
            break;
        }
    }
    false
}

/// finding id -> measure column, for every row of the register's finding
/// table. The table is `| F123 | what | measure | status |`.
fn register_rows(root: &Path) -> BTreeMap<String, String> {
    let raw = std::fs::read_to_string(root.join("docs/deployment/REGISTER.md"))
        .expect("the register is part of the repo");
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
        if cells.len() < 4 {
            continue;
        }
        let id = cells[0].trim();
        let is_finding =
            id.starts_with('F') && id.len() > 1 && id[1..].chars().all(|c| c.is_ascii_digit());
        if is_finding {
            out.insert(id.to_string(), cells[2].trim().to_string());
        }
    }
    assert!(
        out.len() > 100,
        "the register parser found only {} findings — the table shape changed \
         and this check would silently pass on nothing",
        out.len()
    );
    out
}

/// The test name a row claims, if it claims one: ``Test: `name` ``.
fn claimed_test(measure: &str) -> Option<String> {
    let after = measure.split("Test: `").nth(1)?;
    let name = after.split('`').next()?;
    (!name.is_empty()).then(|| name.to_string())
}

fn sources() -> (PathBuf, Vec<(PathBuf, String)>) {
    let root = repo_root();
    let files = rust_sources(&root)
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
        .collect();
    (root, files)
}

/// Rule 1: a marker that names a finding the register does not have.
#[test]
fn every_covers_marker_names_a_real_finding() {
    let (root, files) = sources();
    let rows = register_rows(&root);
    let mut dangling = Vec::new();
    for (id, where_) in markers(&files) {
        if !rows.contains_key(&id) {
            dangling.push(format!("{} (in {:?})", id, where_));
        }
    }
    assert!(
        dangling.is_empty(),
        "these `covers:` markers name findings the register does not have — \
         a broken link reads exactly like a working one: {:?}",
        dangling
    );
}

/// Rule 2: a row that claims a test must name one that exists and that says
/// so itself.
#[test]
fn every_claimed_test_exists_and_names_its_finding() {
    let (root, files) = sources();
    let rows = register_rows(&root);
    let mut broken = Vec::new();

    for (id, measure) in &rows {
        let Some(name) = claimed_test(measure) else {
            continue;
        };
        let needle = format!("fn {}(", name);
        let holder = files.iter().find(|(_, t)| t.contains(&needle));
        let Some((path, _)) = holder else {
            broken.push(format!("{}: names `{}`, which does not exist", id, name));
            continue;
        };
        let text = &files.iter().find(|(p, _)| p == path).unwrap().1;
        if !function_carries(text, &name, id) {
            broken.push(format!(
                "{}: `{}` exists in {:?} but does not itself carry \
                 `/// covers: {}` — the link only goes one way",
                id, name, path, id
            ));
        }
    }

    assert!(
        broken.is_empty(),
        "the register claims tests that do not back it up: {:#?}",
        broken
    );
}

/// And the claim has to be worth making: a row that names a test must be one
/// that was actually closed. A `Test:` on an open finding is a promise, and a
/// promise in the measure column is what this whole file exists to stop.
#[test]
fn a_claimed_test_belongs_to_a_closed_finding() {
    let root = repo_root();
    let raw = std::fs::read_to_string(root.join("docs/deployment/REGISTER.md")).unwrap();
    let mut open_claims = Vec::new();
    for line in raw.lines() {
        let cells: Vec<&str> = line.trim().trim_matches('|').split('|').collect();
        if cells.len() < 4 {
            continue;
        }
        let id = cells[0].trim();
        if !(id.starts_with('F') && id.len() > 1 && id[1..].chars().all(|c| c.is_ascii_digit())) {
            continue;
        }
        let status = cells[3].trim();
        if claimed_test(cells[2]).is_some() && status.starts_with("open") {
            open_claims.push(id.to_string());
        }
    }
    assert!(
        open_claims.is_empty(),
        "these findings are open and already claim a test: {:?}",
        open_claims
    );
}

/// A6 · no new finding may claim "done" without saying what proves it.
///
/// Kenny chose *bewaker eerst, dan opruimen* at the A6 gate: stop the debt
/// growing, then work off what is there. The Phase-7 audit reported 133 rows
/// marked fixed with no test claim; measuring it here with a stated
/// definition gives a different number, and the definition is the reason —
/// this counts rows whose STATUS column says `done` and whose text names no
/// test, no file and no deliberate "nothing".
///
/// The design avoids the thing it is guarding against. A list of grandfathered
/// IDs would itself be a hand-maintained file that drifts — exactly what Kenny
/// ruled out on 2026-09-02. There was a watermark (every finding from F226 on
/// must comply) and a ratchet on the older rows; the backlog was worked to
/// zero the same day, so both halves now say the same thing and neither
/// carries a number anybody has to maintain.
mod proof_ratchet {
    /// The first finding recorded after the rule existed.
    const WATERMARK: u32 = 226;

    fn register() -> String {
        std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("docs/deployment/REGISTER.md"),
        )
        .expect("the register must be where this test says it is")
    }

    /// A row names its proof when it points at something a reader can go and
    /// check for themselves.
    ///
    /// Deliberately broader than "names a test". More than half of this
    /// register is Phase-1 inventory — observations of what the fleet was
    /// doing on a given day — and for those the proof IS the measurement: the
    /// command that was run, the file that was read. Demanding a unit test
    /// for "CT 107 runs no docker" would be demanding the wrong thing, and a
    /// rule that asks for the wrong thing gets satisfied with noise.
    ///
    /// So: a backticked citation of any kind, a snake_case test name, or an
    /// explicit written-down "there is deliberately no test, because…".
    /// Judging whether a citation is a GOOD one stays a human's job; this
    /// only refuses a row pointing at nothing whatsoever, which is exactly
    /// what "it just works now" looks like from the outside.
    fn names_proof(row: &str) -> bool {
        let cites_something = row
            .split('`')
            .skip(1)
            .step_by(2)
            .any(|t| t.trim().len() >= 4);
        let has_long_snake = row
            .split(|c: char| !(c.is_ascii_lowercase() || c == '_'))
            .any(|w| w.len() >= 15 && w.contains('_'));
        // Case-insensitive on purpose: the sentence starts a clause as often
        // as it starts a cell, and a guard that misses "No test, because"
        // while accepting "no test, because" teaches people to phrase around
        // it rather than to answer it.
        let lower = row.to_lowercase();
        let says_none = lower.contains("consciously no")
            || lower.contains("no test, because")
            || lower.contains("superseded");
        cites_something || has_long_snake || says_none
    }

    fn done_rows() -> Vec<(u32, String)> {
        let mut out = Vec::new();
        for line in register().lines() {
            let Some(rest) = line.strip_prefix("| F") else {
                continue;
            };
            let Some((num, _)) = rest.split_once(' ') else {
                continue;
            };
            let Ok(n) = num.parse::<u32>() else { continue };
            // The status is the last non-empty cell.
            let status = line
                .rsplit('|')
                .map(str::trim)
                .find(|c| !c.is_empty())
                .unwrap_or("");
            if status.starts_with("done") {
                out.push((n, line.to_string()));
            }
        }
        assert!(
            out.len() > 50,
            "found only {} finished findings — this test is reading the register wrong, \
             which is worse than failing",
            out.len()
        );
        out
    }

    #[test]
    fn no_finding_recorded_since_the_rule_may_claim_done_without_naming_its_proof() {
        let offenders: Vec<u32> = done_rows()
            .into_iter()
            .filter(|(n, row)| *n >= WATERMARK && !names_proof(row))
            .map(|(n, _)| n)
            .collect();
        assert!(
            offenders.is_empty(),
            "these findings say they are done and name nothing that proves it: {:?}. \
             Name the test, the file, or write down that there is deliberately none \
             and why — a row that claims a fix nobody can check is how F226 shipped.",
            offenders
        );
    }

    /// The backlog is worked off, so this now holds the ground rather than
    /// measuring the descent.
    ///
    /// It was a ratchet — `left <= REMAINING`, lowered as rows were fixed.
    /// At zero clippy pointed out that `left <= 0` for a `usize` is `left ==
    /// 0` and that the staleness half had become always-true: a vacuous
    /// assertion, the exact shape this whole audit is about, in the guard
    /// written to catch it. So it says the true thing instead.
    #[test]
    fn no_finding_at_all_claims_done_while_pointing_at_nothing() {
        let offenders: Vec<u32> = done_rows()
            .into_iter()
            .filter(|(n, row)| *n < WATERMARK && !names_proof(row))
            .map(|(n, _)| n)
            .collect();
        assert!(
            offenders.is_empty(),
            "the backlog behind G19 was worked to zero on 2026-09-02 and these rows have \
             fallen back out of it: {:?}. Restore the citation rather than reopening the \
             backlog — the ground was won once.",
            offenders
        );
    }
}
