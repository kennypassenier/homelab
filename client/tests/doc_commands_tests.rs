//! A document that states behaviour is executed, not reviewed (dev procedure,
//! Phase 8). Every `homelab <verb>` a person can copy out of the documents
//! must name a verb the client actually dispatches; a renamed or planned-but-
//! never-built verb in a guide is how a runbook ships a command that answers
//! "unknown command" in the middle of a recovery.
//!
//! Checked at the level the client parses first: the verb. The arguments are
//! read per verb, positionally, so a second-level check would have to
//! re-implement each verb's parser here.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn is_verb(s: &str) -> bool {
    !s.is_empty()
        && s.starts_with(|c: char| c.is_ascii_lowercase())
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The verbs `main` dispatches: the arms of `match cmd {`, eight spaces in.
fn dispatched_verbs() -> BTreeSet<String> {
    let src = std::fs::read_to_string(root().join("client/src/main.rs")).unwrap();
    let body = &src[src.find("    match cmd {").expect("match cmd in main.rs")..];
    let mut verbs = BTreeSet::from(["help".to_string()]);
    for line in body.lines() {
        let Some(arm) = line.strip_prefix("        ") else {
            continue;
        };
        if !arm.starts_with('"') {
            continue;
        }
        let Some((pats, _)) = arm.split_once("=>") else {
            continue;
        };
        for p in pats.split('|') {
            let v = p.trim().trim_matches('"');
            if is_verb(v) {
                verbs.insert(v.to_string());
            }
        }
    }
    verbs
}

fn verb_after(s: &str) -> Option<String> {
    let v: String = s
        .chars()
        .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
        .collect();
    is_verb(&v).then_some(v)
}

/// `homelab <verb>` as a line in a code block, or inside an inline code span.
fn commands_in(text: &str) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut in_block = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if in_block {
            let mut rest = line.trim_start().trim_start_matches("$ ");
            // `HOMELAB_HOST=… homelab deploy` — skip leading assignments.
            while let Some((word, tail)) = rest.split_once(' ') {
                if word.contains('=') && !word.starts_with('-') {
                    rest = tail;
                } else {
                    break;
                }
            }
            let rest = rest.trim_start_matches("~/.cargo/bin/");
            if let Some(v) = rest.strip_prefix("homelab ").and_then(verb_after) {
                out.push((i + 1, v));
            }
        }
        for prefix in ["`homelab ", "`~/.cargo/bin/homelab "] {
            for (at, _) in line.match_indices(prefix) {
                if let Some(v) = verb_after(&line[at + prefix.len()..]) {
                    out.push((i + 1, v));
                }
            }
        }
    }
    out
}

fn documents() -> Vec<PathBuf> {
    let mut docs = vec![root().join("README.md")];
    for dir in ["docs", "docs/deployment"] {
        let mut here: Vec<PathBuf> = std::fs::read_dir(root().join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "md"))
            .collect();
        here.sort();
        docs.extend(here);
    }
    docs
}

#[test]
fn every_homelab_command_in_the_documents_names_a_verb_the_client_dispatches() {
    let verbs = dispatched_verbs();
    assert!(
        verbs.len() > 30,
        "reading main.rs found only {} verbs — the parser here is broken, not the docs",
        verbs.len()
    );
    let mut seen = 0;
    let mut unknown = Vec::new();
    for doc in documents() {
        let text = std::fs::read_to_string(&doc).unwrap();
        for (line, verb) in commands_in(&text) {
            seen += 1;
            if !verbs.contains(&verb) {
                let rel = doc.strip_prefix(root()).unwrap().display().to_string();
                unknown.push(format!("{rel}:{line}: homelab {verb}"));
            }
        }
    }
    assert!(seen > 100, "only {seen} commands found — extraction broke");
    assert!(
        unknown.is_empty(),
        "documents name verbs the client does not have:\n{}",
        unknown.join("\n")
    );
}

#[test]
fn the_extractor_finds_block_lines_inline_spans_and_skips_prose() {
    let text = "Run `homelab deploy stacks/x` first.\n\
                The homelab has three hosts.\n\
                ```bash\n\
                $ HOMELAB_HOST=a:1 ~/.cargo/bin/homelab status\n\
                ```\n";
    let got: Vec<String> = commands_in(text).into_iter().map(|(_, v)| v).collect();
    assert_eq!(got, vec!["deploy".to_string(), "status".to_string()]);
}
