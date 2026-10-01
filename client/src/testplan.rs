//! Phase 7's output document, generated from the tests themselves.
//!
//! `PROCEDURE.md` asks Phase 7 to leave behind a `TEST_PLAN.md` describing
//! every suite and every accepted limitation. The orchestrator's was 482 lines
//! written by hand, and Kenny's rule of 2026-09-02 is that a file a human has
//! to keep in step with reality will drift, so this one is derived instead.
//!
//! What it reads: each test file's `//!` header for what the suite is for,
//! every `#[test]`/`#[tokio::test]` for what is actually checked, and the
//! `covers: F123` annotations that tie a test to the register finding it
//! exists for. The finding's own title comes from `REGISTER.md`, so an id is
//! never printed without saying what it was. What it cannot read, the
//! limitations somebody consciously accepted, comes from the gap table in
//! `REALIZATION_PLAN.md`, so there is one source of truth for those rather
//! than a second list to keep in step.
//!
//! The fixed prose below was rewritten at the Phase 8 gate (Kenny,
//! "Herschrijven"): every sentence in it describes what this file does, and
//! nothing else.

use std::collections::BTreeMap;
use std::path::Path;

struct Test {
    name: String,
    doc: String,
    covers: Vec<String>,
}

struct Suite {
    /// Path as printed: `<crate>/tests/<file>.rs`.
    file: String,
    /// File stem, which is also cargo's name for the test target.
    stem: String,
    purpose: String,
    tests: Vec<Test>,
}

struct Crate {
    package: String,
    tests_dir: String,
    suites: Vec<Suite>,
}

/// The first paragraph of a `//!` header: enough to say what a suite is for
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

/// `covers: F1, fix-2.` as a list of ids.
fn parse_covers(rest: &str) -> Vec<String> {
    rest.split(',')
        .map(|f| f.trim().trim_end_matches('.').to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

fn is_test_attr(t: &str) -> bool {
    t == "#[test]" || t.starts_with("#[tokio::test")
}

/// Walk up from a `fn` line over its attributes and comments. Returns whether
/// one of the attributes makes it a test, the first sentence of its doc
/// comment, and the `covers:` ids in that comment.
///
/// Only the block directly above the `fn` counts. The earlier version looked
/// six lines back for a test attribute, so a helper `fn` declared at the top of
/// a test's body was counted as a test of its own.
fn attributes_above(lines: &[&str], idx: usize) -> (bool, String, Vec<String>) {
    let mut is_test = false;
    let mut docs: Vec<String> = Vec::new();
    let mut covers = Vec::new();
    let mut i = idx;
    while i > 0 {
        i -= 1;
        let t = lines[i].trim();
        if t.starts_with("#[") {
            is_test |= is_test_attr(t);
            continue;
        }
        if let Some(d) = t.strip_prefix("///") {
            let d = d.trim();
            if let Some(rest) = d.strip_prefix("covers:") {
                covers.extend(parse_covers(rest));
            } else {
                docs.push(d.to_string());
            }
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        break;
    }
    docs.reverse();
    let joined = docs
        .iter()
        .map(|d| d.as_str())
        .filter(|d| !d.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let first = match joined.split_once(". ") {
        Some((first, _)) => format!("{}.", first),
        None => joined,
    };
    (is_test, first, covers)
}

fn read_suite(path: &Path) -> Option<Suite> {
    let src = std::fs::read_to_string(path).ok()?;
    let lines: Vec<&str> = src.lines().collect();
    let mut tests = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let t = l.trim();
        if !t.starts_with("fn ") && !t.starts_with("async fn ") && !t.starts_with("pub fn ") {
            continue;
        }
        let (is_test, doc, mut covers) = attributes_above(&lines, i);
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
        covers.sort();
        covers.dedup();
        tests.push(Test { name, doc, covers });
    }
    if tests.is_empty() {
        return None;
    }
    Some(Suite {
        // `<crate>/tests/<file>` from the path's own last three parts, so the
        // text is the same whichever directory the generator runs from.
        file: {
            let parts: Vec<String> = path
                .components()
                .rev()
                .take(3)
                .map(|c| c.as_os_str().to_string_lossy().to_string())
                .collect();
            parts.into_iter().rev().collect::<Vec<_>>().join("/")
        },
        stem: path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        purpose: header_purpose(&src),
        tests,
    })
}

/// The `[package] name` of the crate a tests directory belongs to, which is
/// what `cargo test -p` wants.
fn package_name(tests_dir: &Path) -> String {
    let manifest = tests_dir
        .parent()
        .map(|p| p.join("Cargo.toml"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let mut in_package = false;
    for line in manifest.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && let Some(v) = t.strip_prefix("name") {
            let v = v.trim_start().trim_start_matches('=').trim();
            return v.trim_matches('"').to_string();
        }
    }
    tests_dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// How many test functions live in a crate's own `src/` tree.
fn count_unit_tests(dir: &Path) -> usize {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            n += count_unit_tests(&p);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(src) = std::fs::read_to_string(&p)
        {
            n += src.lines().filter(|l| is_test_attr(l.trim())).count();
        }
    }
    n
}

/// The `members` of the workspace a tests directory sits in, read from the
/// workspace `Cargo.toml` two levels up (`<workspace>/<crate>/tests`).
fn workspace_members(tests_dir: &Path) -> (std::path::PathBuf, Vec<String>) {
    let ws = tests_dir
        .parent()
        .and_then(|c| c.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let manifest = std::fs::read_to_string(ws.join("Cargo.toml")).unwrap_or_default();
    let Some(at) = manifest.find("members") else {
        return (ws, Vec::new());
    };
    let rest = &manifest[at..];
    let (Some(open), Some(close)) = (rest.find('['), rest.find(']')) else {
        return (ws, Vec::new());
    };
    let members = rest[open + 1..close]
        .split(',')
        .map(|m| m.trim().trim_matches('"').to_string())
        .filter(|m| !m.is_empty())
        .collect();
    (ws, members)
}

/// One line per register id: the row's bold title when it has one, else its
/// first sentence. Rows are `| ID | text | ... |`.
fn register_titles(register: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in register.lines() {
        let Some(rest) = line.strip_prefix("| ") else {
            continue;
        };
        let Some((id, rest)) = rest.split_once(" | ") else {
            continue;
        };
        let id = id.trim();
        if id.is_empty() || id.contains(' ') || out.contains_key(id) {
            continue;
        }
        let cell = rest.split(" | ").next().unwrap_or("").trim();
        let title = if let Some(bold) = cell.strip_prefix("**") {
            bold.split("**").next().unwrap_or("").trim().to_string()
        } else {
            match cell.split_once(". ") {
                Some((first, _)) => first.to_string(),
                None => cell.to_string(),
            }
        };
        out.insert(id.to_string(), shorten(title.trim_end_matches('.'), 160));
    }
    out
}

/// Cut at a word boundary, so a long first sentence still reads as one line.
fn shorten(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return if s.ends_with(['?', '!']) {
            s.to_string()
        } else {
            format!("{}.", s)
        };
    }
    let mut out = String::new();
    for word in s.split_whitespace() {
        if out.chars().count() + word.chars().count() + 1 > max {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    format!("{} (...)", out.trim_end_matches([',', ';', ':']))
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
                "- **{}**: {}. Decision: {}",
                cells.get(1).unwrap_or(&""),
                cells.get(2).unwrap_or(&"").trim_end_matches('.'),
                cells.get(3).unwrap_or(&"")
            )
        })
        .collect()
}

/// Write the plan. Returns how many suites it described.
///
/// `stacks_dir` is read only to build the rule-public-docs address map (a
/// test's doc comment can quote a real fleet address, e.g. a 'node-exporter
/// on 10.10.10.13:9100' comment): every such address is replaced, by name
/// when it is one the stack files declare, by an RFC 5737 placeholder
/// otherwise, in the same way `generate_runbook` does it — see
/// `crate::netredact`.
pub fn generate_test_plan(
    roots: &[&Path],
    plan_path: &Path,
    stacks_dir: &Path,
    out: &Path,
) -> Result<usize, String> {
    let mut crates: Vec<Crate> = Vec::new();
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
        let suites: Vec<Suite> = paths.iter().filter_map(|p| read_suite(p)).collect();
        if suites.is_empty() {
            continue;
        }
        let crate_dir = root.parent().unwrap_or(root);
        crates.push(Crate {
            package: package_name(root),
            tests_dir: format!(
                "{}/{}",
                crate_dir
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
                root.file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default()
            ),
            suites,
        });
    }
    let suite_count: usize = crates.iter().map(|c| c.suites.len()).sum();
    if suite_count == 0 {
        return Err("no test suites found: wrong directory".into());
    }
    let plan = std::fs::read_to_string(plan_path).unwrap_or_default();
    let register = plan_path
        .parent()
        .map(|d| d.join("REGISTER.md"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let titles = register_titles(&register);
    let gloss = |id: &str| -> String {
        titles
            .get(id)
            .cloned()
            .unwrap_or_else(|| "not found in REGISTER.md.".to_string())
    };

    let total: usize = crates
        .iter()
        .flat_map(|c| c.suites.iter())
        .map(|s| s.tests.len())
        .sum();
    // Unit tests: every workspace crate's own `src/`, described nowhere else,
    // so at least their number is on record.
    let (ws, members) = roots
        .first()
        .map(|r| workspace_members(r))
        .unwrap_or_default();
    let unit_counts: Vec<(String, usize)> = members
        .iter()
        .map(|m| (m.clone(), count_unit_tests(&ws.join(m).join("src"))))
        .collect();
    let unit_total: usize = unit_counts.iter().map(|(_, n)| n).sum();

    let mut d = String::new();
    d.push_str("# Test plan\n\n");
    d.push_str(
        "*Generated by `homelab testplan` from the test sources. Do not edit by hand: \
         the next run overwrites it, and a test checks that the committed copy is current.*\n\n",
    );
    d.push_str(&format!(
        "**{} tests in {} integration-test suites.**\n\n",
        total, suite_count
    ));

    d.push_str("## How this document is made\n\n");
    d.push_str(
        "Every statement below is read out of a file, not written beside it:\n\n\
         - **What a suite is for** is the first paragraph of the `//!` comment at the top \
         of its file.\n\
         - **What a test checks** is its function name, which in this codebase is written \
         as a sentence, followed by the first sentence of its `///` comment when it has one.\n\
         - **Which finding a test pins** comes from a `/// covers: <id>` line above it. The \
         one-line description of each id is the title of that row in \
         `docs/deployment/REGISTER.md`.\n\
         - **Accepted limitations** are the rows of the gap table in \
         `docs/deployment/REALIZATION_PLAN.md` whose status is *later*.\n\n\
         A test that is deleted or renamed disappears from this document on the next run.\n\n",
    );

    d.push_str("## Scope\n\n");
    d.push_str(&format!(
        "Described here: every test in the integration-test directories {}.\n\n",
        crates
            .iter()
            .map(|c| format!("`{}`", c.tests_dir))
            .collect::<Vec<_>>()
            .join(" and ")
    ));
    if !unit_counts.is_empty() {
        d.push_str(&format!(
            "Counted but not described: the {} unit tests inside the workspace crates' own \
             `src/` trees ({}). They run with `cargo test --workspace` like the rest; this \
             document does not read them.\n\n",
            unit_total,
            unit_counts
                .iter()
                .map(|(m, n)| format!("`{}/src` {}", m, n))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    d.push_str("## Running the tests\n\n```sh\n");
    d.push_str("# the whole workspace, as the commit gate and make release run it\n");
    d.push_str("cargo test --workspace --locked\n");
    d.push_str("# one crate\n");
    for c in &crates {
        d.push_str(&format!("cargo test -p {}\n", c.package));
    }
    if let Some(c) = crates.first()
        && let Some(s) = c.suites.first()
    {
        d.push_str("# one suite, named after its file\n");
        d.push_str(&format!("cargo test -p {} --test {}\n", c.package, s.stem));
    }
    d.push_str("```\n\n");

    d.push_str("## Overview\n\n| Suite | Crate | Tests | Findings pinned |\n|---|---|---:|---:|\n");
    for c in &crates {
        for s in &c.suites {
            let mut ids: Vec<&String> = s.tests.iter().flat_map(|t| t.covers.iter()).collect();
            ids.sort();
            ids.dedup();
            d.push_str(&format!(
                "| [`{}`](#{}) | {} | {} | {} |\n",
                s.stem,
                s.stem,
                c.package,
                s.tests.len(),
                ids.len()
            ));
        }
    }
    d.push('\n');

    d.push_str("## Accepted limitations\n\n");
    let acc = accepted_limitations(&plan);
    if acc.is_empty() {
        d.push_str(
            "None: the gap table in `REALIZATION_PLAN.md` has no row whose status is \
             *later*.\n\n",
        );
    } else {
        d.push_str(
            "Gaps that were looked at and deliberately left open, as the gap table in \
             `REALIZATION_PLAN.md` records them.\n\n",
        );
        for a in &acc {
            d.push_str(a);
            d.push('\n');
        }
        d.push('\n');
    }

    for c in &crates {
        d.push_str(&format!(
            "## Suites in `{}` ({})\n\n",
            c.tests_dir, c.package
        ));
        for s in &c.suites {
            d.push_str(&format!("### {}\n\n", s.stem));
            d.push_str(&format!(
                "`{}` · {} test{} · `cargo test -p {} --test {}`\n\n",
                s.file,
                s.tests.len(),
                if s.tests.len() == 1 { "" } else { "s" },
                c.package,
                s.stem
            ));
            if s.purpose.is_empty() {
                d.push_str("*This file has no `//!` header, so its purpose is not stated.*\n\n");
            } else {
                d.push_str(&format!("{}\n\n", s.purpose));
            }
            let mut ids: Vec<&String> = s.tests.iter().flat_map(|t| t.covers.iter()).collect();
            ids.sort();
            ids.dedup();
            if !ids.is_empty() {
                d.push_str("Findings these tests pin:\n\n");
                for id in ids {
                    d.push_str(&format!("- **{}**: {}\n", id, gloss(id)));
                }
                d.push('\n');
            }
            d.push_str("Tests:\n\n");
            for t in &s.tests {
                let mut line = format!("- `{}`", t.name);
                if !t.doc.is_empty() {
                    line.push_str(&format!(": {}", t.doc));
                }
                if !t.covers.is_empty() {
                    line.push_str(&format!(" *(pins {})*", t.covers.join(", ")));
                }
                d.push_str(&line);
                d.push('\n');
            }
            d.push('\n');
        }
    }
    // rule-public-docs: see this function's doc comment.
    let client_host = crate::repo_config::load(stacks_dir)
        .ok()
        .flatten()
        .and_then(|(_, c)| c.host);
    let addr_map = crate::netredact::build_address_map(stacks_dir, client_host.as_deref());
    let d = crate::netredact::redact(&d, &addr_map);
    std::fs::write(out, d).map_err(|e| format!("cannot write {}: {}", out.display(), e))?;
    Ok(suite_count)
}
