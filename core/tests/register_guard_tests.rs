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
//! `(gone: <why, and the commit>)` after its name; a test that lives in
//! another repository gets `(in another repository: <name>)`. Both keep
//! the history readable and tell this check the absence is known.

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core/ has a parent")
        .to_path_buf()
}

fn sources_with_ext(root: &Path, ext: &str) -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name.starts_with("target")
                    || name == ".git"
                    || name == ".claude"
                    || name == "node_modules"
                {
                    continue;
                }
                walk(&p, ext, out);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    let mut paths = Vec::new();
    walk(root, ext, &mut paths);
    paths
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
        .collect()
}

/// A backticked span that names a test by this repository's convention: a
/// register id's kind and number in snake case, then the test's sentence
/// (`fix_240_the_default_wait_fits_a_person`). Narrow on purpose: function
/// names such as `forget_list_by_lane` are cited too, and they are not
/// claims of a test.
fn is_test_name(span: &str) -> bool {
    let mut parts = span.split('_');
    let kind = parts.next().unwrap_or("");
    let kinds = [
        "fix", "feat", "redesign", "gap", "step", "ask", "arch", "tech", "scope",
    ];
    if !kinds.contains(&kind) {
        return false;
    }
    let rest: Vec<&str> = parts.collect();
    // fix_240_x, feat_shell_1_x: a number within the first two parts, and
    // at least one word after it.
    let number_at = rest
        .iter()
        .take(2)
        .position(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()));
    let Some(at) = number_at else {
        return false;
    };
    rest.len() > at + 1
        && span
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// Every test name a register line claims that `exists` cannot find,
/// unless the line marks it `(gone: …)` or `(in another repository…)`
/// after the name. Pure.
fn missing_tests(line: &str, exists: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    let mut offset = 0;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let span = &after[..end];
        let span_end = offset + start + 1 + end + 1;
        rest = &after[end + 1..];
        offset = span_end;
        if !is_test_name(span) || exists(span) {
            continue;
        }
        let tail = &line[span_end..];
        let excused = tail.contains("(gone:") || tail.contains("(in another repository");
        if !excused {
            out.push(span.to_string());
        }
    }
    out
}

/// covers: fix-guards-1
#[test]
fn every_test_a_register_row_names_exists() {
    let root = repo_root();
    let register = std::fs::read_to_string(root.join("docs/deployment/REGISTER.md")).unwrap();
    let rust = sources_with_ext(&root, "rs");
    let js: Vec<(PathBuf, String)> = sources_with_ext(&root, "js")
        .into_iter()
        .filter(|(p, _)| {
            let n = p.to_string_lossy();
            n.ends_with(".test.js") || n.ends_with(".e2e.js")
        })
        .collect();
    let exists = |name: &str| {
        let needle = format!("fn {name}(");
        rust.iter().any(|(_, t)| t.contains(&needle)) || js.iter().any(|(_, t)| t.contains(name))
    };
    let mut broken = Vec::new();
    for line in register.lines().filter(|l| l.starts_with("| ")) {
        let id = line[2..].split(" |").next().unwrap_or("").trim();
        for name in missing_tests(line, &exists) {
            broken.push(format!("{id}: `{name}`"));
        }
    }
    assert!(
        broken.is_empty(),
        "these register rows name tests that do not exist (renamed? deleted?). Fix the \
         name, or write `(gone: <why, commit>)` after it:\n{}",
        broken.join("\n")
    );
}

#[test]
fn missing_tests_refuses_an_unknown_name_and_accepts_a_marked_one() {
    let exists = |n: &str| n == "fix_1_real_test";
    let bad = "| fix-1 | x | Tests `fix_1_real_test`, `fix_1_renamed_since` failed first | done |";
    assert_eq!(missing_tests(bad, &exists), vec!["fix_1_renamed_since"]);
    let gone =
        "| fix-1 | x | Tests `fix_1_renamed_since` (gone: removed by fix-9, abc123) | done |";
    assert!(missing_tests(gone, &exists).is_empty());
    let elsewhere = "| T1 | x | test `fix_7_a_thing_there` (in another repository: kyu) | done |";
    assert!(missing_tests(elsewhere, &exists).is_empty());
    // Function names are cited too; they are not test claims.
    assert!(!is_test_name("forget_list_by_lane"));
    assert!(!is_test_name("fix_142"));
    assert!(is_test_name("feat_shell_1_the_bar_is_one_row"));
    assert!(is_test_name("fix_240_the_default_wait_fits_a_person"));
}

/// Days since 1970-01-01 → (year, month, day), proleptic Gregorian
/// (Howard Hinnant's civil_from_days).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Every `YYYY-MM-DD` in `text` that is not a real date, or lies after
/// `today` (as `(y, m, d)`). Pure.
fn bad_dates(text: &str, today: (i64, u32, u32)) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 10 <= b.len() {
        let w = &b[i..i + 10];
        let digit = |k: usize| w[k].is_ascii_digit();
        let shape = (0..4).all(digit)
            && w[4] == b'-'
            && digit(5)
            && digit(6)
            && w[7] == b'-'
            && digit(8)
            && digit(9);
        let bounded = (i == 0 || !b[i - 1].is_ascii_digit())
            && b.get(i + 10).is_none_or(|c| !c.is_ascii_digit());
        if shape && bounded && w.starts_with(b"20") {
            let s = std::str::from_utf8(w).unwrap();
            let y: i64 = s[0..4].parse().unwrap();
            let m: u32 = s[5..7].parse().unwrap();
            let d: u32 = s[8..10].parse().unwrap();
            let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
            let days_in = match m {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 if leap => 29,
                2 => 28,
                _ => 0,
            };
            if d == 0 || d > days_in {
                out.push(format!("{s} is not a date"));
            } else if (y, m, d) > today {
                out.push(format!("{s} lies in the future"));
            }
            i += 10;
            continue;
        }
        i += 1;
    }
    out
}

/// covers: fix-guards-6
/// fail-first: no status date lay in the future when this guard was added;
/// `bad_dates_refuses_a_future_or_impossible_date` is its constructed case.
#[test]
fn no_status_or_ratification_date_lies_in_the_future() {
    let root = repo_root();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    // One day of slack: a status written just after midnight in Kenny's
    // time zone is still "today" in UTC's yesterday.
    let today = civil_from_days(now.div_euclid(86_400) + 1);
    let mut found = Vec::new();
    let register = std::fs::read_to_string(root.join("docs/deployment/REGISTER.md")).unwrap();
    for line in register.lines().filter(|l| l.starts_with("| ")) {
        let id = line[2..].split(" |").next().unwrap_or("").trim();
        // The status is the last non-empty cell: when something was
        // measured, decided or closed. A finding's text may name a future
        // date on purpose (a certificate that expires in 2027).
        let status = line
            .rsplit('|')
            .map(str::trim)
            .find(|c| !c.is_empty())
            .unwrap_or("");
        for f in bad_dates(status, today) {
            found.push(format!("REGISTER {id}: {f}"));
        }
    }
    let corrections = std::fs::read_to_string(root.join("docs/deployment/CORRECTIONS.md")).unwrap();
    for line in corrections
        .lines()
        .filter(|l| l.starts_with("## ") || l.contains("Ratified"))
    {
        for f in bad_dates(line, today) {
            found.push(format!("CORRECTIONS `{}`: {f}", line.trim()));
        }
    }
    assert!(
        found.is_empty(),
        "dates that cannot be measurements:\n{}",
        found.join("\n")
    );
}

#[test]
fn bad_dates_refuses_a_future_or_impossible_date() {
    let today = (2026, 10, 3);
    assert!(bad_dates("done 2026-10-03: measured", today).is_empty());
    assert_eq!(
        bad_dates("done 2026-10-30: measured", today),
        vec!["2026-10-30 lies in the future"]
    );
    assert_eq!(
        bad_dates("measured 2026-02-30", today),
        vec!["2026-02-30 is not a date"]
    );
    assert!(bad_dates("v3.70.7, 12026-10-03x", today).is_empty());
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_729), (2026, 10, 3));
}
