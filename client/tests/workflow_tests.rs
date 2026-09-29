//! The GitHub workflows are part of the supply chain of a binary that runs as
//! root on the hypervisor, so their security properties are asserted here
//! rather than trusted to review (expert panel 2026-09-27,
//! host-release-unsigned). The files are parsed, not grepped, so a property
//! moved to another key or job is still found.
//!
//! Since 2026-09-28 (Kenny: "Lokaal bouwen en uploaden") the release is built
//! on the release machine by `make release`, not by a GitHub job; the tests
//! for that job went with it and the Makefile's release path is asserted
//! below instead.

use serde_yaml::{Mapping, Value};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn workflow_text(name: &str) -> String {
    std::fs::read_to_string(root().join(".github/workflows").join(name)).unwrap()
}

fn workflow(name: &str) -> Value {
    serde_yaml::from_str(&workflow_text(name)).unwrap()
}

fn steps(job: &Value) -> Vec<&Value> {
    job.get("steps")
        .and_then(Value::as_sequence)
        .map(|s| s.iter().collect())
        .unwrap_or_default()
}

fn run_text(job: &Value) -> String {
    steps(job)
        .iter()
        .filter_map(|s| s.get("run").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

/// `permissions:` as a map of scope → level; a bare string (`write-all`,
/// `read-all`) comes back as the single entry `* → <string>`.
fn permissions(v: Option<&Value>) -> Vec<(String, String)> {
    match v {
        None => vec![("*".into(), "<default>".into())],
        Some(Value::String(s)) => vec![("*".into(), s.clone())],
        Some(Value::Mapping(m)) => map_pairs(m),
        Some(other) => panic!("unexpected permissions value {other:?}"),
    }
}

fn map_pairs(m: &Mapping) -> Vec<(String, String)> {
    m.iter()
        .map(|(k, v)| {
            (
                k.as_str().unwrap_or_default().to_string(),
                v.as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

fn grants_write(perms: &[(String, String)]) -> bool {
    perms
        .iter()
        .any(|(_, level)| level == "write" || level == "write-all" || level == "<default>")
}

#[test]
fn fix_139_every_action_is_pinned_by_commit_sha_with_its_tag_named() {
    let dir = root().join(".github/workflows");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "yml" || x == "yaml"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    let mut bad = Vec::new();
    for f in &files {
        let text = std::fs::read_to_string(f).unwrap();
        for (n, line) in text.lines().enumerate() {
            let t = line.trim_start().trim_start_matches("- ");
            let Some(rest) = t.strip_prefix("uses:") else {
                continue;
            };
            let (spec, comment) = rest.split_once('#').unwrap_or((rest, ""));
            let spec = spec.trim();
            if spec.starts_with("./") || spec.starts_with("docker://") {
                continue;
            }
            let sha = spec.rsplit_once('@').map(|(_, r)| r).unwrap_or("");
            let pinned = sha.len() == 40 && sha.chars().all(|c| c.is_ascii_hexdigit());
            // Dependabot keeps SHA pins current and rewrites the comment with
            // them; without the tag nobody can tell which release it is.
            let named = comment.trim().starts_with('v');
            if !pinned || !named {
                bad.push(format!(
                    "{}:{}: {}",
                    f.file_name().unwrap().to_string_lossy(),
                    n + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        bad.is_empty(),
        "actions must be pinned by a 40-hex commit SHA with `# vX.Y.Z` after it:\n{}",
        bad.join("\n")
    );
}

fn ci_run_text() -> String {
    let wf = workflow("ci.yml");
    wf.get("jobs")
        .and_then(Value::as_mapping)
        .unwrap()
        .values()
        .map(run_text)
        .collect::<Vec<_>>()
        .join("\n")
}

/// The MSRV `Cargo.toml` promises (`rust-version`), as the Makefile reads it.
fn msrv() -> String {
    let cargo = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
    cargo
        .lines()
        .find_map(|l| l.strip_prefix("rust-version = \""))
        .and_then(|r| r.strip_suffix('"'))
        .unwrap()
        .to_string()
}

#[test]
fn fix_140_ci_checks_advisories_secrets_and_the_msrv_on_every_push() {
    let run = ci_run_text();
    // Dependabot alerts went unread for weeks (F48); a red run is read.
    assert!(
        run.contains("cargo deny") && run.contains("check advisories"),
        "ci.yml runs `cargo deny check advisories`"
    );
    // The local check-secrets.sh matches one shape and `--no-verify` skips
    // it; on a public repository the server-side scan is the one that holds.
    let gl = run
        .lines()
        .find(|l| l.contains("gitleaks git"))
        .expect("ci.yml runs `gitleaks git` over the history");
    assert!(
        gl.contains("--redact"),
        "a public repository's CI log must not print the secret it found: {gl}"
    );
    assert!(
        run.contains("sha256sum -c"),
        "downloaded scanners are checked against a pinned checksum"
    );
    // The MSRV check used to live only in `make release`.
    assert!(
        run.contains("rust-version") && run.contains("cargo +\"$msrv\" check --workspace --locked"),
        "ci.yml checks the workspace with the rust-version Cargo.toml declares ({})",
        msrv()
    );
    // Each of these jobs reads only (a job without its own `permissions:`
    // gets the workflow's).
    let wf = workflow("ci.yml");
    for (name, j) in wf.get("jobs").and_then(Value::as_mapping).unwrap() {
        let p = permissions(j.get("permissions").or(wf.get("permissions")));
        assert!(
            !grants_write(&p),
            "ci.yml job `{}` holds {p:?}",
            name.as_str().unwrap()
        );
    }
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
    assert!(builds
        .iter()
        .all(|l| l.contains("--locked") && l.contains("--release")));
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
