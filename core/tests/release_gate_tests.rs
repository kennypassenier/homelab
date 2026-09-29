//! The release gate runs once (Kenny, 2026-09-29: "drie keer dezelfde
//! testrun is dom, dat moet naar één"). `.githooks/gate-stamp` hands the
//! shared helper from the workstation repository this project's toolchain:
//! `make release` skips a gate that already passed on the same tree
//! (`fresh`), and the pre-commit hook skips the version-bump commit
//! (`version-only`). Without the helper, both must fail closed.

use std::path::{Path, PathBuf};
use std::process::Command;

const GIT_VARS: [&str; 6] = [
    "GIT_DIR",
    "GIT_INDEX_FILE",
    "GIT_WORK_TREE",
    "GIT_PREFIX",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
];

fn wrapper() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".githooks/gate-stamp")
}

/// The helper the wrapper would find on this machine, if any. CI has none.
fn helper() -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var("HOME").ok()?);
    let candidates = [
        std::env::var("GATE_STAMP").ok().map(PathBuf::from),
        Some(home.join("Projects/workstation/bin/gate-stamp")),
        Some(home.join("Projects/.wt/workstation/bin/gate-stamp")),
    ];
    candidates.into_iter().flatten().find(|p| p.is_file())
}

/// A git command in `dir` that cannot reach the repository running the
/// tests: `git commit` exports GIT_DIR and friends into its hooks.
fn git(dir: &Path, args: &[&str]) -> String {
    let mut c = Command::new("git");
    c.current_dir(dir).args([
        "-c",
        "user.name=t",
        "-c",
        "user.email=t@t",
        "-c",
        "commit.gpgsign=false",
        "-c",
        "core.hooksPath=/dev/null",
    ]);
    for v in GIT_VARS {
        c.env_remove(v);
    }
    let out = c.args(args).output().unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

/// Run the wrapper in `dir` with a fixed toolchain line; returns the exit code.
fn stamp(dir: &Path, args: &[&str], toolchain: &str, env: &[(&str, &str)]) -> i32 {
    let mut c = Command::new("bash");
    c.arg(wrapper())
        .args(args)
        .current_dir(dir)
        .env("GATE_STAMP_TOOLCHAIN", toolchain);
    for v in GIT_VARS {
        c.env_remove(v);
    }
    for (k, v) in env {
        c.env(k, v);
    }
    c.output().unwrap().status.code().unwrap_or(-1)
}

const TOML_VERSION: &str = r#"Cargo.toml:^version = "[0-9]+\.[0-9]+\.[0-9]+"$"#;
const LOCK_VERSION: &str = r#"Cargo.lock:^version = "[0-9]+\.[0-9]+\.[0-9]+"$"#;

fn version_only(dir: &Path) -> i32 {
    stamp(dir, &["version-only", TOML_VERSION, LOCK_VERSION], "t", &[])
}

fn toml(v: &str) -> String {
    format!(
        "[workspace]\nmembers = [\"core\", \"host\"]\n\n[workspace.package]\nversion = \"{v}\"\nrust-version = \"1.97\"\n\n[workspace.dependencies]\nserde = {{ version = \"1\" }}\n"
    )
}

fn lock(v: &str, serde: &str) -> String {
    format!(
        "# generated\nversion = 4\n\n[[package]]\nname = \"homelab-core\"\nversion = \"{v}\"\ndependencies = [\n \"serde\",\n]\n\n[[package]]\nname = \"homelab-host\"\nversion = \"{v}\"\ndependencies = [\n \"homelab-core\",\n]\n\n[[package]]\nname = \"serde\"\nversion = \"{serde}\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\nchecksum = \"sum-{serde}\"\n"
    )
}

/// A repository at release 1.2.3, as `make release` finds it.
fn repo() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().unwrap();
    let d = tmp.path();
    git(d, &["init", "-q", "-b", "main"]);
    std::fs::write(d.join("Cargo.toml"), toml("1.2.3")).unwrap();
    std::fs::write(d.join("Cargo.lock"), lock("1.2.3", "1.0.0")).unwrap();
    std::fs::write(d.join("lib.rs"), "fn main() {}\n").unwrap();
    git(d, &["add", "-A"]);
    git(d, &["commit", "-q", "-m", "base"]);
    tmp
}

/// With no helper on the machine every question answers "run the gate".
#[test]
fn without_the_shared_helper_the_gate_always_runs() {
    let tmp = repo();
    let d = tmp.path();
    let home = tempfile::tempdir().unwrap();
    let env = [
        ("GATE_STAMP", "/nonexistent/gate-stamp"),
        ("HOME", home.path().to_str().unwrap()),
    ];
    std::fs::write(d.join("Cargo.toml"), toml("1.2.4")).unwrap();
    std::fs::write(d.join("Cargo.lock"), lock("1.2.4", "1.0.0")).unwrap();
    git(d, &["add", "-A"]);
    let args = ["version-only", TOML_VERSION, LOCK_VERSION];
    assert_eq!(
        stamp(d, &args, "t", &env),
        1,
        "no helper, no skipped commit"
    );
    assert_eq!(
        stamp(d, &["fresh"], "t", &env),
        1,
        "no helper, no skipped gate"
    );
}

/// The release's own bump (sed on Cargo.toml, cargo update on the lock)
/// skips the gates; anything else runs them: another file, another
/// Cargo.toml line, a registry crate bumped along, a version that is not
/// x.y.z, or nothing staged.
#[test]
fn only_the_release_version_bump_skips_the_commit_gates() {
    let Some(h) = helper() else {
        eprintln!("shared gate-stamp helper not on this machine; covered by the fail-closed test");
        return;
    };
    eprintln!("helper: {}", h.display());

    let tmp = repo();
    let d = tmp.path();
    assert_eq!(version_only(d), 1, "nothing staged is not a version bump");
    std::fs::write(d.join("Cargo.toml"), toml("1.2.4")).unwrap();
    std::fs::write(d.join("Cargo.lock"), lock("1.2.4", "1.0.0")).unwrap();
    git(d, &["add", "-A"]);
    assert_eq!(
        version_only(d),
        0,
        "the release's bump commit skips the gates"
    );

    let cases: [(&str, String, String, bool); 4] = [
        ("another file", toml("1.2.4"), lock("1.2.4", "1.0.0"), true),
        (
            "a dependency line in Cargo.toml",
            toml("1.2.4").replace("version = \"1\"", "version = \"2\""),
            lock("1.2.4", "1.0.0"),
            false,
        ),
        (
            "a registry crate in the lock",
            toml("1.2.4"),
            lock("1.2.4", "1.0.1"),
            false,
        ),
        (
            "a version that is not x.y.z",
            toml("1.2.4-rc1"),
            lock("1.2.4-rc1", "1.0.0"),
            false,
        ),
    ];
    for (what, t, l, other_file) in cases {
        let tmp = repo();
        let d = tmp.path();
        std::fs::write(d.join("Cargo.toml"), t).unwrap();
        std::fs::write(d.join("Cargo.lock"), l).unwrap();
        if other_file {
            std::fs::write(d.join("lib.rs"), "fn main() { panic!() }\n").unwrap();
        }
        git(d, &["add", "-A"]);
        assert_eq!(version_only(d), 1, "{what} must run the gates");
    }
}

/// `make release` skips its gate only for a clean checkout whose HEAD tree
/// and toolchain equal the stamp.
#[test]
fn make_release_skips_the_gate_only_for_the_stamped_tree_and_toolchain() {
    if helper().is_none() {
        eprintln!("shared gate-stamp helper not on this machine; covered by the fail-closed test");
        return;
    }
    let tmp = repo();
    let d = tmp.path();
    let tc = "rustc 1.97.0 node v22";

    assert_eq!(stamp(d, &["fresh"], tc, &[]), 1, "no stamp, run the gate");
    assert_eq!(stamp(d, &["record"], tc, &[]), 0);
    assert_eq!(
        stamp(d, &["fresh"], tc, &[]),
        0,
        "same tree, same toolchain"
    );
    assert_eq!(
        stamp(d, &["fresh"], "rustc 1.98.0 node v22", &[]),
        1,
        "another compiler, run the gate"
    );

    std::fs::write(d.join("lib.rs"), "fn main() { panic!() }\n").unwrap();
    assert_eq!(
        stamp(d, &["fresh"], tc, &[]),
        1,
        "a dirty checkout, run the gate"
    );
    git(d, &["commit", "-qam", "next"]);
    assert_eq!(
        stamp(d, &["fresh"], tc, &[]),
        1,
        "another tree, run the gate"
    );
}
