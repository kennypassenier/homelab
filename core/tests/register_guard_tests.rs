//! fix-guards-1, fix-guards-6: two register claims that nothing checked.
//!
//! Kenny, 2026-10-03: "je zou je gedragen als een world-class developer,
//! maar ik zie vaak amateuristische fouten, die moeten eruit". Two of the
//! recurring ones live in this register's own text:
//!
//! * **A row names a test that does not exist.** `register_tests.rs` only
//!   follows the narrow ``Test: `name` `` shape; most rows say "Tests `a`,
//!   `b` failed first", and nothing looked those names up. Measured on
//!   2026-10-03: 22 test names in 11 rows pointed at nothing — renamed,
//!   deleted with the Grafana and Uptime Kuma retirement, with the GitHub
//!   Actions workflows, or by fix-199 removing the version gates fix-105
//!   and fix-185 had tested (one, T64's, lives in http-switchboard). A
//!   row whose proof has quietly gone reads
//!   exactly like one whose proof is there.
//! * **A date in a status cell lies in the future.** A status says when
//!   something was measured or decided; a date after today is a typo or a
//!   guess, never a measurement.
//!
//! The convention a row uses when a named test is gone on purpose: write
//! `(gone: <why, and the commit>)` right after its closing backtick; a test
//! that lives in another repository gets `(in another repository: <name>)`
//! there. Both keep the history readable and tell this check the absence is
//! known.
//!
//! One implementation judges: `.githooks/check-register.py`, which the
//! commit hook runs on what a commit stages and these tests run on the
//! tree (`--tree tests`, `--tree dates`). Its constructed bad cases are in
//! register_hook_tests.rs.

mod common;

use common::{repo_root, run_check_register};

/// covers: fix-guards-1
#[test]
fn every_test_a_register_row_names_exists() {
    let root = repo_root();
    let (code, _, err) = run_check_register(&["--tree", "tests"], &root, &[]);
    assert_eq!(
        code, 0,
        "these register rows name tests that do not exist (renamed? deleted?). Fix the \
         name, or write `(gone: <why, commit>)` right after it:\n{err}"
    );
}

/// covers: fix-guards-6
/// fail-first: no status date lay in the future when this guard was added;
/// `a_future_or_impossible_date_in_a_written_status_is_refused` in
/// register_hook_tests.rs is its constructed case.
#[test]
fn no_status_or_ratification_date_lies_in_the_future() {
    let root = repo_root();
    let (code, _, err) = run_check_register(&["--tree", "dates"], &root, &[]);
    assert_eq!(code, 0, "dates that cannot be measurements:\n{err}");
}
