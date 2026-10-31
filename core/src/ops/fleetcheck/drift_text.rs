//! The words a repo-drift finding opens with, and the helpers that find and
//! treat those findings in rendered text.

/// fix-219 (drift-finding-names-only-filenames, 2026-10-02): the exact words
/// `evaluate_repo_drift` opens a finding with. One place names them, so the
/// client and the dashboard — which only see this finding's rendered text or
/// its `what` field, never a structured "this is a repo-drift finding" flag
/// (digests are hashes, not content, by design: see this module's doc
/// comment) — can find the same findings `evaluate_repo_drift` produced
/// without a second copy of the sentence to keep in step.
pub const REPO_DRIFT_WHAT_PREFIX: &str = "the files differ from what the host applied on";

/// fix-219: is `what` (a `Finding::what`, or `"<subject>: <what>"` as
/// `ops::today::assemble` folds it) a repo-drift finding from
/// `evaluate_repo_drift` — the one kind of finding that names a bare
/// filename where Kenny wants the actual change.
pub fn is_repo_file_drift_text(what: &str) -> bool {
    what.contains(REPO_DRIFT_WHAT_PREFIX)
}

/// fix-219: every stack named by a repo-drift finding in `rendered` — the
/// text `render()` (for `homelab check`) or `ops::today::render` (for
/// `homelab today`) produces. Both shapes put the stack directly beside the
/// phrase (`"[drift] <stack> — the files differ …"` and `"[attention]
/// <stack>: the files differ … (check)"`), so one scan handles either: find
/// the phrase, trim the separator right before it (" —" or ":"), then take
/// the text after the last `"] "` on what remains. Sorted and deduplicated so
/// a caller fetches each drifted stack's applied files once.
pub fn repo_drift_stacks_in_rendered_text(rendered: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in rendered.lines() {
        let Some(pos) = line.find(REPO_DRIFT_WHAT_PREFIX) else {
            continue;
        };
        let before = line[..pos].trim_end();
        let before = before
            .strip_suffix(" —")
            .or_else(|| before.strip_suffix(':'))
            .unwrap_or(before);
        let stack = before.rsplit("] ").next().unwrap_or(before).trim();
        if !stack.is_empty() {
            out.push(stack.to_string());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// fix-219: is `path` one this project never puts in a repository/applied
/// comparison in the first place (`.env`, `.env.*`) — belt and braces on top
/// of `evaluate_repo_drift`'s own inputs, which never carry a secret path at
/// all (`fetch_secrets: false`, see this module's doc comment above). A
/// caller that builds a per-file diff from full file content (the client, or
/// the dashboard, fetching `GetApplied`) checks this before ever putting
/// that content in a diff — never content for a secret, only "changed".
pub fn is_secret_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == ".env" || name.starts_with(".env.")
}

/// fix-219: how much of one file's diff a drift finding shows inline, before
/// "… N more lines" — short enough to read beside the finding, not a
/// replacement for `git diff` on the stack directory itself.
pub const FILE_DIFF_MAX_LINES: usize = 20;
