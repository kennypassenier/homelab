//! key-move: the checks a `CommitStackFiles` request passes before the host
//! touches its clone. Pure: no file is read here. The host adds the checks
//! only the disk can answer (a symlink on the way, the staged set after
//! `git add`).

use super::StackFileChange;

/// Why one repository path is not a plain one, or None. Refused: an empty
/// path, an absolute one, a backslash or a control character, and any
/// component that is empty, `.`, `..` or `.git` (in any case) — so a request
/// can neither climb out of the clone nor write into git's own files.
pub fn path_problem(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("an empty path".into());
    }
    if path.starts_with('/') {
        return Some(format!("{path:?} is absolute"));
    }
    if path.chars().any(|c| c == '\\' || c.is_control()) {
        return Some(format!("{path:?} holds a backslash or a control character"));
    }
    let trimmed = path.strip_suffix('/').unwrap_or(path);
    for part in trimmed.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return Some(format!(
                "{path:?} is not a plain path (empty, . or .. component)"
            ));
        }
        if part.eq_ignore_ascii_case(".git") {
            return Some(format!("{path:?} reaches into .git"));
        }
    }
    None
}

/// A secret's file name (`.env`, `app.env`, `.env.local`): the repository is
/// public, so the host never commits one, whatever the request says. The
/// same rule the dashboard's editor uses to leave such files out.
pub fn is_secret_name(name: &str) -> bool {
    name == ".env" || name.ends_with(".env") || name.starts_with(".env")
}

/// Whether `path` lies under one of `roots`: a root ending in `/` is a
/// directory prefix, any other root is one exact file.
pub fn under_roots(path: &str, roots: &[String]) -> bool {
    roots.iter().any(|r| {
        if r.ends_with('/') {
            path.starts_with(r.as_str()) && path.len() > r.len()
        } else {
            path == r
        }
    })
}

/// A full commit hash (SHA-1 or SHA-256, lowercase hex).
pub fn is_commit_hash(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// A branch name git would take and a refspec cannot misread.
pub fn is_branch_name(b: &str) -> bool {
    !b.is_empty()
        && !b.starts_with('-')
        && !b.starts_with('+')
        && !b.contains("..")
        && !b.contains(':')
        && !b.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// A remote the host may push to: ssh (`git@host:path`, `ssh://`) or a local
/// path (tests). Never http(s): a token would have to sit in the URL.
pub fn remote_problem(r: &str) -> Option<String> {
    if r.trim().is_empty() {
        return Some("stack_commits.remote is empty".into());
    }
    if r.starts_with("http://") || r.starts_with("https://") {
        return Some(
            "stack_commits.remote must be an ssh remote (git@github.com:owner/repo.git): \
             the push goes with the deploy key, never a token in a URL"
                .into(),
        );
    }
    None
}

/// Every reason the request is refused before anything is written; empty
/// when it may go ahead.
pub fn request_problems(
    message: &str,
    base: &str,
    files: &[StackFileChange],
    roots: &[String],
    max_bytes: u64,
) -> Vec<String> {
    let mut out = Vec::new();
    if message.trim().is_empty() {
        out.push("the commit message is empty".to_string());
    }
    if !is_commit_hash(base) {
        out.push(format!("base {base:?} is not a full commit hash"));
    }
    if files.is_empty() {
        out.push("the request names no file".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    for f in files {
        out.extend(file_problem(f, roots));
        if !seen.insert(f.path.as_str()) {
            out.push(format!("{} is named twice", f.path));
        }
    }
    let size: u64 = message.len() as u64
        + files
            .iter()
            .map(|f| f.content.as_ref().map_or(0, |c| c.len() as u64))
            .sum::<u64>();
    if size > max_bytes {
        out.push(format!(
            "the request carries {size} bytes, more than stack_commits.max_bytes ({max_bytes})"
        ));
    }
    out
}

fn file_problem(f: &StackFileChange, roots: &[String]) -> Option<String> {
    if let Some(p) = path_problem(&f.path) {
        return Some(p);
    }
    if f.path.ends_with('/') {
        return Some(format!("{} names a directory, not a file", f.path));
    }
    if !under_roots(&f.path, roots) {
        return Some(format!(
            "{} lies outside the paths the host commits ({})",
            f.path,
            roots.join(", ")
        ));
    }
    let name = f.path.rsplit('/').next().unwrap_or(&f.path);
    if is_secret_name(name) {
        return Some(format!(
            "{} is a secret's file and never enters the repository",
            f.path
        ));
    }
    None
}

/// The staged paths that the request did not name: after `git add` these
/// must be none (a rename or a stray file would otherwise ride along).
pub fn staged_outside<'a>(staged: &'a [String], files: &[StackFileChange]) -> Vec<&'a str> {
    staged
        .iter()
        .map(String::as_str)
        .filter(|s| !files.iter().any(|f| f.path == *s))
        .collect()
}
