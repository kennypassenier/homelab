//! The GitHub workflows are part of the supply chain of a binary that runs as
//! root on the hypervisor, so their security properties are asserted here
//! rather than trusted to review (expert panel 2026-09-27,
//! host-release-unsigned). The files are parsed, not grepped, so a property
//! moved to another key or job is still found.

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

fn job<'a>(wf: &'a Value, name: &str) -> &'a Value {
    wf.get("jobs")
        .and_then(|j| j.get(name))
        .unwrap_or_else(|| panic!("job `{name}` missing"))
}

fn steps(job: &Value) -> Vec<&Value> {
    job.get("steps")
        .and_then(Value::as_sequence)
        .map(|s| s.iter().collect())
        .unwrap_or_default()
}

fn step_uses(step: &Value) -> Option<&str> {
    step.get("uses").and_then(Value::as_str)
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
fn fix_139_the_release_build_runs_without_a_write_token() {
    let wf = workflow("release.yml");
    // Nothing at workflow level: a job that forgets to ask gets no rights,
    // instead of inheriting the publish job's write token.
    let top = permissions(wf.get("permissions"));
    assert!(
        !grants_write(&top),
        "release.yml grants {top:?} at workflow level; every job must ask for its own"
    );

    let build = job(&wf, "build");
    let perms = permissions(build.get("permissions"));
    assert_eq!(
        perms,
        vec![("contents".to_string(), "read".to_string())],
        "the build job runs every dependency's build.rs and must hold contents: read only"
    );
    let checkouts: Vec<_> = steps(build)
        .into_iter()
        .filter(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/checkout@")))
        .collect();
    assert!(!checkouts.is_empty(), "the build job checks the code out");
    for c in checkouts {
        assert_eq!(
            c.get("with").and_then(|w| w.get("persist-credentials")),
            Some(&Value::Bool(false)),
            "checkout must not leave the token in .git/config for the build to read"
        );
    }
    let run = run_text(build);
    for cmd in ["cargo clippy", "cargo test", "cargo build"] {
        let lines: Vec<_> = run.lines().filter(|l| l.contains(cmd)).collect();
        assert!(!lines.is_empty(), "the build job runs `{cmd}`");
        for l in lines {
            assert!(
                l.contains("--locked"),
                "`{}` must build what Cargo.lock pins (F235)",
                l.trim()
            );
        }
    }
}

#[test]
fn fix_139_only_the_publish_job_writes_and_it_builds_nothing() {
    let wf = workflow("release.yml");
    let publish = job(&wf, "publish");
    assert_eq!(
        publish.get("needs").and_then(Value::as_str),
        Some("build"),
        "publish runs after the build and only on its result"
    );
    let perms = permissions(publish.get("permissions"));
    assert_eq!(
        perms,
        vec![("contents".to_string(), "write".to_string())],
        "publish holds exactly the write token a release needs"
    );
    assert!(
        steps(publish)
            .iter()
            .all(|s| !step_uses(s).is_some_and(|u| u.starts_with("actions/checkout@"))),
        "publish must not check out code: no build step runs next to the write token"
    );
    assert!(
        !run_text(publish).contains("cargo "),
        "publish must not compile anything"
    );
    // Any other job in the file must not hold a write token either.
    for (name, j) in wf.get("jobs").and_then(Value::as_mapping).unwrap() {
        let name = name.as_str().unwrap();
        if name != "publish" {
            let p = permissions(j.get("permissions"));
            assert!(!grants_write(&p), "job `{name}` holds {p:?}");
        }
    }
}

#[test]
fn fix_139_a_release_refuses_a_tag_off_main_or_unequal_to_the_version() {
    let wf = workflow("release.yml");
    let build = job(&wf, "build");
    let run = run_text(build);
    assert!(
        run.contains("GITHUB_REF_NAME") && run.contains("Cargo.toml"),
        "the build compares the tag with the workspace version"
    );
    assert!(
        run.contains("merge-base --is-ancestor") && run.contains("origin/main"),
        "the build refuses a tagged commit that is not on main"
    );
    // The ancestry check needs main's history in the checkout.
    let checkout = steps(build)
        .into_iter()
        .find(|s| step_uses(s).is_some_and(|u| u.starts_with("actions/checkout@")))
        .unwrap();
    assert_eq!(
        checkout.get("with").and_then(|w| w.get("fetch-depth")),
        Some(&Value::Number(0.into())),
        "fetch-depth 0, or origin/main is not there to compare against"
    );
    // The check comes before anything is compiled.
    let pos = |needle: &str| {
        steps(build).iter().position(|s| {
            s.get("run")
                .and_then(Value::as_str)
                .is_some_and(|r| r.contains(needle))
        })
    };
    assert!(
        pos("merge-base --is-ancestor").unwrap() < pos("cargo").unwrap(),
        "refuse the tag before building"
    );
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
fn fix_140_the_release_build_restores_a_cache_and_never_writes_one() {
    let wf = workflow("release.yml");
    let build = job(&wf, "build");
    let uses: Vec<_> = steps(build).into_iter().filter_map(step_uses).collect();
    assert!(
        uses.iter().any(|u| u.starts_with("actions/cache/restore@")),
        "the release build restores CI's cargo cache: {uses:?}"
    );
    // A tag's own cache scope is new every release, so saving is wasted;
    // and a release job that writes no cache cannot poison one.
    assert!(
        !uses
            .iter()
            .any(|u| u.starts_with("actions/cache@") || u.starts_with("actions/cache/save@")),
        "the release build must not save a cache: {uses:?}"
    );
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
