//! `docs/INVARIANTS.md` claims a test for every row Kenny has stated as
//! "must always be so" — this file proves the claim the same way
//! `register_tests.rs` proves the register's `covers:` markers: by reading
//! the sources and refusing a citation that does not exist.
//!
//! A row's "Test(s)" column cites tests in one of two shapes, in backticks:
//!
//! * a bare identifier, `` `fn_name` `` — a Rust test, found as `fn
//!   fn_name(` anywhere in the workspace;
//! * `` `path/to/file.test.js: "the test's own description"` `` — a node
//!   `test(...)` in that file, found as that exact description string
//!   inside that exact file.
//!
//! Any other backticked span in that column (a file name on its own, a
//! macro, a wildcard such as `fix_171_*`) is not a citation and is
//! ignored — prose is free to use backticks for other code references,
//! same as everywhere else in this repository's docs. This means the
//! check is a floor, not a full index: it proves every row names at least
//! one test that is real, not that every test mentioned in a row's prose
//! exists.

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

/// One row's citation: either a Rust function name, or a JS file + the
/// exact description string of a `test(...)` in it.
#[derive(Debug, Clone)]
enum Citation {
    Rust(String),
    Js { file: String, description: String },
}

fn is_rust_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Every backticked span in a line, in order.
fn backticked_spans(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        out.push(after[..end].to_string());
        rest = &after[end + 1..];
    }
    out
}

/// A span recognised as one of the two citation shapes, or `None` when it
/// is some other backticked code reference this check does not index.
fn as_citation(span: &str) -> Option<Citation> {
    if is_rust_ident(span) {
        return Some(Citation::Rust(span.to_string()));
    }
    // `path.test.js: "description"` — the file ends .test.js (or .e2e.js,
    // this project's own Playwright-suite suffix), a colon, a space, and
    // the description in double quotes to the end of the span.
    let (file, rest) = span.split_once(": ")?;
    if !(file.ends_with(".test.js") || file.ends_with(".e2e.js")) {
        return None;
    }
    let description = rest.strip_prefix('"')?.strip_suffix('"')?;
    if description.is_empty() {
        return None;
    }
    Some(Citation::Js {
        file: file.to_string(),
        description: description.to_string(),
    })
}

/// (row number, test column text) for every data row of the invariants
/// table — a 4-column `| # | Invariant | Kenny said | Test(s) |` table,
/// the header and the `---` separator skipped.
fn invariant_rows(root: &Path) -> Vec<(String, String)> {
    let raw = std::fs::read_to_string(root.join("docs/INVARIANTS.md"))
        .expect("docs/INVARIANTS.md is part of the repo");
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        if cells.len() != 4 {
            continue;
        }
        let id = cells[0];
        if id.is_empty() || id == "#" || id.chars().all(|c| c == '-' || c == ':') {
            continue; // header or the `---|---|---|---` separator
        }
        if !id.chars().all(|c| c.is_ascii_digit()) {
            continue; // not a numbered row (defensive; should not happen)
        }
        out.push((id.to_string(), cells[3].to_string()));
    }
    out
}

#[test]
fn every_invariant_row_names_a_test_that_exists() {
    let root = repo_root();
    let rows = invariant_rows(&root);
    assert!(
        rows.len() >= 11,
        "the invariants table parser found only {} rows — the table shape \
         changed and this check would silently pass on almost nothing",
        rows.len()
    );

    let rust_sources = sources_with_ext(&root, "rs");
    let mut missing = Vec::new();

    for (id, test_col) in &rows {
        let spans = backticked_spans(test_col);
        let citations: Vec<Citation> = spans.iter().filter_map(|s| as_citation(s)).collect();
        if citations.is_empty() {
            missing.push(format!(
                "row {id}: no `fn_name` or `file.test.js: \"description\"` citation found in {:?}",
                test_col
            ));
            continue;
        }
        for c in citations {
            match c {
                Citation::Rust(name) => {
                    let needle = format!("fn {}(", name);
                    let found = rust_sources.iter().any(|(_, text)| text.contains(&needle));
                    if !found {
                        missing.push(format!("row {id}: no Rust test named `{name}` exists"));
                    }
                }
                Citation::Js { file, description } => {
                    let path = root.join(&file);
                    match std::fs::read_to_string(&path) {
                        Ok(text) => {
                            if !text.contains(description.as_str()) {
                                missing.push(format!(
                                    "row {id}: {file} exists but does not contain the test description {:?}",
                                    description
                                ));
                            }
                        }
                        Err(_) => missing.push(format!("row {id}: {file} does not exist")),
                    }
                }
            }
        }
    }

    assert!(
        missing.is_empty(),
        "docs/INVARIANTS.md names tests that do not back it up:\n{}",
        missing.join("\n")
    );
}

#[test]
fn as_citation_ignores_non_test_backticks() {
    assert!(matches!(
        as_citation("app_knowledge_no_declared_app_or_stack_is_named_in_the_code"),
        Some(Citation::Rust(_))
    ));
    assert!(matches!(
        as_citation(r#"admin/web/test/act.test.js: "a thing happens""#),
        Some(Citation::Js { .. })
    ));
    // A bare file name, a macro, a wildcard, a path: none of these are
    // citations this check can verify, so they must not be treated as one
    // (a false "exists" would be worse than silently skipping them).
    assert!(as_citation("core/tests/native_tests.rs").is_none());
    assert!(as_citation("debug_assert!").is_none());
    assert!(as_citation("fix_171_*").is_none());
    assert!(as_citation("core/src/runner.rs").is_none());
    assert!(as_citation("jobs.js percent()").is_none());
}
