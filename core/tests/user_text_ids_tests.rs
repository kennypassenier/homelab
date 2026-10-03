//! fix-guards-8: text the orchestrator shows a person never carries a bare
//! register id.
//!
//! Kenny, 2026-10-03: "je zou je gedragen als een world-class developer,
//! maar ik zie vaak amateuristische fouten, die moeten eruit". The help
//! once listed `(E1)`, `(D9/B6)`, `(ask-8)` (fix-108); on 2026-10-03 this
//! check found 28 more: the Settings page described four host.toml keys
//! with "(fix-170)", "(fix-184)", "(fix-191)" and "gap-26:", a plan dialog
//! said "(fix-28)", a scheduler notice "(H8, fix-59)", restore refusals
//! "older than fix-112" and "before fix-64", and the generated disaster
//! runbook carried eight. A register id means something in
//! REGISTER.md and nothing to the person reading the page, the CLI or the
//! Host log; it belongs in comments and commit messages.
//!
//! What is judged: every string literal in the non-test Rust sources of
//! core, host, client and admin (comments, `#[cfg(test)]` modules and
//! `tests/` skipped) that reads as prose — it holds whitespace. An id
//! inside `[brackets]` is a commit trailer, written for git, and is
//! allowed; so is a line marked `// id-ok: <why>` (a commit subject built
//! in pieces, a test fixture's name).
//!
//! The browser half of the same rule is `admin/web/test/user_text.test.js`.

mod common;

use common::repo_root;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if name.starts_with("target") || name == "tests" || name == "benches" {
                continue;
            }
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// (line number, literal) for every string literal in `src` outside
/// comments and outside a `#[cfg(test)]` item. Handles escapes, raw
/// strings (`r#"…"#`), byte strings and char literals well enough for this
/// repository's sources.
fn string_literals(src: &str) -> Vec<(usize, String)> {
    let b = src.as_bytes();
    let n = b.len();
    let mut out = Vec::new();
    let mut i = 0;
    let mut line = 1;
    // A `#[cfg(test)]` item (a test module, a test-only fn) is skipped to
    // the brace that closes it; its strings are fixtures, not text.
    let mut depth: i32 = 0;
    let mut pending_test_item = false;
    let mut skip_from: Option<i32> = None;
    let keep = |out: &mut Vec<(usize, String)>, line: usize, text: String, skipping: bool| {
        if !skipping {
            out.push((line, text));
        }
    };
    while i < n {
        let c = b[i];
        let skipping = pending_test_item || skip_from.is_some();
        if src[i..].starts_with("#[cfg(test)]") {
            pending_test_item = skip_from.is_none();
            i += "#[cfg(test)]".len();
            continue;
        }
        if c == b'{' {
            if pending_test_item {
                skip_from = Some(depth);
                pending_test_item = false;
            }
            depth += 1;
            i += 1;
            continue;
        }
        if c == b'}' {
            depth -= 1;
            if skip_from == Some(depth) {
                skip_from = None;
            }
            i += 1;
            continue;
        }
        if c == b';' && pending_test_item {
            pending_test_item = false;
        }
        if c == b'\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'/' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if c == b'/' && i + 1 < n && b[i + 1] == b'*' {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if b[i] == b'\n' {
                    line += 1;
                }
                if b[i] == b'/' && i + 1 < n && b[i + 1] == b'*' {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && i + 1 < n && b[i + 1] == b'/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            continue;
        }
        // Raw string: r"…", r#"…"#, br#"…"#.
        let raw_start = if c == b'r' {
            Some(i + 1)
        } else if c == b'b' && i + 1 < n && b[i + 1] == b'r' {
            Some(i + 2)
        } else {
            None
        };
        let ident_before = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if let (Some(mut j), false) = (raw_start, ident_before) {
            let mut hashes = 0;
            while j < n && b[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < n && b[j] == b'"' {
                let close: Vec<u8> = std::iter::once(b'"')
                    .chain(std::iter::repeat_n(b'#', hashes))
                    .collect();
                let start = j + 1;
                let mut k = start;
                while k + close.len() <= n && b[k..k + close.len()] != close[..] {
                    k += 1;
                }
                let text = &src[start..k.min(n)];
                // Reported at its LAST line, like a plain literal, so an
                // `id-ok:` marker after the closing quote is found.
                line += text.matches('\n').count();
                keep(&mut out, line, text.to_string(), skipping);
                i = k + close.len();
                continue;
            }
        }
        if c == b'"' {
            let start = i + 1;
            let mut k = start;
            let mut text = String::new();
            while k < n && b[k] != b'"' {
                if b[k] == b'\\' && k + 1 < n {
                    if b[k + 1] == b'\n' {
                        line += 1;
                    }
                    k += 2;
                    continue;
                }
                if b[k] == b'\n' {
                    line += 1;
                }
                k += 1;
            }
            text.push_str(&src[start..k.min(n)]);
            keep(&mut out, line, text, skipping);
            i = k + 1;
            continue;
        }
        if c == b'\'' {
            // A char literal ('x', '\n', '\'') — or a lifetime ('a), which
            // has no closing quote right after it.
            if i + 2 < n && b[i + 1] == b'\\' {
                let mut k = i + 2;
                while k < n && b[k] != b'\'' {
                    k += 1;
                }
                i = k + 1;
                continue;
            }
            let ch_len = src[i + 1..].chars().next().map_or(1, char::len_utf8);
            if i + 1 + ch_len < n && b[i + 1 + ch_len] == b'\'' {
                i += 2 + ch_len;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The kinds of a register id, from the one list the commit gate and the
/// dashboard's check read too: (lowercase kinds, uppercase prefixes).
fn kinds() -> (Vec<String>, Vec<String>) {
    let text = std::fs::read_to_string(repo_root().join(".githooks/register-id-kinds.txt"))
        .expect(".githooks/register-id-kinds.txt lists the id kinds");
    let mut lower = Vec::new();
    let mut upper = Vec::new();
    for line in text.lines() {
        let k = line.split('#').next().unwrap_or("").trim();
        if k.is_empty() {
            continue;
        }
        if k.chars().all(|c| c.is_ascii_uppercase()) {
            upper.push(k.to_string());
        } else {
            lower.push(k.to_string());
        }
    }
    (lower, upper)
}

/// The register id `text` shows a person, if any: `fix-240`,
/// `feat-shell-1`, `gap-26`, `H8`, `D9` outside `[brackets]`. A
/// version-like tail (`redesign-3.71`) is not an id. Every line of the
/// literal is judged, one starting with `#` or `//` too: a literal is text
/// some reader gets (a runbook heading is read by a person), and a source
/// comment is not a literal. A generated file's own comment that must
/// carry an id is marked `// id-ok: <why>` at the literal.
fn bare_id(text: &str) -> Option<String> {
    if !text.trim().contains(char::is_whitespace) {
        return None; // a key or an identifier, not prose
    }
    let text = text.replace("\\n", "\n");
    let mut plain = String::new();
    let mut depth = 0;
    for c in text.chars() {
        match c {
            '[' => depth += 1,
            ']' if depth > 0 => depth -= 1,
            _ if depth == 0 => plain.push(c),
            _ => {}
        }
    }
    let (lower, upper) = kinds();
    let words: Vec<&str> = plain
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_'))
        .collect();
    for w in words {
        let w = w.trim_end_matches('.');
        if upper.iter().any(|p| {
            w.strip_prefix(p.as_str())
                .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        }) {
            return Some(w.to_string());
        }
        let Some((kind, rest)) = w.split_once('-') else {
            continue;
        };
        if !lower.iter().any(|k| k == kind) {
            continue;
        }
        let Some((_, num)) = rest.rsplit_once('-').or(Some(("", rest))) else {
            continue;
        };
        let domain_ok = rest.rsplit_once('-').is_none_or(|(d, _)| {
            d.split('-')
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_lowercase()))
        });
        if domain_ok && !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            return Some(w.to_string());
        }
    }
    None
}

/// covers: fix-guards-8
#[test]
fn no_text_a_person_reads_names_a_register_id() {
    let root = repo_root();
    let mut files = Vec::new();
    for dir in [
        "core/src",
        "host/src",
        "client/src",
        "admin/src",
        "proto/src",
    ] {
        walk(&root.join(dir), &mut files);
    }
    let mut found = Vec::new();
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        let lines: Vec<&str> = src.lines().collect();
        for (line, text) in string_literals(&src) {
            let Some(id) = bare_id(&text) else {
                continue;
            };
            // A literal spanning lines is reported at its last line; the
            // marker may sit on any line it covers.
            let first = line.saturating_sub(text.matches('\n').count());
            let marked = (first..=line).any(|l| {
                lines
                    .get(l.wrapping_sub(1))
                    .is_some_and(|s| s.contains("id-ok:"))
            });
            if marked {
                continue;
            }
            let rel = f.strip_prefix(&root).unwrap_or(&f).display().to_string();
            let shown: String = text.chars().take(90).collect();
            found.push(format!("{rel}:{line}: {id} in {shown:?}"));
        }
    }
    assert!(
        found.is_empty(),
        "a person would read these register ids — say what the thing does instead, \
         and keep the id in a comment:\n{}",
        found.join("\n")
    );
}

#[test]
fn bare_id_and_the_lexer_judge_what_a_person_reads() {
    let src = r###"
// a comment naming fix-240 is fine
/* so is feat-shell-1 */
fn a() -> &'static str { "outside the night window (fix-129)" }
fn b() -> String { format!("chore(host): {} [fix-110]\n", x) }
fn c() -> &'static str { "feat-stacks-3" }
fn d() -> &'static str { r#"gap-26: journal.jsonl is cut back"# }
fn e() -> char { '"' }
fn f() -> &'static str { "see redesign-3.71/backups.html for it" }
fn g() -> &'static str { "# Written by homelab (fix-88): edit the stack\nport = 1" }
fn h() -> &'static str { "## Restore a stack (gap-28)\n\nRun it" }
fn i() -> &'static str { "backups continue (H8): investigate" }
fn j() -> &'static str { "see E1 and D9/B6 for why" }
fn k() -> &'static str { "sha-256 and utf-8 are not ids, nor is v3.70.2 or KEY_B64" }
#[cfg(test)]
mod tests { fn t() { let _ = "fix-1 in a test is fine"; } }
"###;
    let judged: Vec<String> = string_literals(src)
        .into_iter()
        .filter_map(|(_, t)| bare_id(&t))
        .collect();
    // A `#` or `//` line inside a literal is text some reader gets (the
    // runbook's headings are read by a person); an uppercase id is an id.
    assert_eq!(
        judged,
        vec!["fix-129", "gap-26", "fix-88", "gap-28", "H8", "E1"]
    );
}
