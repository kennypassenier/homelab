//! Helpers the document and hook checks share (fix-guards review, LOW:
//! the same walkers and cell splitters had been copied into each file).
//! Every test binary includes this module and uses part of it.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("core/ has a parent")
        .to_path_buf()
}

/// Every file under `root` with extension `ext`, with its text; build
/// output, git, worktrees and node_modules skipped.
pub fn sources_with_ext(root: &Path, ext: &str) -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            if p.is_dir() {
                if name.starts_with("target")
                    || name == ".git"
                    || name == ".claude"
                    || name == "node_modules"
                {
                    continue;
                }
                walk(&p, ext, out);
            } else if p.extension().is_some_and(|x| x == ext) {
                out.push(p);
            }
        }
    }
    let mut paths = Vec::new();
    walk(root, ext, &mut paths);
    paths
        .into_iter()
        .filter_map(|p| std::fs::read_to_string(&p).ok().map(|t| (p, t)))
        .collect()
}

/// The cells of a Markdown table row, trimmed. A `\|` inside a cell is
/// part of the cell, not a border (the same rule as `cells()` in
/// .githooks/check-register.py).
pub fn split_cells(line: &str) -> Vec<String> {
    let mut s = line.trim();
    s = s.strip_prefix('|').unwrap_or(s);
    if s.ends_with('|') && !s.ends_with("\\|") {
        s = &s[..s.len() - 1];
    }
    let mut out = Vec::new();
    let mut cell = String::new();
    let mut prev = '\0';
    for c in s.chars() {
        if c == '|' && prev != '\\' {
            out.push(cell.trim().to_string());
            cell.clear();
        } else {
            cell.push(c);
        }
        prev = c;
    }
    out.push(cell.trim().to_string());
    out
}

/// .githooks/check-register.py: the one implementation of the register and
/// INVARIANTS checks, which the commit hook runs and these tests drive.
pub fn check_register_script() -> PathBuf {
    repo_root().join(".githooks/check-register.py")
}

/// Run check-register.py with `args` in `cwd`, UNMEASURED_OK removed and
/// `env` added; (exit code, stdout, stderr).
pub fn run_check_register(
    args: &[&str],
    cwd: &Path,
    env: &[(&str, &str)],
) -> (i32, String, String) {
    let mut cmd = Command::new("python3");
    cmd.arg(check_register_script())
        .args(args)
        .current_dir(cwd)
        .env_remove("UNMEASURED_OK")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE");
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .expect("python3 runs .githooks/check-register.py");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// `git <args>` in `dir` with a fixed identity and none of the GIT_*
/// variables a hook exports; panics when it fails.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.org"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// Like `git`, but returns whether it succeeded (a merge that conflicts).
pub fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.org"])
        .args([
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_WORK_TREE")
        .output()
        .expect("git runs")
        .status
        .success()
}
