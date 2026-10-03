//! fix-guards-2, fix-guards-4: `.githooks/check-register.py` refuses what it
//! was written to refuse.
//!
//! The script judges the register rows a commit adds or changes (pre-commit)
//! and, at `make release`, every row of an earlier release. A gate whose
//! refusals are never exercised is a gate nobody knows works, so each rule
//! is driven here with a constructed bad row and a good one.

use std::path::PathBuf;
use std::process::Command;

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".githooks/check-register.py")
}

/// Run the commit mode over a constructed staged diff; (exit code, stderr).
fn commit_check(diff: &str) -> (i32, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("staged.diff");
    std::fs::write(&path, diff).unwrap();
    let out = Command::new("python3")
        .arg(script())
        .arg("--diff")
        .arg(&path)
        .output()
        .expect("python3 runs the register gate");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Run the release mode over a constructed register; (exit code, stderr).
fn release_check(register: &str, version: &str, accept: Option<&str>) -> (i32, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("REGISTER.md");
    std::fs::write(&path, register).unwrap();
    let mut cmd = Command::new("python3");
    cmd.arg(script())
        .arg("--release")
        .arg(version)
        .arg("--register")
        .arg(&path)
        .env_remove("UNMEASURED_OK");
    if let Some(why) = accept {
        cmd.env("UNMEASURED_OK", why);
    }
    let out = cmd.output().expect("python3 runs the register gate");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
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

/// covers: fix-guards-2
#[test]
fn a_row_closed_as_done_without_a_live_measurement_is_refused() {
    let old = "| fix-9 | x | built | doing: release 3.70.1 |";
    for status in [
        "done",
        "done 2026-10-02",
        // The register-audit shape: closed on a tag, explicitly not measured.
        "done 2026-10-02 (register audit): released in a tag at or below v3.70.2. Not re-measured one by one",
    ] {
        let (code, err) = commit_check(&changed(old, &format!("| fix-9 | x | built | {status} |")));
        assert_eq!(code, 1, "accepted `{status}`");
        assert!(err.contains("without a live measurement"), "{err}");
    }
    let good = "| fix-9 | x | built | done 2026-10-03: measured live, `homelab doctor` reads ok |";
    assert_eq!(commit_check(&changed(old, good)).0, 0);
    // A row touched only to correct its text keeps the status it had; the
    // status rules judge the status a commit writes.
    let legacy = "| fix-9 | x | Test `fix_9_a` | done |";
    let renamed = "| fix-9 | x | Test `fix_9_b` (renamed) | done |";
    assert_eq!(commit_check(&changed(legacy, renamed)).0, 0);
}

/// covers: fix-guards-2
#[test]
fn a_released_row_says_how_it_will_be_measured() {
    let old = "| fix-9 | x | built | doing: release 3.70.7 |";
    let bad = "| fix-9 | x | built | released 3.70.7: this row not measured on its own |";
    let (code, err) = commit_check(&changed(old, bad));
    assert_eq!(code, 1);
    assert!(err.contains("measure:"), "{err}");
    let good = "| fix-9 | x | built | released 3.70.7; measure: the next manual backup keeps that night's nightly |";
    assert_eq!(commit_check(&changed(old, good)).0, 0);
}

/// covers: fix-guards-4
#[test]
fn an_open_gap_with_nobody_holding_it_is_refused() {
    let (code, err) = commit_check(&added(
        "| redesign-x-4 | **Finding: a verify does not count as a drill.** | left | open |",
    ));
    assert_eq!(code, 1);
    assert!(err.contains("nobody holding it"), "{err}");
    let held = "| redesign-x-4 | **Finding.** | left | open: Kenny 2026-10-03, Later |";
    assert_eq!(commit_check(&added(held)).0, 0);
}

/// covers: fix-guards-5
#[test]
fn a_new_row_naming_tests_says_they_failed_first() {
    let bad = "| fix-300 | x | Tests `fix_300_the_thing_holds` | doing: release 3.71.0 |";
    let (code, err) = commit_check(&added(bad));
    assert_eq!(code, 1);
    assert!(err.contains("failed first"), "{err}");
    let good = "| fix-300 | x | Tests `fix_300_the_thing_holds` failed first on the old code | doing: release 3.71.0 |";
    assert_eq!(commit_check(&added(good)).0, 0);
    // An existing row whose status changes is not asked again.
    let old = "| fix-300 | x | Tests `fix_300_the_thing_holds` | doing: release 3.71.0 |";
    let new = "| fix-300 | x | Tests `fix_300_the_thing_holds` | done 2026-10-04: measured live |";
    assert_eq!(commit_check(&changed(old, new)).0, 0);
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
| fix-6 | a | b | obsolete 2026-10-02: retired |
";
    let (code, err) = release_check(register, "3.71.0", None);
    assert_eq!(code, 1, "{err}");
    for id in ["fix-1 ", "fix-2 ", "fix-3 "] {
        assert!(err.contains(id), "{id} missing from {err}");
    }
    for id in ["fix-4 ", "fix-5 ", "fix-6 "] {
        assert!(!err.contains(id), "{id} wrongly listed in {err}");
    }
    // A deliberate release past them is possible, and says why.
    let (code, err) = release_check(register, "3.71.0", Some("Kenny 2026-10-04: measure after"));
    assert_eq!(code, 0);
    assert!(err.contains("Kenny 2026-10-04"), "{err}");
    // Nothing older than the release left unmeasured: it passes.
    let clean = "| fix-5 | a | b | doing: release 3.71.0 |\n";
    assert_eq!(release_check(clean, "3.71.0", None).0, 0);
}
