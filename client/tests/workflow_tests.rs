//! The build and check chain is part of the supply chain of a binary that runs
//! as root on the hypervisor, so its security properties are asserted here
//! rather than trusted to review (expert panel 2026-09-27,
//! host-release-unsigned).
//!
//! Since 2026-09-28 (Kenny: "Lokaal bouwen en uploaden") the release is built
//! on the release machine by `make release`, and since 2026-09-29 (Kenny:
//! "alle builds lokaal") so is every check CI ran: GitHub Actions runs
//! nothing for this repository. The Makefile and the commit hook are asserted
//! below instead of the workflows.

use serde_yaml::Value;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn makefile() -> String {
    std::fs::read_to_string(root().join("Makefile")).unwrap()
}

/// The recipe lines of one Makefile target, up to the next blank line.
fn recipe(mk: &str, target: &str) -> String {
    let start = mk
        .find(&format!("\n{target}:"))
        .unwrap_or_else(|| panic!("Makefile has no `{target}` target"));
    mk[start + 1..].split("\n\n").next().unwrap().to_string()
}

#[test]
fn no_github_workflow_builds_or_checks_anything() {
    let dir = root().join(".github/workflows");
    let files: Vec<_> = std::fs::read_dir(&dir)
        .map(|d| {
            d.map(|e| e.unwrap().path())
                .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        files.is_empty(),
        "every build and check runs locally (Kenny, 2026-09-29); found {files:?}"
    );
}

#[test]
fn fix_140_the_release_machine_checks_advisories_secrets_and_the_msrv() {
    let mk = makefile();
    assert!(
        recipe(&mk, "check").starts_with("check: gate advisories secrets msrv"),
        "`make check` is everything CI ran"
    );
    // Dependabot alerts went unread for weeks (F48); a refused release is read.
    assert!(recipe(&mk, "advisories").contains("$(DENY) --locked check advisories"));
    // A found secret must not end up in a terminal log either.
    let secrets = recipe(&mk, "secrets");
    assert!(secrets.contains("$(GITLEAKS) git") && secrets.contains("--redact"));
    // The scanners are pinned release binaries, checked before first use.
    assert_eq!(mk.matches("sha256sum -c --quiet -").count(), 2);
    // The MSRV Cargo.toml promises, with that compiler.
    assert!(recipe(&mk, "msrv").contains("cargo +$$msrv check --workspace --locked"));
    // The release runs all three before DRY=1 stops and before anything is
    // tagged, and no longer asks GitHub for a verdict.
    let scan = mk.find("$(MAKE) advisories secrets msrv").unwrap();
    assert!(scan < mk.find("ifdef DRY").unwrap());
    assert!(scan < mk.find("git tag -a").unwrap());
    assert!(!mk.contains("check-runs"), "no CI verdict is read any more");
}

#[test]
fn the_commit_gate_scans_the_staged_changes_for_secrets_and_advisories() {
    let hook = std::fs::read_to_string(root().join(".githooks/pre-commit")).unwrap();
    assert!(hook.contains("make -s secrets-staged"));
    assert!(hook.contains("make -s advisories"));
    let staged = recipe(&makefile(), "secrets-staged");
    assert!(staged.contains("--staged") && staged.contains("--redact"));
}

#[test]
fn fix_140_dependabot_commits_carry_a_bracketed_id() {
    let text = std::fs::read_to_string(root().join(".github/dependabot.yml")).unwrap();
    let v: Value = serde_yaml::from_str(&text).unwrap();
    let updates = v.get("updates").and_then(Value::as_sequence).unwrap();
    assert!(!updates.is_empty());
    for u in updates {
        let eco = u.get("package-ecosystem").and_then(Value::as_str).unwrap();
        let prefix = u
            .get("commit-message")
            .and_then(|c| c.get("prefix"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        // The commit-msg hook's rule 4: an ID in brackets on every commit.
        assert!(
            prefix.contains("[meta]"),
            "{eco}: Dependabot's prefix `{prefix}` carries no bracketed ID"
        );
    }
}

/// release-build (Kenny, 2026-09-28): the release binaries are built locally,
/// in the Debian image the GitHub job used, from what Cargo.lock pins, the
/// dashboard in a build of its own, all three in SHA256SUMS, and published
/// only after the tag is pushed.
#[test]
fn release_build_is_local_in_the_debian_image_and_locked() {
    let mk = std::fs::read_to_string(root().join("Makefile")).unwrap();
    // 2026-09-29: the image is pinned to the Rust rust-toolchain.toml names
    // and the build uses that image's own install, so no download per build.
    let channel = std::fs::read_to_string(root().join("rust-toolchain.toml"))
        .unwrap()
        .lines()
        .find_map(|l| {
            l.strip_prefix("channel = \"")
                .map(|v| v.trim_end_matches('"').to_string())
        })
        .expect("rust-toolchain.toml names a channel");
    assert!(
        mk.contains(&format!("DEBIAN_IMAGE ?= rust:{channel}-bookworm")),
        "the Debian image carries the pinned Rust {channel}"
    );
    assert!(
        mk.contains("-e RUSTUP_TOOLCHAIN=$(IMAGE_TOOLCHAIN)"),
        "the build uses the image's own toolchain"
    );
    let builds: Vec<&str> = mk
        .lines()
        .filter(|l| l.contains("$(DOCKER_CARGO) build"))
        .collect();
    assert_eq!(
        builds.len(),
        2,
        "host+client, then the dashboard alone: {builds:?}"
    );
    assert!(
        builds
            .iter()
            .all(|l| l.contains("--locked") && l.contains("--release"))
    );
    assert!(builds[1].contains("-p homelab-admin") && !builds[0].contains("homelab-admin"));
    assert!(mk.contains("sha256sum homelab-host homelab homelab-admin > SHA256SUMS"));
    let push = mk.find("git push origin HEAD --follow-tags").unwrap();
    let publish = mk.find("gh release create").unwrap();
    let build = mk.find("$(MAKE) release-binaries").unwrap();
    assert!(
        build < push && push < publish,
        "build, then push, then publish"
    );
    assert!(
        !root().join(".github/workflows/release.yml").exists(),
        "no second builder"
    );
}
