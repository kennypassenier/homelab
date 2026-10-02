//! Selection logic for the carried gate rerun (`.githooks/gate-carry.sh`,
//! `.githooks/gate-carry-meta.py`).
//!
//! `make gate` used to run the whole suite again on every tree, even a
//! fix commit that touched one crate — measured 2026-10-01: the full
//! suite ran four times for one release. These tests exercise the three
//! rules that decide "rerun everything" vs "rerun just this": a global
//! trigger (the lockfile, the toolchain, or the gate's own definition),
//! mapping a failing test's binary back to its package, and the
//! foundational-crate fallback (a crate depended on by most of the
//! workspace counts as "changed everywhere"). The full carry/full
//! decision loop (`cmd_rust`/`cmd_node`) was exercised by hand against a
//! throwaway two-crate fixture workspace during development, driving real
//! `cargo test` runs; it is not repeated here because that needs its own
//! compiled fixture crates, which would cost more than the gate it is
//! meant to save.
//!
//! covers: fix-187

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn run_bash(snippet: &str, arg: &str) -> String {
    let out = Command::new("bash")
        .arg("-c")
        .arg(snippet)
        .arg("bash") // becomes $0 inside the snippet
        .arg(arg) // becomes $1 inside the snippet
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "gate-carry.sh snippet exited {:?}, stderr: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// covers: fix-187
#[test]
fn a_global_trigger_fires_on_the_lockfile_the_toolchain_and_the_gate_itself() {
    let snippet = r#"
        source .githooks/gate-carry.sh
        gate_global_trigger "$1" >/dev/null && echo TRIGGERED || echo CLEAR
    "#;
    let cases = [
        ("Cargo.lock", "TRIGGERED"),
        ("Cargo.toml", "TRIGGERED"),
        ("rust-toolchain.toml", "TRIGGERED"),
        ("Makefile", "TRIGGERED"),
        (".githooks/gate-cache.sh", "TRIGGERED"),
        (".githooks/gate-carry.sh", "TRIGGERED"),
        (".claude/hooks/gates.sh", "TRIGGERED"),
        // A crate's own Cargo.toml is not the workspace manifest and must
        // not force a full run by itself — only the root one does (a
        // crate-local change is scoped by the crate-dir match instead).
        ("core/Cargo.toml", "CLEAR"),
        ("core/src/lib.rs", "CLEAR"),
        ("docs/USER_GUIDE.md", "CLEAR"),
    ];
    for (changed, want) in cases {
        let got = run_bash(snippet, changed);
        assert_eq!(got, want, "changed={changed}");
    }
}

/// covers: fix-187
#[test]
fn rust_parse_failures_maps_a_failing_binary_back_to_its_package() {
    let tmp = tempfile::tempdir().unwrap();
    let stemmap = tmp.path().join("stem-map.tsv");
    std::fs::write(
        &stemmap,
        "homelab_proto\thomelab-proto\nscope_tests\thomelab-proto\n",
    )
    .unwrap();
    let log = tmp.path().join("run.log");
    std::fs::write(
        &log,
        "\
     Running unittests src/lib.rs (target/debug/deps/homelab_proto-627d4c5fcab764bc)

running 1 test
test wire_tests::ok_one ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/scope_tests.rs (target/debug/deps/scope_tests-4e2484ca41178c3a)

running 2 tests
test arch_tokens_a_command_name_is_its_wire_name ... ok
test feat_platform_1_a_text_request_carries_no_json_key ... FAILED

failures:

---- feat_platform_1_a_text_request_carries_no_json_key stdout ----
assertion failed

failures:
    feat_platform_1_a_text_request_carries_no_json_key

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
",
    )
    .unwrap();

    let out = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "source .githooks/gate-carry.sh && rust_parse_failures '{}' '{}'",
            log.display(),
            stemmap.display()
        ))
        .current_dir(repo_root())
        .output()
        .unwrap();
    let got = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        got.trim(),
        "homelab-proto\tfeat_platform_1_a_text_request_carries_no_json_key",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The "failures:" summary repeats the same name once more; a naive
    // line-match would double-count it.
    assert_eq!(got.trim().lines().count(), 1);
}

/// covers: fix-187
#[test]
fn node_parse_failures_reads_tap_not_ok_lines() {
    let tmp = tempfile::tempdir().unwrap();
    let tap = tmp.path().join("node.tap");
    std::fs::write(
        &tap,
        "TAP version 13\n\
         # Subtest: edit routes\n\
         \u{20}\u{20}\u{20}\u{20}ok 1 - rejects a path outside the repo\n\
         \u{20}\u{20}\u{20}\u{20}not ok 2 - accepts a relative path\n\
         not ok 1 - edit routes\n\
         # pass 1\n\
         # fail 1\n",
    )
    .unwrap();

    let out = Command::new("bash")
        .arg("-c")
        .arg(format!(
            "source .githooks/gate-carry.sh && node_parse_failures '{}'",
            tap.display()
        ))
        .current_dir(repo_root())
        .output()
        .unwrap();
    let got = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = got.trim().lines().collect();
    assert_eq!(lines, vec!["accepts a relative path", "edit routes"]);
}

/// covers: fix-187
#[test]
fn foundational_names_core_and_proto_which_the_rest_of_the_workspace_depends_on() {
    // Real `cargo metadata` over this actual workspace, not a fixture: core
    // and proto are depended on (directly or transitively) by every other
    // member, host/client/admin are leaves. If a sixth crate is ever added
    // between core and the leaves, this is expected to need re-checking —
    // that is the point: the fallback follows the real dependency graph,
    // not a hand-kept list (feedback_homelab_no_app_knowledge_in_code).
    let out = Command::new("python3")
        .arg(".githooks/gate-carry-meta.py")
        .arg("foundational")
        .args([
            "homelab-core",
            "homelab-proto",
            "homelab-host",
            "homelab-client",
            "homelab-admin",
        ])
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = String::from_utf8_lossy(&out.stdout);
    let mut names: Vec<&str> = got.lines().map(|l| l.split('\t').next().unwrap()).collect();
    names.sort_unstable();
    assert_eq!(names, vec!["homelab-core", "homelab-proto"]);
}

/// covers: fix-187
#[test]
fn dir_map_covers_every_workspace_member() {
    let out = Command::new("python3")
        .arg(".githooks/gate-carry-meta.py")
        .arg("dir-map")
        .current_dir(repo_root())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got = String::from_utf8_lossy(&out.stdout);
    for (dir, pkg) in [
        ("core", "homelab-core"),
        ("proto", "homelab-proto"),
        ("host", "homelab-host"),
        ("client", "homelab-client"),
        ("admin", "homelab-admin"),
    ] {
        assert!(
            got.lines().any(|l| l == format!("{dir}\t{pkg}")),
            "dir-map missing {dir} -> {pkg}, got:\n{got}"
        );
    }
}
