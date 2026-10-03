//! fix-guards-1 to fix-guards-9: the commit and release gates refuse what
//! they were written to refuse, and accept what they prescribe.
//!
//! `.githooks/check-register.py` judges the register rows a commit adds or
//! changes (pre-commit), every row of an earlier release (`make release`),
//! and the documents on disk (the Rust tests over the tree call it). A gate
//! whose refusals are never exercised is a gate nobody knows works, so each
//! rule is driven here, through the real script, with a constructed bad
//! case and a good one. The other hook scripts (`make host-drift`,
//! `.githooks/worktree-target.sh`, `.githooks/pre-commit`'s first checks)
//! are driven the same way.

mod common;

use common::{git, git_ok, repo_root, run_check_register};
use std::path::Path;
use std::process::Command;

/// The day every constructed case is judged on.
const TODAY: &str = "2026-10-03";

/// Run the commit mode over a constructed staged diff, every test name
/// these cases cite existing; (exit code, stderr).
fn commit_check(diff: &str) -> (i32, String) {
    commit_check_with(
        diff,
        &["fix_9_a", "fix_9_b", "fix_300_the_thing_holds"],
        None,
    )
}

/// The commit mode with the test names that exist and an INVARIANTS.md.
fn commit_check_with(diff: &str, known: &[&str], invariants: Option<&str>) -> (i32, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("staged.diff");
    std::fs::write(&path, diff).unwrap();
    let known = known.join(",");
    let path_s = path.to_string_lossy().to_string();
    let mut args = vec!["--diff", &path_s, "--known", &known, "--today", TODAY];
    let inv = dir.path().join("INVARIANTS.md");
    let inv_s = inv.to_string_lossy().to_string();
    if let Some(doc) = invariants {
        std::fs::write(&inv, doc).unwrap();
        args.extend(["--invariants", &inv_s]);
    }
    let (code, _, err) = run_check_register(&args, dir.path(), &[]);
    (code, err)
}

/// Run the release mode over a constructed register; (exit code, stderr).
fn release_check(register: &str, version: &str, accept: Option<&str>) -> (i32, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("REGISTER.md");
    std::fs::write(&path, register).unwrap();
    let path_s = path.to_string_lossy().to_string();
    let env: Vec<(&str, &str)> = accept.map(|w| ("UNMEASURED_OK", w)).into_iter().collect();
    let (code, _, err) = run_check_register(
        &[
            "--release",
            version,
            "--register",
            &path_s,
            "--today",
            TODAY,
        ],
        dir.path(),
        &env,
    );
    (code, err)
}

fn added(row: &str) -> String {
    format!(
        "--- a/docs/deployment/REGISTER.md\n+++ b/docs/deployment/REGISTER.md\n@@ -1,0 +1 @@\n+{row}\n"
    )
}

fn changed(old: &str, new: &str) -> String {
    format!(
        "--- a/docs/deployment/REGISTER.md\n+++ b/docs/deployment/REGISTER.md\n@@ -1 +1 @@\n-{old}\n+{new}\n"
    )
}

/// A status this commit writes on an existing row.
fn status_check(status: &str) -> (i32, String) {
    commit_check(&changed(
        "| fix-9 | x | built | doing: release 3.70.1 |",
        &format!("| fix-9 | x | built | {status} |"),
    ))
}

/// covers: fix-guards-1
#[test]
fn the_commit_gate_refuses_a_row_naming_a_test_that_does_not_exist() {
    let row = "| fix-9 | x | Tests `fix_9_renamed_since` failed first | doing: release 3.71.0 |";
    let (code, err) = commit_check_with(&added(row), &["fix_9_the_real_one"], None);
    assert_eq!(code, 1);
    assert!(err.contains("fix_9_renamed_since"), "{err}");
    assert_eq!(
        commit_check_with(&added(row), &["fix_9_renamed_since"], None).0,
        0
    );
    let gone = "| fix-9 | x | Tests `fix_9_renamed_since` (gone: removed, abc123) failed first | doing: release 3.71.0 |";
    assert_eq!(commit_check_with(&added(gone), &[], None).0, 0);
    // Review M3: a marker excuses the name it follows, not every earlier
    // missing name in the row.
    let one_marker = "| fix-9 | x | Tests `fix_9_also_missing`, `fix_9_renamed_since` (gone: removed, abc123) failed first | doing: release 3.71.0 |";
    let (code, err) = commit_check_with(&added(one_marker), &[], None);
    assert_eq!(code, 1, "one (gone:) excused the whole row");
    assert!(err.contains("fix_9_also_missing"), "{err}");
    assert!(!err.contains("`fix_9_renamed_since`"), "{err}");
}

/// covers: fix-guards-3
#[test]
fn the_commit_gate_refuses_a_staged_invariants_table_with_broken_numbering() {
    let head = "| # | Invariant | Kenny said | Test(s) |\n|---|---|---|---|\n";
    let collided = format!("{head}| 1 | a | b | `t` |\n| 2 | a | b | `t` |\n| 2 | c | d | `t` |\n");
    let (code, err) = commit_check_with("", &[], Some(&collided));
    assert_eq!(code, 1);
    assert!(err.contains("row 2 appears twice"), "{err}");
    let split = format!("{head}| 1 | a | b | `t` |\n\n| 2 | a | b | `t` |\n");
    assert!(
        commit_check_with("", &[], Some(&split))
            .1
            .contains("table is broken")
    );
    let gap = format!("{head}| 1 | a | b | `t` |\n| 3 | a | b | `t` |\n");
    assert!(
        commit_check_with("", &[], Some(&gap))
            .1
            .contains("row 3 follows row 1")
    );
    // Review LOW: the first row is 1 (the Rust check had this, the hook not).
    let late = format!("{head}| 2 | a | b | `t` |\n| 3 | a | b | `t` |\n");
    assert!(
        commit_check_with("", &[], Some(&late))
            .1
            .contains("the first row is 2, not 1")
    );
    // Review LOW: the table is bounded by contiguity, so a later table's
    // numbered rows (or a stray one far below) are found as a cut, not
    // read as part of it.
    let stray = format!(
        "{head}| 1 | a | b | `t` |\n| 2 | a | b | `t` |\n\nSome prose.\n\n| 3 | late | b | `t` |\n"
    );
    let err = commit_check_with("", &[], Some(&stray)).1;
    assert!(err.contains("table is broken"), "{err}");
    assert!(err.contains("row 3"), "{err}");
    let good = format!("{head}| 1 | a | b | `t` |\n| 2 | a | b | `t` |\n\nProse after it.\n");
    assert_eq!(commit_check_with("", &[], Some(&good)).0, 0);
}

/// covers: fix-guards-2
#[test]
fn a_row_closed_as_done_without_a_live_measurement_is_refused() {
    for status in [
        "done",
        "done 2026-10-02",
        // The register-audit shape: closed on a tag, explicitly not measured.
        "done 2026-10-02 (register audit): released in a tag at or below v3.70.2. Not re-measured one by one",
    ] {
        let (code, err) = status_check(status);
        assert_eq!(code, 1, "accepted `{status}`");
        assert!(err.contains("without a live measurement"), "{err}");
    }
    let good = "done 2026-10-03: measured live, `homelab doctor` reads ok";
    assert_eq!(status_check(good).0, 0);
    // A row touched only to correct its text keeps the status it had; the
    // status rules judge the status a commit writes.
    let legacy = "| fix-9 | x | Test `fix_9_a` | done |";
    let renamed = "| fix-9 | x | Test `fix_9_b` (renamed) | done |";
    assert_eq!(commit_check(&changed(legacy, renamed)).0, 0);
}

/// Review M1: the measurement is the clause right after `done <date>:`,
/// judged on its own — a positive "re-measured" passes, a later clause
/// about something else not measured does not void it, and a negation of
/// the measurement itself in any language the register uses refuses it.
///
/// covers: fix-guards-2
#[test]
fn the_measurement_clause_right_after_the_prefix_is_what_is_judged() {
    for good in [
        "done 2026-10-03: re-measured on 3.70.7, the Backups page shows 14 rows",
        "done 2026-10-03: measured the nightly run in the Host log; the restore drill not measured, it has its own row",
        "done 2026-10-03: `homelab doctor` read 0 findings",
    ] {
        let (code, err) = status_check(good);
        assert_eq!(code, 0, "refused `{good}`: {err}");
    }
    for bad in [
        "done 2026-10-03: will be measured at the next nightly run",
        "done 2026-10-03: wasn't measured, the host was down",
        "done 2026-10-03: niet nagemeten",
        "done 2026-10-03: built and released; measured later",
        "done 2026-10-03: the page loads (measured)",
    ] {
        let (code, err) = status_check(bad);
        assert_eq!(code, 1, "accepted `{bad}`");
        assert!(err.contains("without a live measurement"), "{err}");
    }
}

/// Review H1: the shape the gate prescribes for a released row is one the
/// release gate accepts — `measure-after <date>: <how>` — and a released
/// row without it is refused at commit.
///
/// covers: fix-guards-2
#[test]
fn a_released_row_says_when_and_how_it_will_be_measured() {
    let (code, err) = status_check("released 3.70.7: this row not measured on its own");
    assert_eq!(code, 1);
    assert!(err.contains("measure-after <date>: <how>"), "{err}");
    let good = "measure-after 2026-10-19: the first manual backup after the 19th keeps that night's nightly, read on the Backups page";
    assert_eq!(status_check(good).0, 0, "{}", status_check(good).1);
    // Its date is a plan, not a measurement: not a future-date fault.
    assert!(!status_check(good).1.contains("future"));
    // A plan dated in the past is a measurement that is due now.
    let (code, err) = status_check("measure-after 2026-09-01: read the Host log");
    assert_eq!(code, 1);
    assert!(err.contains("lies in the past"), "{err}");
    // And it says how.
    assert_eq!(status_check("measure-after 2026-10-19:").0, 1);
}

/// covers: fix-guards-6
/// fail-first: no status date lay in the future when this guard was added;
/// this constructed case is its proof.
#[test]
fn a_future_or_impossible_date_in_a_written_status_is_refused() {
    let (code, err) = status_check("done 2026-10-30: measured live");
    assert_eq!(code, 1);
    assert!(err.contains("2026-10-30 lies in the future"), "{err}");
    let (code, err) = status_check("done 2026-02-30: measured live");
    assert_eq!(code, 1);
    assert!(err.contains("2026-02-30 is not a date"), "{err}");
    assert_eq!(status_check("done 2026-10-03: measured live").0, 0);
}

/// Review M2: obsolete, superseded, closed, dropped and klopt close a row
/// too, and need a dated reason as much as done needs a measurement.
///
/// covers: fix-guards-4
#[test]
fn a_row_closed_without_a_dated_reason_is_refused() {
    for bad in [
        "obsolete",
        "superseded",
        "closed 2026-10-03",
        "dropped 2026-10-03: no",
        "klopt",
    ] {
        let (code, err) = status_check(bad);
        assert_eq!(code, 1, "accepted `{bad}`");
        assert!(err.contains("without a dated reason"), "{err}");
    }
    let good = "superseded 2026-10-03: fix-240 replaced this with a per-stack wait";
    assert_eq!(status_check(good).0, 0);
}

/// covers: fix-guards-4
#[test]
fn an_open_gap_with_nobody_holding_it_is_refused() {
    for bad in [
        "open",
        // Review M2: words that only sounded like a holder.
        "open: later",
        "later, nothing decided",
        "parked: Kenny",
    ] {
        let (code, err) = commit_check(&added(&format!(
            "| redesign-x-4 | **Finding: a verify does not count as a drill.** | left | {bad} |"
        )));
        assert_eq!(code, 1, "accepted `{bad}`");
        assert!(err.contains("nobody holding it"), "{err}");
    }
    let held = "| redesign-x-4 | **Finding.** | left | open: Kenny 2026-10-03, waits on the drill decision form |";
    assert_eq!(commit_check(&added(held)).0, 0);
}

/// covers: fix-guards-5
#[test]
fn a_new_row_naming_tests_says_they_failed_first() {
    let bad = "| fix-300 | x | Tests `fix_300_the_thing_holds` | doing: release 3.71.0 |";
    let (code, err) = commit_check(&added(bad));
    assert_eq!(code, 1);
    assert!(err.contains("failed first"), "{err}");
    // Review M2: the phrase has to say it happened.
    for denied in [
        "Tests `fix_300_the_thing_holds`; no fail-first run yet",
        "Tests `fix_300_the_thing_holds`, not failed first on the old code",
    ] {
        let row = format!("| fix-300 | x | {denied} | doing: release 3.71.0 |");
        assert_eq!(commit_check(&added(&row)).0, 1, "accepted `{denied}`");
    }
    let good = "| fix-300 | x | Tests `fix_300_the_thing_holds` failed first on the old code | doing: release 3.71.0 |";
    assert_eq!(commit_check(&added(good)).0, 0);
    // An existing row whose status changes is not asked again.
    let old = "| fix-300 | x | Tests `fix_300_the_thing_holds` | doing: release 3.71.0 |";
    let new = "| fix-300 | x | Tests `fix_300_the_thing_holds` | done 2026-10-03: measured live |";
    assert_eq!(commit_check(&changed(old, new)).0, 0);
}

/// Review M5, M6: an escaped pipe stays inside its cell, and every row of a
/// register table is judged whatever its id's shape.
///
/// covers: fix-guards-2
#[test]
fn escaped_pipes_and_every_id_shape_are_read_as_the_table_has_them() {
    // The status is "done \| see below"; split on every pipe it read as
    // "see below" and the unmeasured done went through.
    let (code, err) = status_check(r"done \| see below");
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("without a live measurement"), "{err}");
    for id in ["M-G7", "tui-host-settings", "F186"] {
        let (code, err) = commit_check(&added(&format!("| {id} | x | y | done |")));
        assert_eq!(code, 1, "{id} was not judged");
        assert!(err.contains(id), "{err}");
    }
}

/// Review H4: a register id added in a string a person reads is refused at
/// commit, where the Rust and node checks (which wait for the suite) do not
/// run; comments, tests, commit trailers and marked lines pass.
///
/// covers: fix-guards-8
#[test]
fn the_commit_gate_refuses_a_register_id_in_an_added_user_facing_string() {
    let run = |diff: &str| {
        let dir = tempfile::tempdir().unwrap();
        let code = dir.path().join("code.diff");
        let empty = dir.path().join("empty.diff");
        std::fs::write(&code, diff).unwrap();
        std::fs::write(&empty, "").unwrap();
        run_check_register(
            &[
                "--diff",
                &empty.to_string_lossy(),
                "--code-diff",
                &code.to_string_lossy(),
                "--today",
                TODAY,
            ],
            dir.path(),
            &[],
        )
    };
    let file =
        |path: &str, line: &str| format!("--- a/{path}\n+++ b/{path}\n@@ -1,0 +1 @@\n+{line}\n");
    for (path, line) in [
        (
            "core/src/ops/x.rs",
            r#"    msg("updates parked (H8): investigate")"#,
        ),
        (
            "core/src/ops/x.rs",
            r#"    "older than fix-112, refused".into()"#,
        ),
        (
            "admin/web/js/pages/x.js",
            r#"  hint: "Every stack a destroy removed (ask-9) is kept","#,
        ),
    ] {
        let (code, _, err) = run(&file(path, line));
        assert_eq!(code, 1, "accepted {line}");
        assert!(err.contains("names the register id"), "{err}");
    }
    for (path, line) in [
        (
            "core/src/ops/x.rs",
            "    // fix-112: a comment names the id",
        ),
        ("core/src/ops/x.rs", r#"    let m = "chore: x [fix-110]";"#),
        (
            "core/tests/x_tests.rs",
            r#"    let m = "fixture fix-1 here";"#,
        ),
        (
            "core/src/ops/x.rs",
            r#"    let m = "subject fix-1 part"; // id-ok: a commit subject"#,
        ),
        (
            "core/src/ops/x.rs",
            r#"    let m = "sha-256 and utf-8, v3.70.2";"#,
        ),
    ] {
        let (code, _, err) = run(&file(path, line));
        assert_eq!(code, 0, "refused {line}: {err}");
    }
}

/// covers: fix-guards-2
#[test]
fn the_release_gate_refuses_rows_of_an_earlier_release_that_were_never_measured() {
    let register = "\
| fix-1 | a | b | doing: release 3.70.1 |
| fix-2 | a | b | released 3.70.7: live since 2026-10-03 (page version measured); this row not measured on its own |
| fix-3 | a | b | built 2026-10-01, ships in 3.70.0 |
| fix-4 | a | b | done 2026-10-03: measured live, 3.70.7 |
| fix-5 | a | b | doing: release 3.71.0 |
| fix-6 | a | b | obsolete 2026-10-02: retired with the uptime stack |
| feat-x-1 | a | b | doing: rollout of v3.70.2 |
| fix-7 | a | b | blocked on chassis-rs released 3.4.0 |
| fix-8 | a | b | released 3.70.7; measured 2026-10-03 with `homelab doctor`, 0 findings |
";
    let (code, err) = release_check(register, "3.71.0", None);
    assert_eq!(code, 1, "{err}");
    for id in ["fix-1:", "fix-2:", "fix-3:", "feat-x-1:"] {
        assert!(err.contains(id), "{id} missing from {err}");
    }
    // Review LOW: another project's version is not a homelab release; a
    // correctly measured row passes.
    for id in ["fix-4:", "fix-5:", "fix-6:", "fix-7:", "fix-8:"] {
        assert!(!err.contains(id), "{id} wrongly listed in {err}");
    }
    // Review H2: grouped by release, then by kind, with counts.
    assert!(err.contains("3.70.7 — 1 row(s)"), "{err}");
    assert!(err.contains("3.70.2 — 1 row(s)"), "{err}");
    assert!(err.contains("  feat-x (1)"), "{err}");
    // Review H2: the 3.70.7 row passes once reworded to the done shape.
    let reworded = register.replace(
        "released 3.70.7: live since 2026-10-03 (page version measured); this row not measured on its own",
        "done 2026-10-03: measured this row on 3.70.7 through Live view, the Backups page lists 14 runs",
    );
    assert!(
        !release_check(&reworded, "3.71.0", None)
            .1
            .contains("fix-2:")
    );
    // A deliberate release past them is possible, and says why.
    let (code, err) = release_check(register, "3.71.0", Some("Kenny 2026-10-04: measure after"));
    assert_eq!(code, 0);
    assert!(err.contains("Kenny 2026-10-04"), "{err}");
    // Nothing older than the release left unmeasured: it passes.
    let clean = "| fix-5 | a | b | doing: release 3.71.0 |\n";
    assert_eq!(release_check(clean, "3.71.0", None).0, 0);
}

/// Review H1: a `measure-after` row waits until its date and blocks from
/// then on.
///
/// covers: fix-guards-2
#[test]
fn the_release_gate_respects_measure_after_until_its_date() {
    let waiting = "| fix-1 | a | b | measure-after 2026-10-19: released 3.70.7; the first manual backup after the 19th keeps that night's nightly |\n";
    assert_eq!(release_check(waiting, "3.71.0", None).0, 0);
    let due = "| fix-1 | a | b | measure-after 2026-10-01: released 3.70.7; read the Host log for the nightly |\n";
    let (code, err) = release_check(due, "3.71.0", None);
    assert_eq!(code, 1);
    assert!(err.contains("fix-1:"), "{err}");
}

/// A released row a test proves on the demo host (or a core test on the
/// mock executor) closes as `proven`, never as measured: the gate accepts
/// it only while every test it names exists, with "passed" and the run's
/// duration said, at commit and at the release alike.
///
/// covers: fix-guards-10
#[test]
fn a_test_proven_row_closes_only_while_its_named_tests_exist() {
    let case = "invariants: the bar stays in one row";
    let good = format!(
        "proven 2026-10-03: `fix_9_a` and `admin/web/test-e2e/invariants.e2e.js: \"{case}\"` passed on the demo host (41 s)"
    );
    let known = ["fix_9_a", case];
    let commit = |status: &str, known: &[&str]| {
        commit_check_with(
            &changed(
                "| fix-9 | x | built | doing: release 3.70.1 |",
                &format!("| fix-9 | x | built | {status} |"),
            ),
            known,
            None,
        )
    };
    assert_eq!(commit(&good, &known).0, 0, "{}", commit(&good, &known).1);
    // A named case that does not exist, either kind, is refused.
    let (code, err) = commit(&good, &["fix_9_a"]);
    assert_eq!(code, 1);
    assert!(err.contains("does not exist"), "{err}");
    let (code, err) = commit(&good, &[case]);
    assert_eq!(code, 1);
    assert!(err.contains("`fix_9_a`"), "{err}");
    // No test, no "passed", no duration, or a live claim: refused.
    for bad in [
        "proven 2026-10-03: the demo host showed it (41 s), passed",
        "proven 2026-10-03: `fix_9_a` passed on the demo host",
        "proven 2026-10-03: `fix_9_a` ran on the demo host (41 s)",
        "proven 2026-10-03: `fix_9_a` passed and measured live (41 s)",
    ] {
        assert_eq!(commit(bad, &known).0, 1, "{bad} was accepted");
    }
    // The release gate: closed while the tests exist, listed once one is gone.
    let register = format!("| fix-9 | a | b | {good} |\n");
    let release = |known: &str| {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("REGISTER.md");
        std::fs::write(&path, &register).unwrap();
        let path_s = path.to_string_lossy().to_string();
        run_check_register(
            &[
                "--release",
                "3.71.0",
                "--register",
                &path_s,
                "--today",
                TODAY,
                "--known",
                known,
            ],
            dir.path(),
            &[],
        )
    };
    let (code, _, err) = release(&format!("fix_9_a,{case}"));
    assert_eq!(code, 0, "{err}");
    let (code, _, err) = release("fix_9_a");
    assert_eq!(code, 1);
    assert!(
        err.contains("fix-9:") && err.contains("does not exist"),
        "{err}"
    );
}

/// `scripts/measure-due.py` (wired into `make release`): a `measure-after`
/// row whose day has come is read with its one read-only command; a match
/// of its `expect /…/` is written back as done, a command off the
/// read-only list is never run, and a due row left unmeasured blocks.
///
/// covers: fix-guards-10
#[test]
fn measure_due_reads_a_due_row_with_its_own_read_only_command() {
    let dir = tempfile::tempdir().unwrap();
    let fake = dir.path().join("homelab");
    std::fs::write(&fake, "#!/bin/sh\necho '{\"trigger\":\"nightly\"}'\n").unwrap();
    Command::new("chmod").arg("+x").arg(&fake).status().unwrap();
    let reg = dir.path().join("REGISTER.md");
    let row = |status: &str| format!("| fix-9 | a | b | {status} |\n");
    let run = |text: &str, today: &str| {
        std::fs::write(&reg, text).unwrap();
        let out = Command::new("python3")
            .arg(repo_root().join("scripts/measure-due.py"))
            .args(["--run", "--write", "--today", today, "--register"])
            .arg(&reg)
            .arg("--homelab")
            .arg(&fake)
            .env_remove("UNMEASURED_OK")
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        (
            out.status.code(),
            text,
            std::fs::read_to_string(&reg).unwrap(),
        )
    };
    let waiting = row(
        "measure-after 2026-10-04: `homelab snapshots stacks/x --json` shows the tag; expect /\"trigger\":\"nightly\"/",
    );
    // Before its day: nothing runs, nothing blocks.
    let (code, text, after) = run(&waiting, "2026-10-03");
    assert_eq!(code, Some(0), "{text}");
    assert_eq!(after, waiting);
    // On its day: read, matched, written as done.
    let (code, text, after) = run(&waiting, "2026-10-04");
    assert_eq!(code, Some(0), "{text}");
    assert!(
        after.contains("| done 2026-10-04: `homelab snapshots stacks/x --json` read"),
        "{after}"
    );
    // A pattern the output lacks: fail, left as it was, blocks.
    let missing = waiting.replace("nightly\"/", "manual\"/");
    let (code, text, after) = run(&missing, "2026-10-04");
    assert_eq!(code, Some(1), "{text}");
    assert!(text.contains("fail:"), "{text}");
    assert_eq!(after, missing);
    // A command that writes is never run: needs eyes, blocks.
    let writes = row("measure-after 2026-10-04: `homelab deploy stacks/x` then look; expect /ok/");
    let (code, text, _) = run(&writes, "2026-10-04");
    assert_eq!(code, Some(1), "{text}");
    assert!(
        text.contains("needs-eyes: its command is not on the read-only list"),
        "{text}"
    );
}

/// Review M2: an override is written where the tag's annotation reads it,
/// with the rows it went past.
///
/// covers: fix-guards-2
#[test]
fn an_override_of_the_release_gate_is_recorded_for_the_tag() {
    let dir = tempfile::tempdir().unwrap();
    let reg = dir.path().join("REGISTER.md");
    std::fs::write(&reg, "| fix-1 | a | b | doing: release 3.70.1 |\n").unwrap();
    let record = dir.path().join("overrides.txt");
    let (code, _, _) = run_check_register(
        &[
            "--release",
            "3.71.0",
            "--register",
            &reg.to_string_lossy(),
            "--record",
            &record.to_string_lossy(),
            "--today",
            TODAY,
        ],
        dir.path(),
        &[("UNMEASURED_OK", "Kenny 2026-10-03: the drill rows wait")],
    );
    assert_eq!(code, 0);
    let text = std::fs::read_to_string(&record).expect("the override is recorded");
    assert!(
        text.contains("UNMEASURED_OK: Kenny 2026-10-03: the drill rows wait"),
        "{text}"
    );
    assert!(text.contains("fix-1: doing: release 3.70.1"), "{text}");
    // The release target writes that file into the tag's annotation, and
    // the host-drift override into the same file.
    let make = std::fs::read_to_string(repo_root().join("Makefile")).unwrap();
    assert!(make.contains("--record \"$(RELEASE_RECORD_FILE)\""));
    assert!(make.contains("git tag -a \"v$(VERSION)\" -F \"$(RELEASE_RECORD_FILE).msg\""));
    assert!(make.contains("HOST_DRIFT_OK: $$HOST_DRIFT_OK\" >>\"$(RELEASE_RECORD)\""));
}

/// Review LOW: a malformed call ends in one line saying what is wrong, not
/// a Python traceback.
#[test]
fn a_malformed_call_is_refused_without_a_traceback() {
    let dir = tempfile::tempdir().unwrap();
    for args in [
        vec!["--release", "3.71.0rc1"],
        vec!["--release"],
        vec!["--diff"],
    ] {
        let (code, _, err) = run_check_register(&args, dir.path(), &[]);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(!err.contains("Traceback"), "{args:?}: {err}");
        assert!(err.starts_with("check-register.py: "), "{err}");
    }
}

/// Review H3: in a conflicted merge, a row the merge brings unchanged from
/// one side was judged where it was written; only a row that differs from
/// every parent is this commit's own.
///
/// covers: fix-guards-2
#[test]
fn a_merge_is_judged_on_the_rows_it_writes_itself() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let reg = repo.join("docs/deployment");
    std::fs::create_dir_all(&reg).unwrap();
    let file = reg.join("REGISTER.md");
    let base = "| # | Finding | Impact | Status |\n|---|---|---|---|\n| fix-1 | a | b | doing: release 3.71.0 |\n";
    git(repo, &["init", "-q", "-b", "main"]);
    std::fs::write(&file, base).unwrap();
    git(repo, &["add", "."]);
    git(repo, &["commit", "-q", "-m", "base"]);
    git(repo, &["checkout", "-q", "-b", "side"]);
    // Written on the side branch under older rules: a bare "done".
    std::fs::write(&file, format!("{base}| fix-2 | a | b | done |\n")).unwrap();
    git(repo, &["commit", "-q", "-am", "side"]);
    git(repo, &["checkout", "-q", "main"]);
    std::fs::write(
        &file,
        format!("{base}| fix-3 | a | b | doing: release 3.71.0 |\n"),
    )
    .unwrap();
    git(repo, &["commit", "-q", "-am", "main"]);
    assert!(
        !git_ok(repo, &["merge", "-q", "side"]),
        "the merge conflicts"
    );
    // The resolution keeps both sides' rows.
    std::fs::write(
        &file,
        format!("{base}| fix-3 | a | b | doing: release 3.71.0 |\n| fix-2 | a | b | done |\n"),
    )
    .unwrap();
    git(repo, &["add", "."]);
    let (code, _, err) = run_check_register(&["--today", TODAY], repo, &[]);
    assert_eq!(code, 0, "a row the merge did not write was judged: {err}");
    // A row the resolution writes itself is judged.
    std::fs::write(
        &file,
        format!(
            "{base}| fix-3 | a | b | doing: release 3.71.0 |\n| fix-2 | a | b | done |\n| fix-4 | a | b | done |\n"
        ),
    )
    .unwrap();
    git(repo, &["add", "."]);
    let (code, _, err) = run_check_register(&["--today", TODAY], repo, &[]);
    assert_eq!(code, 1, "{err}");
    assert!(err.contains("fix-4") && !err.contains("fix-2:"), "{err}");
}

/// fix-guards-7 (review H5a): drift and an unreachable host are told
/// apart — drift is the client's exit 3; any other failure is "could not
/// ask", which is no verdict on drift.
///
/// covers: fix-guards-7
#[test]
fn make_host_drift_tells_drift_from_an_unreachable_host() {
    let run = |code: i32, env: &[(&str, &str)]| {
        let mut cmd = Command::new("make");
        cmd.arg("-s")
            .arg("--no-print-directory")
            .arg("host-drift")
            .arg(format!("HOST_DIFF=sh -c 'exit {code}'"))
            .current_dir(repo_root())
            .env_remove("HOST_DRIFT_OK")
            .env_remove("MAKEFLAGS");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("make runs");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stderr).to_string(),
        )
    };
    assert_eq!(homelab_core::hostconfig::DRIFT_EXIT_CODE, 3);
    assert!(run(0, &[]).0);
    let (ok, drift) = run(3, &[]);
    assert!(!ok);
    assert!(drift.contains("disagree"), "{drift}");
    let (ok, unreachable) = run(1, &[]);
    assert!(!ok);
    assert!(unreachable.contains("could not be asked"), "{unreachable}");
    assert!(unreachable.contains("not a drift verdict"), "{unreachable}");
    assert!(!unreachable.contains("disagree"), "{unreachable}");
    // The release builds the client as it ships it.
    let make = std::fs::read_to_string(repo_root().join("Makefile")).unwrap();
    assert!(make.contains("HOST_DIFF ?= cargo run --release"));
}

/// The script with a clean environment in `cwd`; its stdout, trimmed.
fn worktree_target(cwd: &Path, targets: &Path, explicit: Option<&str>) -> String {
    let mut cmd = Command::new(repo_root().join(".githooks/worktree-target.sh"));
    cmd.current_dir(cwd)
        .env("HOMELAB_TARGETS", targets)
        .env("HOMELAB_TARGETS_KEEP", "2")
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    if let Some(dir) = explicit {
        cmd.env("CARGO_TARGET_DIR", dir);
    }
    let out = cmd.output().expect("the script runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// fix-guards-9 (coordinator, 2026-10-03): two worktrees committing at once
/// built in the one shared target directory, and one hook compiled the
/// other tree's sources. Each linked worktree now gets its own directory;
/// the main checkout keeps the shared one; an explicit CARGO_TARGET_DIR
/// wins; a gone worktree's directory is removed and only the most recently
/// used ones are kept.
///
/// covers: fix-guards-9
#[test]
fn each_worktree_builds_in_its_own_target_directory() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main");
    let targets = dir.path().join("targets");
    std::fs::create_dir_all(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    std::fs::write(main.join("a"), "a").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-q", "-m", "a"]);
    let wt = |n: &str| {
        let p = dir.path().join(n);
        git(
            &main,
            &["worktree", "add", "-q", "--detach", &p.to_string_lossy()],
        );
        p
    };
    let (one, two) = (wt("one"), wt("two"));
    assert_eq!(
        worktree_target(&main, &targets, None),
        "",
        "the main checkout keeps the shared directory"
    );
    let t1 = worktree_target(&one, &targets, None);
    let t2 = worktree_target(&two, &targets, None);
    assert!(
        t1.starts_with(&*targets.to_string_lossy()) && t1 != t2,
        "{t1} {t2}"
    );
    assert_eq!(
        worktree_target(&one, &targets, None),
        t1,
        "stable per worktree"
    );
    assert_eq!(
        worktree_target(&one, &targets, Some("/x")),
        "",
        "an explicit choice wins"
    );
    // A removed worktree's directory goes at the next call from anywhere.
    std::fs::write(Path::new(&t2).join("big"), "x").unwrap();
    git(
        &main,
        &["worktree", "remove", "--force", &two.to_string_lossy()],
    );
    worktree_target(&main, &targets, None);
    assert!(
        !Path::new(&t2).exists(),
        "the gone worktree's directory is still there"
    );
    assert!(Path::new(&t1).exists());
    // The cap (2 here): the least recently used directory goes.
    let three = wt("three");
    let four = wt("four");
    worktree_target(&three, &targets, None);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    worktree_target(&four, &targets, None);
    let kept = std::fs::read_dir(&targets).unwrap().count();
    assert_eq!(kept, 2, "the cap kept {kept}");
    assert!(
        !Path::new(&t1).exists(),
        "the least recently used one stayed"
    );
    // The gates use it.
    let gates = std::fs::read_to_string(repo_root().join(".claude/hooks/gates.sh")).unwrap();
    assert!(gates.contains(".githooks/worktree-target.sh"));
}

/// Review LOW: pre-commit says python3 is missing before anything else,
/// instead of failing halfway with "command not found".
#[test]
fn pre_commit_refuses_plainly_when_python3_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    for tool in ["git", "cargo"] {
        let real = Command::new("sh")
            .arg("-c")
            .arg(format!("command -v {tool}"))
            .output()
            .unwrap();
        let real = String::from_utf8_lossy(&real.stdout).trim().to_string();
        std::os::unix::fs::symlink(real, bin.join(tool)).unwrap();
    }
    let bash = Command::new("sh")
        .arg("-c")
        .arg("command -v bash")
        .output()
        .unwrap();
    let bash = String::from_utf8_lossy(&bash.stdout).trim().to_string();
    let out = Command::new(bash)
        .arg(repo_root().join(".githooks/pre-commit"))
        .current_dir(repo_root())
        .env("PATH", &bin)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(err.contains("python3 not found"), "{err}");
}

/// fix-guards-5 (review M4): `make fail-first` reports only what it proved.
/// On a constructed crate whose range adds a test that fails on the old
/// code (through a helper in tests/common/), one that passes there, one
/// that cannot build there and one without `covers:`: only the first is
/// proof; the next two are refused; the last is listed; the throwaway
/// worktree and its build output are gone afterwards.
///
/// covers: fix-guards-5
#[test]
fn fail_first_proves_only_a_test_that_fails_on_the_old_code() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let crate_dir = repo.join("mini");
    std::fs::create_dir_all(crate_dir.join("src")).unwrap();
    git(dir.path(), &["init", "-q", "-b", "main", "repo"]);
    std::fs::write(
        repo.join("Cargo.toml"),
        "[workspace]\nmembers = [\"mini\"]\n",
    )
    .unwrap();
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        "[package]\nname = \"mini\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(crate_dir.join("src/lib.rs"), "pub fn f() -> i32 { 1 }\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    let base = git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    std::fs::write(
        crate_dir.join("src/lib.rs"),
        "pub fn f() -> i32 { 2 }\npub fn g() -> i32 { 3 }\n",
    )
    .unwrap();
    std::fs::create_dir_all(crate_dir.join("tests/common")).unwrap();
    std::fs::write(
        crate_dir.join("tests/common/mod.rs"),
        "pub fn two() -> i32 { 2 }\n",
    )
    .unwrap();
    std::fs::write(
        crate_dir.join("tests/t.rs"),
        [
            "mod common;\n\n",
            "/// covers: fix-guards-5\n#[test]\nfn proves() {\n    assert_eq!(mini::f(), common::two());\n}\n\n",
            "/// covers: fix-guards-5\n#[test]\nfn vacuous() {\n    assert!(mini::f() > 0);\n}\n\n",
            "#[test]\nfn unclaimed() {}\n",
        ]
        .concat(),
    )
    .unwrap();
    std::fs::write(
        crate_dir.join("tests/u.rs"),
        "/// covers: fix-guards-5\n#[test]\nfn needs_new_api() {\n    assert_eq!(mini::g(), 3);\n}\n",
    )
    .unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "fix"]);
    // The working tree differs from HEAD: the script reads HEAD.
    std::fs::write(crate_dir.join("tests/u.rs"), "garbage that does not parse").unwrap();
    let work = dir.path().join("ff");
    let out = Command::new("python3")
        .arg(repo_root().join("scripts/fail-first.py"))
        .arg(&base)
        .current_dir(&repo)
        .env("FAIL_FIRST_DIR", &work)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let line = |name: &str| {
        text.lines()
            .find(|l| l.contains(&format!(":: {name}")))
            .unwrap_or_else(|| panic!("{name} not reported:\n{text}"))
            .to_string()
    };
    assert!(
        line("proves").starts_with("fails on the old code"),
        "{text}"
    );
    assert!(
        line("vacuous").starts_with("PASSES on the old code"),
        "{text}"
    );
    assert!(
        line("needs_new_api").starts_with("DOES NOT BUILD"),
        "{text}"
    );
    assert!(line("unclaimed").starts_with("no covers:"), "{text}");
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("2 claimed test(s) are not proven"), "{text}");
    // Nothing left behind: no worktree, no build output.
    let left: Vec<_> = std::fs::read_dir(&work)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "left behind: {left:?}");
    assert!(!git(&repo, &["worktree", "list"]).contains("ff"));
    // 0 matched is no failure.
    let verdict = Command::new("python3")
        .arg("-c")
        .arg(
            "import importlib.util,sys\n\
             s=importlib.util.spec_from_file_location('ff',sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)\n\
             print(m.rust_verdict('test result: ok. 0 passed; 0 failed; 0 ignored'))",
        )
        .arg(repo_root().join("scripts/fail-first.py"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&verdict.stdout).starts_with("NOT FOUND"));
}

/// A scratch repository holding this tree's dashboard web code (its
/// node_modules linked, not copied) and the catalog hook, committed.
fn web_scratch(dir: &Path) -> std::path::PathBuf {
    let repo = dir.join("repo");
    let web = repo.join("admin/web");
    std::fs::create_dir_all(repo.join(".githooks")).unwrap();
    std::fs::create_dir_all(&web).unwrap();
    let src = repo_root().join("admin/web");
    for part in ["js", "scripts", "test", "package.json"] {
        let ok = Command::new("cp")
            .arg("-a")
            .arg(src.join(part))
            .arg(&web)
            .status()
            .unwrap();
        assert!(ok.success(), "cp {part}");
    }
    std::os::unix::fs::symlink(src.join("node_modules"), web.join("node_modules")).unwrap();
    std::fs::copy(
        repo_root().join(".githooks/drivecatalog.sh"),
        repo.join(".githooks/drivecatalog.sh"),
    )
    .unwrap();
    // The node hook finds the kit's components through Cargo.lock's pin.
    std::fs::copy(repo_root().join("Cargo.lock"), repo.join("Cargo.lock")).unwrap();
    std::fs::write(repo.join(".gitignore"), "node_modules\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "base"]);
    repo
}

/// The catalog hook in `repo`: (exit code, stdout + stderr).
fn catalog_hook(repo: &Path) -> (i32, String) {
    let out = Command::new("bash")
        .arg(repo.join(".githooks/drivecatalog.sh"))
        .current_dir(repo)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

/// drive-reach review M1: the commit gate never ran the catalog checks
/// (gates.sh skips node tests at commit). The hook regenerates a stale
/// catalog and stages it, and runs its test alone, refusing a declaration
/// the catalog cannot hold.
///
/// covers: redesign-drive-5
#[test]
fn the_commit_gate_regenerates_the_control_catalog_and_runs_its_test() {
    if Command::new("node").arg("--version").output().is_err()
        || !repo_root().join("admin/web/node_modules").is_dir()
    {
        panic!("this test needs node and admin/web/node_modules (npm ci)");
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = web_scratch(dir.path());
    let cat = repo.join("admin/web/js/drivecatalog.json");
    let good = std::fs::read_to_string(&cat).unwrap();
    // A commit that changed the declarations but staged the old catalog.
    std::fs::write(&cat, "{}\n").unwrap();
    git(&repo, &["add", "admin/web/js/drivecatalog.json"]);
    let (code, out) = catalog_hook(&repo);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("regenerated and staged"), "{out}");
    assert!(out.contains("drivecatalog.test.js passed"), "{out}");
    assert_eq!(std::fs::read_to_string(&cat).unwrap(), good);
    let staged = git(&repo, &["show", ":admin/web/js/drivecatalog.json"]);
    assert_eq!(
        staged.trim(),
        good.trim(),
        "the staged catalog is not the built one"
    );
    // A page that marks an element by hand fails the commit.
    let page = repo.join("admin/web/js/pages/shell.js");
    let mut src = std::fs::read_to_string(&page).unwrap();
    src.push_str("\nexport const bad = (el) => el.setAttribute(\"data-drive\", \"x\");\n");
    std::fs::write(&page, src).unwrap();
    let (code, out) = catalog_hook(&repo);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("COMMIT BLOCKED"), "{out}");
    // pre-commit calls it whenever the dashboard's web code moves.
    let hook = std::fs::read_to_string(repo_root().join(".githooks/pre-commit")).unwrap();
    assert!(
        hook.contains(".githooks/drivecatalog.sh"),
        "pre-commit does not call it"
    );
}
