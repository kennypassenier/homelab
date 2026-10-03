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

// ── fix-guards-3: the numbers are one sequence and every citation is current ──
//
// The rows are hand-numbered and several helper branches append to the table
// at once, each "after the current last number" (the coordinator renumbers
// on merge). Two faults came out of that by 2026-10-03, both silent: the
// table was split in two by a blank line between rows 17 and 18 (Markdown
// renders rows 18 onwards as a loose paragraph of pipes), and fix-239's
// register row cited "INVARIANTS row 38" for the rule that is row 39 —
// row 38 is fix-238's retention lanes. Nothing compared a number with
// anything. These checks do, on the text alone.

/// The number in a table row's first cell, if the line is a numbered row.
fn row_number(line: &str) -> Option<u32> {
    let rest = line.trim().strip_prefix('|')?;
    rest.split('|').next()?.trim().parse().ok()
}

/// Every fault in the table's numbering: a blank or foreign line inside the
/// table, a number used twice, or a number that is not the previous one
/// plus one. Pure, so a constructed bad table can prove it fails.
fn numbering_faults(doc: &str) -> Vec<String> {
    let lines: Vec<&str> = doc.lines().collect();
    let Some(header) = lines
        .iter()
        .position(|l| l.trim_start().starts_with("| # |"))
    else {
        return vec!["no `| # |` table header found".to_string()];
    };
    // The table ends at the last numbered row; everything between the header
    // (and its `---` separator) and that row has to be a numbered row.
    let last = lines
        .iter()
        .rposition(|l| row_number(l).is_some())
        .unwrap_or(header);
    let mut faults = Vec::new();
    let mut seen: std::collections::BTreeMap<u32, usize> = Default::default();
    let mut previous: Option<u32> = None;
    for (i, line) in lines.iter().enumerate().take(last + 1).skip(header + 2) {
        let Some(n) = row_number(line) else {
            faults.push(format!(
                "line {}: the table is broken here (a blank or non-row line between rows) — \
                 every row after it renders as plain text",
                i + 1
            ));
            continue;
        };
        if let Some(first) = seen.insert(n, i + 1) {
            faults.push(format!(
                "row {n} appears twice (lines {first} and {}) — two branches appended the same \
                 number; give the later one the next free number and move its citations with it",
                i + 1
            ));
        } else if let Some(p) = previous {
            if n != p + 1 {
                faults.push(format!(
                    "line {}: row {n} follows row {p} — the numbers must run 1, 2, 3 … with no gap",
                    i + 1
                ));
            }
        } else if n != 1 {
            faults.push(format!("the first row is {n}, not 1"));
        }
        previous = Some(n);
    }
    faults
}

/// Does `text` name register id `id` as a whole word (`fix-17` is not in
/// `fix-172`)?
fn names_id(text: &str, id: &str) -> bool {
    let part =
        |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    text.match_indices(id).any(|(at, _)| {
        !part(text[..at].chars().next_back()) && !part(text[at + id.len()..].chars().next())
    })
}

/// The invariant numbers a line cites: "invariant 14", "invariants 32, 36
/// and 40", "INVARIANTS row 38".
fn cited_rows(line: &str) -> Vec<u32> {
    let lower = line.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut rest = lower.as_str();
    while let Some(at) = rest.find("invariant") {
        let mut tail = &rest[at + "invariant".len()..];
        tail = tail.strip_prefix('s').unwrap_or(tail);
        tail = tail.strip_prefix(".md").unwrap_or(tail);
        let t = tail.trim_start();
        let mut list = t.strip_prefix("row").map(str::trim_start).unwrap_or(t);
        loop {
            let digits: String = list.chars().take_while(|c| c.is_ascii_digit()).collect();
            if digits.is_empty() {
                break;
            }
            out.push(digits.parse().unwrap_or(0));
            list = &list[digits.len()..];
            let next = list.trim_start();
            match next.strip_prefix(',').or_else(|| next.strip_prefix("and ")) {
                Some(more) => list = more.trim_start(),
                None => break,
            }
        }
        rest = &rest[at + "invariant".len()..];
    }
    out
}

/// A house-scheme register id: a lowercase kind (dashes allowed), a dash and
/// a number (`fix-239`, `feat-shell-1`).
fn is_register_id(s: &str) -> bool {
    s.rsplit_once('-').is_some_and(|(kind, n)| {
        !kind.is_empty()
            && kind.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            && !n.is_empty()
            && n.chars().all(|c| c.is_ascii_digit())
    })
}

/// The register id a line speaks for: the row's own id in a register table
/// (`| fix-239 | …`), or an id written right before the citation in code
/// (`// fix-239 / invariant 39`).
fn owner_of(line: &str) -> Option<String> {
    if let Some(rest) = line.strip_prefix("| ") {
        let id = rest.split(" |").next().unwrap_or("").trim();
        if is_register_id(id) {
            return Some(id.to_string());
        }
    }
    let at = line.to_ascii_lowercase().find("invariant")?;
    let before = line[..at].trim_end().strip_suffix('/')?.trim_end();
    let word = before
        .rsplit(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
        .next()?;
    is_register_id(word).then(|| word.to_string())
}

/// Every citation of an invariant row that points away from the row(s) that
/// name the citing id: when some row of the table names `fix-239`, a line
/// speaking for fix-239 may only cite one of those rows. A citation by an
/// id no row names (a row that merely obeys an invariant) is not judged.
fn stale_citations(invariants: &str, sources: &[(String, String)]) -> Vec<String> {
    let rows: Vec<(u32, &str)> = invariants
        .lines()
        .filter_map(|l| Some((row_number(l)?, l)))
        .collect();
    let mut out = Vec::new();
    for (name, text) in sources {
        for (i, line) in text.lines().enumerate() {
            let cited = cited_rows(line);
            if cited.is_empty() {
                continue;
            }
            let Some(owner) = owner_of(line) else {
                continue;
            };
            let naming: Vec<u32> = rows
                .iter()
                .filter(|(_, l)| names_id(l, &owner))
                .map(|(n, _)| *n)
                .collect();
            if naming.is_empty() {
                continue;
            }
            for n in cited {
                if !naming.contains(&n) {
                    out.push(format!(
                        "{name}:{}: {owner} cites invariant {n}, but the row(s) naming {owner} \
                         are {naming:?} — a renumber or a collision moved the row and left this \
                         citation behind",
                        i + 1
                    ));
                }
            }
        }
    }
    out
}

/// covers: fix-guards-3
#[test]
fn invariant_numbers_run_one_to_n_in_one_unbroken_table() {
    let doc = std::fs::read_to_string(repo_root().join("docs/INVARIANTS.md")).unwrap();
    let faults = numbering_faults(&doc);
    assert!(
        faults.is_empty(),
        "docs/INVARIANTS.md's numbering is broken:\n{}",
        faults.join("\n")
    );
}

#[test]
fn numbering_faults_catches_a_collision_a_gap_and_a_split_table() {
    let head = "| # | Invariant | Kenny said | Test(s) |\n|---|---|---|---|\n";
    let good = format!("{head}| 1 | a | b | `t` |\n| 2 | a | b | `t` |\n");
    assert!(numbering_faults(&good).is_empty());
    let collided = format!("{head}| 1 | a | b | `t` |\n| 2 | a | b | `t` |\n| 2 | c | d | `t` |\n");
    assert!(
        numbering_faults(&collided)
            .iter()
            .any(|f| f.contains("row 2 appears twice"))
    );
    let gap = format!("{head}| 1 | a | b | `t` |\n| 3 | a | b | `t` |\n");
    assert!(
        numbering_faults(&gap)
            .iter()
            .any(|f| f.contains("row 3 follows row 1"))
    );
    let split = format!("{head}| 1 | a | b | `t` |\n\n| 2 | a | b | `t` |\n");
    assert!(
        numbering_faults(&split)
            .iter()
            .any(|f| f.contains("table is broken"))
    );
}

/// covers: fix-guards-3
#[test]
fn every_citation_of_an_invariant_row_points_at_the_row_that_names_it() {
    let root = repo_root();
    let invariants = std::fs::read_to_string(root.join("docs/INVARIANTS.md")).unwrap();
    let mut sources: Vec<(String, String)> = [
        "docs/deployment/REGISTER.md",
        "docs/deployment/CORRECTIONS.md",
    ]
    .iter()
    .map(|p| {
        (
            p.to_string(),
            std::fs::read_to_string(root.join(p)).unwrap(),
        )
    })
    .collect();
    for ext in ["rs", "js"] {
        for (p, t) in sources_with_ext(&root, ext) {
            // This file's own fixtures are bad on purpose.
            if p.ends_with("invariants_doc_tests.rs") {
                continue;
            }
            let rel = p.strip_prefix(&root).unwrap_or(&p).display().to_string();
            sources.push((rel, t));
        }
    }
    let stale = stale_citations(&invariants, &sources);
    assert!(
        stale.is_empty(),
        "stale invariant citations:\n{}",
        stale.join("\n")
    );
}

#[test]
fn stale_citations_catches_the_fix_239_shape() {
    let inv =
        "| 38 | lanes | fix-238 | `t` |\n| 39 | Live view reaches every button | fix-239 | `t` |\n";
    let bad = vec![(
        "REGISTER.md".to_string(),
        "| fix-239 | x | the e2e case (INVARIANTS row 38) failed first | done |".to_string(),
    )];
    assert_eq!(stale_citations(inv, &bad).len(), 1);
    let code = vec![(
        "a.js".to_string(),
        "// fix-239 / invariant 38: …".to_string(),
    )];
    assert_eq!(stale_citations(inv, &code).len(), 1);
    let good = vec![(
        "REGISTER.md".to_string(),
        "| fix-239 | x | INVARIANTS row 39 | done |".to_string(),
    )];
    assert!(stale_citations(inv, &good).is_empty());
    // A row no invariant names (it obeys one, it did not create it) is free
    // to cite any row.
    let obeys = vec![(
        "R".to_string(),
        "| feat-x-1 | keeps invariant 38 | done |".to_string(),
    )];
    assert!(stale_citations(inv, &obeys).is_empty());
    assert!(!names_id("fix-172 and more", "fix-17"));
    assert_eq!(
        cited_rows("(invariants 32, 36, 40 and 41)"),
        vec![32, 36, 40, 41]
    );
}
