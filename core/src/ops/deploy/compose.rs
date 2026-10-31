//! Reading `docker compose up -d` output.

/// Did `docker compose up -d` create or recreate a container? Compose ends
/// such a status line with the word `Created` or `Recreated`.
///
/// shell-strings-quoting (expert panel, 2026-09-27): this was
/// `contains("reated")`, which a warning that a volume "was not created by
/// Docker Compose" also matches, and the restart a changed file needs was
/// then skipped.
pub fn compose_reports_created(out: &str) -> bool {
    out.lines().any(|l| {
        let mut words = l.split_whitespace();
        words.next() == Some("Container")
            && matches!(words.last(), Some("Created") | Some("Recreated"))
    })
}
