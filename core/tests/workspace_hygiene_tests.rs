//! rust-code-hygiene (expert panel, 2026-09-27): guards for the workspace
//! itself. Each reads a manifest or a source tree, not a struct's layout.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(root().join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let p = entry.unwrap().path();
        if p.is_dir() {
            rust_sources(&p, out);
        } else if p.extension().is_some_and(|e| e == "rs") {
            out.push(p);
        }
    }
}

const MEMBERS: [&str; 4] = ["core", "proto", "host", "client"];

/// The operations' `step!` macro existed in eleven identical copies; one
/// lives in `ops/mod.rs` now. deploy.rs keeps its own, which marks the
/// stack incomplete and takes different arguments.
#[test]
fn the_step_macro_is_defined_once_for_the_operations() {
    let mut files = Vec::new();
    rust_sources(&root().join("core/src/ops"), &mut files);
    let defining: Vec<String> = files
        .iter()
        .filter(|p| !p.ends_with("deploy.rs"))
        .filter(|p| {
            std::fs::read_to_string(p)
                .unwrap()
                .contains("macro_rules! step {")
        })
        .map(|p| p.display().to_string())
        .collect();
    assert_eq!(defining.len(), 1, "{defining:#?}");
    assert!(defining[0].ends_with("ops/mod.rs"), "{defining:#?}");
}

/// `unsafe` is refused across the workspace, and every member takes its
/// lints from the workspace. `deny` rather than the panel's `forbid`: the
/// one block there is (fix-53, `libc::killpg`) opts out where it stands.
#[test]
fn unsafe_code_is_denied_across_the_workspace() {
    let ws: toml::Table = toml::from_str(&read("Cargo.toml")).unwrap();
    assert_eq!(
        ws.get("workspace")
            .and_then(|w| w.get("lints"))
            .and_then(|l| l.get("rust"))
            .and_then(|r| r.get("unsafe_code"))
            .and_then(|v| v.as_str()),
        Some("deny")
    );
    for m in MEMBERS {
        let t: toml::Table = toml::from_str(&read(&format!("{m}/Cargo.toml"))).unwrap();
        assert_eq!(
            t.get("lints")
                .and_then(|l| l.get("workspace"))
                .and_then(|v| v.as_bool()),
            Some(true),
            "{m} does not inherit the workspace lints"
        );
    }
}

/// A dependency a crate declares must be named somewhere in the code that
/// is built with it: a normal dependency in `src/`, a dev-dependency in
/// `src/` or `tests/`. `tokio-rustls` was a normal dependency of the client
/// that only its tests use, so it was built into every release binary.
#[test]
fn every_declared_dependency_is_used() {
    let code_in = |m: &str, dirs: &[&str]| {
        let mut code = String::new();
        for dir in dirs {
            let d = root().join(m).join(dir);
            if d.is_dir() {
                let mut files = Vec::new();
                rust_sources(&d, &mut files);
                for f in files {
                    code.push_str(&std::fs::read_to_string(f).unwrap());
                }
            }
        }
        code
    };
    let mut unused = Vec::new();
    for m in MEMBERS {
        let t: toml::Table = toml::from_str(&read(&format!("{m}/Cargo.toml"))).unwrap();
        for (section, dirs) in [
            ("dependencies", &["src"][..]),
            ("dev-dependencies", &["src", "tests"][..]),
        ] {
            let code = code_in(m, dirs);
            let Some(deps) = t.get(section).and_then(|d| d.as_table()) else {
                continue;
            };
            for name in deps.keys() {
                let ident = name.replace('-', "_");
                // A crate's dev-dependency on itself only switches a feature on.
                if ident == format!("homelab_{m}") {
                    continue;
                }
                if !code.contains(&format!("{ident}::")) && !code.contains(&format!("use {ident}"))
                {
                    unused.push(format!("{m}: {section}.{name}"));
                }
            }
        }
    }
    assert!(unused.is_empty(), "declared but never used: {unused:#?}");
}

/// Tests run in parallel threads of one process, so a test that sets an
/// environment variable changes it under every other test. The host's
/// config test did that with `HOMELAB_CONFIG`; `load_config_from` takes the
/// path instead.
#[test]
fn no_host_code_changes_the_process_environment() {
    let src = read("host/src/main.rs");
    // The positive twin: the file is read, and it still takes the config
    // path as a value rather than through the environment — so the negative
    // checks below are not passing merely because this read returned
    // nothing, or because the function they describe was renamed away.
    assert!(
        src.contains("load_config_from"),
        "host/src/main.rs: load_config_from not found"
    );
    assert!(
        !src.contains("env::set_var"),
        "host/src/main.rs sets a variable"
    );
    assert!(!src.contains("env::remove_var"));
}

/// tui-preview is a mock-up with no tests; building it on every run cost
/// its own dependency tree (rand, chrono). It stays in the repository,
/// outside the workspace.
#[test]
fn the_tui_mockup_is_not_a_workspace_member() {
    let ws: toml::Table = toml::from_str(&read("Cargo.toml")).unwrap();
    let members: Vec<&str> = ws["workspace"]["members"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    // The positive twin: the crates that ARE workspace members are still
    // listed — so this is a check of membership, not of an array that
    // happens to be empty.
    for m in MEMBERS {
        assert!(members.contains(&m), "{m} missing: {members:?}");
    }
    assert!(!members.contains(&"tui-preview"), "{members:?}");
}
