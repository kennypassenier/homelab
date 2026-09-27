// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): which
// commit a binary was built from. Shared by host/build.rs and client/build.rs
// through `include!`, and by client/tests/build_info_tests.rs, which runs it
// against throwaway repositories.
//
// Both binaries reported only CARGO_PKG_VERSION, so a `make install` or
// `make host-binary` build from a working tree said the same "v3.59.3" as
// the release; `git describe --dirty` tells them apart.

/// `git describe --tags --always --dirty` of `dir`, or `unknown` when `dir`
/// is not in a git tree (a source tarball) or git is not installed.
/// `safe.directory=*` because `make host-binary` builds as root in a
/// container over files the operator owns, which git otherwise refuses.
#[allow(dead_code)]
fn git_describe(dir: &std::path::Path) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "safe.directory=*",
            "describe",
            "--tags",
            "--always",
            "--dirty",
            "--match",
            "v[0-9]*",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if s.is_empty() {
                "unknown".to_string()
            } else {
                s
            }
        }
        _ => "unknown".to_string(),
    }
}

/// The git files whose change moves the describe string: HEAD, the branch
/// it points at, the tags and the index. Only paths that exist, because
/// cargo reruns a build script on every build while a watched path is
/// missing.
#[allow(dead_code)]
fn rerun_paths(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let git = |args: &[&str]| -> Option<String> {
        let o = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "safe.directory=*"])
            .args(args)
            .output()
            .ok()?;
        if !o.status.success() {
            return None;
        }
        let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let mut rel = vec![
        "HEAD".to_string(),
        "index".to_string(),
        "packed-refs".to_string(),
        "refs/tags".to_string(),
    ];
    if let Some(branch) = git(&["symbolic-ref", "-q", "HEAD"]) {
        rel.push(branch);
    }
    rel.iter()
        .filter_map(|r| git(&["rev-parse", "--path-format=absolute", "--git-path", r]))
        .map(std::path::PathBuf::from)
        .filter(|p| p.exists())
        .collect()
}
