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
