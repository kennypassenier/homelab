//! Phase 7's output document, generated from the tests themselves.
//!
//! `PROCEDURE.md` asks Phase 7 to leave behind a `TEST_PLAN.md` describing
//! every suite and every accepted limitation. The orchestrator's is 482 lines
//! written by hand, and Kenny's rule of 2026-09-02 is that a file a human has
//! to keep in step with reality will drift — so this one is derived instead.
//!
//! What it reads: each test file's `//!` header for what the suite is for,
//! every `#[test]`/`#[tokio::test]` for what is actually checked, and the
//! `covers: F123` annotations that tie a test to the finding it exists for.
//! What it cannot read — the limitations somebody consciously accepted —
//! comes from the gap table in `REALIZATION_PLAN.md`, so there is one source
//! of truth for those rather than a second list to keep in step.

use std::path::Path;

struct Suite {
    file: String,
    purpose: String,
    tests: Vec<(String, String)>,
    covers: Vec<String>,
}

/// The first paragraph of a `//!` header — enough to say what a suite is for
/// without reprinting its whole rationale.
fn header_purpose(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
        let Some(rest) = line.strip_prefix("//!") else {
            break;
        };
        let t = rest.trim();
        if t.is_empty() {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(t);
    }
    out
}

/// A test's own one-line description: the first sentence of its doc comment,
/// or nothing when it has none. The name is already a sentence in this
/// codebase, so an absent comment is not a gap.
fn doc_above(lines: &[&str], idx: usize) -> String {
    let mut docs: Vec<String> = Vec::new();
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.starts_with("#[") || t.is_empty() {
            continue;
        }
        if let Some(d) = t.strip_prefix("///") {
            docs.push(d.trim().to_string());
            continue;
        }
        break;
    }
    docs.reverse();
    let joined = docs.join(" ");
    match joined.split_once(". ") {
        Some((first, _)) => format!("{}.", first),
        None => joined,
    }
}

fn read_suite(path: &Path) -> Option<Suite> {
    let src = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = src.lines().collect();
    let mut tests = Vec::new();
    let mut covers: Vec<String> = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if let Some(rest) = t.strip_prefix("/// covers:") {
            for f in rest.split(',') {
                let f = f.trim().trim_end_matches('.');
                if !f.is_empty() {
                    covers.push(f.to_string());
                }
            }
        }
        if !t.starts_with("fn ") && !t.starts_with("async fn ") && !t.starts_with("pub fn ") {
            continue;
        }
        // Only functions the harness actually runs.
        let is_test = lines[..i]
            .iter()
            .rev()
            .take(6)
            .any(|p| p.trim() == "#[test]" || p.trim() == "#[tokio::test]");
        if !is_test {
            continue;
        }
        let name = t
            .trim_start_matches("pub ")
            .trim_start_matches("async ")
            .trim_start_matches("fn ")
            .split('(')
            .next()
            .unwrap_or("")
            .to_string();
        if name.is_empty() {
            continue;
        }
        tests.push((name, doc_above(&lines, i)));
    }
    if tests.is_empty() {
        return None;
    }
    covers.sort();
    covers.dedup();
    Some(Suite {
        file: path
            .strip_prefix(std::env::current_dir().unwrap_or_default())
            .unwrap_or(path)
            .display()
            .to_string(),
        purpose: header_purpose(&src),
        tests,
        covers,
    })
}

/// The gaps somebody consciously decided NOT to close, lifted out of the gap
/// table so this document cannot disagree with the plan.
fn accepted_limitations(plan: &str) -> Vec<String> {
    plan.lines()
        .filter(|l| l.starts_with("| G"))
        .filter(|l| {
            let lower = l.to_lowercase();
            lower.contains("**later**") || lower.contains("| later")
        })
        .map(|l| {
            let cells: Vec<&str> = l.split('|').map(str::trim).collect();
            format!(
                "- **{}** — {} · _{}_",
                cells.get(1).unwrap_or(&""),
                cells.get(2).unwrap_or(&""),
                cells.get(3).unwrap_or(&"")
            )
        })
        .collect()
}

/// Write the plan. Returns how many suites it described.
pub fn generate_test_plan(roots: &[&Path], plan_path: &Path, out: &Path) -> Result<usize, String> {
    let mut suites: Vec<Suite> = Vec::new();
    for root in roots {
        let Ok(rd) = std::fs::read_dir(root) else {
            continue;
        };
        let mut paths: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|x| x == "rs").unwrap_or(false))
            .collect();
        paths.sort();
        for p in paths {
            if let Some(s) = read_suite(&p) {
                suites.push(s);
            }
        }
    }
    if suites.is_empty() {
        return Err("no test suites found — wrong directory".into());
    }
    let plan = std::fs::read_to_string(plan_path).unwrap_or_default();

    let total: usize = suites.iter().map(|s| s.tests.len()).sum();
    let mut d = String::new();
    d.push_str("# Test plan\n\n");
    d.push_str("*Generated by `homelab testplan` — regenerate after adding a suite.*\n\n");
    d.push_str(
        "Phase 7 asks for a document describing every suite and every accepted\n\
         limitation. This one is derived from the tests rather than written beside\n\
         them, because a file a person keeps in step with reality drifts out of it\n\
         (Kenny, 2026-09-02). What each suite is FOR comes from its own header;\n\
         what it checks comes from the test names, which in this codebase are\n\
         sentences. A test that is deleted disappears from here in the same commit.\n\n",
    );
    d.push_str(&format!(
        "**{} tests across {} suites.**\n\n## Accepted limitations\n\n",
        total,
        suites.len()
    ));
    let acc = accepted_limitations(&plan);
    if acc.is_empty() {
        d.push_str("None: every gap the Phase-7 audit raised was closed.\n\n");
    } else {
        d.push_str(
            "Gaps somebody looked at and decided to leave, with the reason. Lifted\n\
             from the gap table in `REALIZATION_PLAN.md` so the two cannot disagree.\n\n",
        );
        for a in &acc {
            d.push_str(a);
            d.push('\n');
        }
        d.push('\n');
    }
    d.push_str("## Suites\n\n");
    for s in &suites {
        d.push_str(&format!("### `{}`\n\n", s.file));
        if !s.purpose.is_empty() {
            d.push_str(&format!("{}\n\n", s.purpose));
        }
        if !s.covers.is_empty() {
            d.push_str(&format!("Covers: {}\n\n", s.covers.join(", ")));
        }
        for (name, doc) in &s.tests {
            if doc.is_empty() {
                d.push_str(&format!("- `{}`\n", name));
            } else {
                d.push_str(&format!("- `{}` — {}\n", name, doc));
            }
        }
        d.push('\n');
    }
    std::fs::write(out, d).map_err(|e| format!("cannot write {}: {}", out.display(), e))?;
    Ok(suites.len())
}
