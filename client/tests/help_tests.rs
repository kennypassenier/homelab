//! help-flat-ids-typo-exit0 (expert panel, 2026-09-27): `homelab --help` was
//! one flat list of 38 verbs, nearly every line carrying an internal id with
//! no gloss, `export|import <file>` was wrong, `destroy --no-backup` and
//! `release-update [tag]` were missing, `homelab deploy --help` printed the
//! same list, and a mistyped verb (stauts) printed it too and exited 0.

use homelab_client::cli_help::{suggest, usage, verb_help, VERBS};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The verbs `main` dispatches: the arms of `match cmd {`, eight spaces in
/// (the same reading `doc_commands_tests` does).
fn dispatched() -> Vec<String> {
    let src = std::fs::read_to_string(root().join("client/src/main.rs")).unwrap();
    let body = &src[src.find("    match cmd {").unwrap()..];
    let mut out = Vec::new();
    for line in body.lines() {
        let Some(arm) = line.strip_prefix("        \"") else {
            continue;
        };
        let Some((pats, _)) = arm.split_once("=>") else {
            continue;
        };
        for p in pats.split('|') {
            let v = p.trim().trim_matches('"');
            if !v.is_empty() && v.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                out.push(v.to_string());
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// covers: fix-108
#[test]
fn fix_108_the_help_is_grouped_and_says_what_each_verb_takes() {
    let text = usage();
    for group in [
        "Daily",
        "Change a stack",
        "Native services",
        "Host and fleet",
        "Local, no host needed",
        "Rare and destructive",
    ] {
        assert!(text.contains(group), "no group {:?}:\n{}", group, text);
    }
    for right in [
        "homelab export stacks/<name> [out.yml]",
        "homelab import <bundle.yml> <new-name> <vmid>",
        "homelab destroy stacks/<name> [--no-backup]",
        "homelab release-update [tag]",
    ] {
        assert!(text.contains(right), "missing {:?}:\n{}", right, text);
    }
    // No bare internal ids: (E1), (D9/B6), (ask-8), (H2b; ...).
    for line in text.lines() {
        let ids = line.split('(').skip(1).any(|after| {
            let id: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect();
            let has_digit = id.chars().any(|c| c.is_ascii_digit());
            let has_upper = id.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            has_digit && (has_upper || id.starts_with("ask-") || id.starts_with("fix-"))
        });
        assert!(!ids, "an id without a gloss: {}", line);
    }
}

/// Every verb the client dispatches has its own help with an example, and
/// the table names no verb the client does not have.
/// covers: fix-108
#[test]
fn fix_108_every_verb_has_its_own_help_with_an_example() {
    let verbs = dispatched();
    assert!(verbs.len() > 30, "the scan broke: {:?}", verbs);
    for v in &verbs {
        let help = verb_help(v).unwrap_or_else(|| panic!("no help for {}", v));
        assert!(help.contains("example:"), "{}", help);
        assert!(help.contains(&format!("homelab {}", v)), "{}", help);
    }
    for v in VERBS {
        assert!(
            verbs.iter().any(|d| d == v.name) || v.name == "help",
            "the help names a verb main does not dispatch: {}",
            v.name
        );
    }
}

/// covers: fix-108
#[test]
fn fix_108_a_mistyped_verb_gets_a_suggestion() {
    assert_eq!(suggest("stauts"), Some("status"));
    assert_eq!(suggest("deplyo"), Some("deploy"));
    assert_eq!(suggest("chek"), Some("check"));
    assert_eq!(suggest("xyzzyplugh"), None);
}

/// A typo in a script or by hand looks like success when it exits 0. It
/// exits 2 and says what was meant; `<verb> --help` is that verb's help.
/// covers: fix-108
#[test]
fn fix_108_an_unknown_verb_exits_2_and_verb_help_exits_0() {
    let bin = env!("CARGO_BIN_EXE_homelab");
    let out = std::process::Command::new(bin)
        .arg("stauts")
        .env("HOMELAB_TOKEN", "offline")
        .env("HOMELAB_HOST", "127.0.0.1:1")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("did you mean 'status'"), "{}", err);

    let out = std::process::Command::new(bin)
        .args(["deploy", "--help"])
        .env_remove("HOMELAB_TOKEN")
        .env("HOME", std::env::temp_dir())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("homelab deploy"), "{}", text);
    assert!(text.contains("example:"), "{}", text);
}
