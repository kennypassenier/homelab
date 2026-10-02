//! fix-219 (drift-finding-names-only-filenames): a repo-drift finding named
//! only the file names that changed, so Kenny had to ask what changed. These
//! tests cover the pure pieces core carries for that: the capped unified
//! diff, the secret-path guard, and the text-matching that finds a
//! repo-drift finding's stack name in a rendered `homelab check`/`today`
//! report (the client and the dashboard build the actual diff themselves,
//! once a stack is already known to have drifted — `client/tests/apply_tests.rs`
//! covers that half).

use homelab_core::ops::fleetcheck::{
    FILE_DIFF_MAX_LINES, is_repo_file_drift_text, is_secret_path,
    repo_drift_stacks_in_rendered_text,
};
use homelab_core::textdiff::unified_capped;

#[test]
fn a_changed_line_shows_up_as_old_and_new() {
    let old = "services:\n  a:\n    image: old:1\n";
    let new = "services:\n  a:\n    image: new:1\n";
    let text = unified_capped(
        "x/docker-compose.yml",
        Some(old),
        Some(new),
        FILE_DIFF_MAX_LINES,
    );
    assert!(text.contains("-    image: old:1"), "{text}");
    assert!(text.contains("+    image: new:1"), "{text}");
}

#[test]
fn a_new_file_shows_only_added_lines() {
    let text = unified_capped("x/new.yml", None, Some("a\nb\n"), FILE_DIFF_MAX_LINES);
    assert!(text.contains("+a"), "{text}");
    assert!(text.contains("+b"), "{text}");
    assert!(!text.contains("-a"), "{text}");
}

#[test]
fn a_removed_file_shows_only_removed_lines() {
    let text = unified_capped("x/gone.yml", Some("a\nb\n"), None, FILE_DIFF_MAX_LINES);
    assert!(text.contains("-a"), "{text}");
    assert!(text.contains("-b"), "{text}");
    assert!(!text.contains("+a"), "{text}");
}

/// fix-219: the cap works. A 30-line change capped to 20 lines of body
/// shows exactly 20 body lines and names how many more there were — the
/// header lines (`--- `/`+++ `/`@@ @@`) are structure, not change, and do
/// not count against the cap.
#[test]
fn a_diff_longer_than_the_cap_is_capped_with_a_more_lines_marker() {
    let old: String = (0..30).map(|i| format!("old{i}\n")).collect();
    let new: String = (0..30).map(|i| format!("new{i}\n")).collect();
    let text = unified_capped("x/big.yml", Some(&old), Some(&new), 20);
    let body_lines: Vec<&str> = text
        .lines()
        .filter(|l| !(l.starts_with("--- ") || l.starts_with("+++ ") || l.starts_with("@@ ")))
        .collect();
    // 20 shown + the "… N more lines" marker itself.
    assert_eq!(body_lines.len(), 21, "{:?}", body_lines);
    assert!(
        body_lines.last().unwrap().contains("more lines"),
        "{:?}",
        body_lines
    );
}

#[test]
fn an_unchanged_file_has_no_body_lines_at_all() {
    let text = unified_capped(
        "x/same.yml",
        Some("a\nb\n"),
        Some("a\nb\n"),
        FILE_DIFF_MAX_LINES,
    );
    assert!(
        text.lines()
            .all(|l| l.starts_with("--- ") || l.starts_with("+++ ")),
        "{text}"
    );
}

#[test]
fn secret_paths_are_recognised_regardless_of_directory() {
    assert!(is_secret_path(".env"));
    assert!(is_secret_path("syncthing/.env"));
    assert!(is_secret_path("stacks/kyu/kyu-runner/.env.local"));
    assert!(!is_secret_path("syncthing/docker-compose.yml"));
    assert!(!is_secret_path("admin/env-notes.md"));
}

#[test]
fn repo_file_drift_text_is_recognised_in_both_what_shapes() {
    // `Finding::what` on its own (the fleet-check finding row).
    assert!(is_repo_file_drift_text(
        "the files differ from what the host applied on 2026-10-02 — changed: a/docker-compose.yml"
    ));
    // `"<subject>: <what>"`, as `ops::today::assemble` folds it.
    assert!(is_repo_file_drift_text(
        "syncthing: the files differ from what the host applied on 2026-10-02 — changed: a/docker-compose.yml"
    ));
    assert!(!is_repo_file_drift_text("last backup was 30 hours ago"));
}

/// fix-219: the stack name is found in both rendered shapes — `render()`'s
/// `[drift] <stack> — …` and `ops::today::render`'s `[attention] <stack>:
/// … (check)` — and only for a real repo-drift line, never for an unrelated
/// one that happens to share a severity tag.
#[test]
fn repo_drift_stacks_are_found_in_both_rendered_report_shapes() {
    let check_text = "fleet check: 1 drift\n\
         drift — works, bites on the next deploy or outage:\n  \
         [drift] syncthing — the files differ from what the host applied on 2026-10-01 — changed: a/docker-compose.yml\n  \
         [drift] kyu — net0 has firewall=0, so Proxmox applies none of the declared rules\n      remedy: x\n";
    assert_eq!(
        repo_drift_stacks_in_rendered_text(check_text),
        vec!["syncthing".to_string()]
    );

    let today_text = "  [attention] syncthing: the files differ from what the host applied on 2026-10-01 — changed: a/docker-compose.yml (check)\n      \u{2192} `homelab deploy stacks/syncthing` (or `homelab apply`) to apply them, or put the files back as they were\nNothing else needs you";
    assert_eq!(
        repo_drift_stacks_in_rendered_text(today_text),
        vec!["syncthing".to_string()]
    );
}

#[test]
fn repo_drift_stacks_are_deduplicated_and_sorted() {
    let text = "  [drift] b — the files differ from what the host applied on 2026-10-01 — changed: x\n\
         [drift] a — the files differ from what the host applied on 2026-10-01 — changed: y\n\
         [drift] a — the files differ from what the host applied on 2026-09-30 — changed: z\n";
    assert_eq!(
        repo_drift_stacks_in_rendered_text(text),
        vec!["a".to_string(), "b".to_string()]
    );
}

#[test]
fn a_clean_report_names_no_drifted_stacks() {
    assert!(repo_drift_stacks_in_rendered_text("fleet check: repo and reality agree").is_empty());
}
