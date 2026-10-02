//! fix-64 (restore-no-confirm-no-safety-snapshot, 2026-09-27): a
//! `homelab snapshots stacks/<name>` verb. `homelab restore` already takes
//! a snapshot id; there was no way short of the dashboard's Backups page to
//! read what ids exist. This reuses `Command::GetBackups` (feat-backup-1/2)
//! unchanged — the same per-repository status, with every snapshot, the
//! dashboard's Backups page and its restore picker already read — so there
//! is one source of this list, not a second query built to match it.

use homelab_core::ops::backup::RepoStatus;

/// The shape of `Command::GetBackups`'s reply (`host/src/main.rs`'s
/// `Rpc::GetBackups` arm): `{"native": bool, "repos": [RepoStatus, ...]}`.
#[derive(Debug, serde::Deserialize)]
pub struct GetBackupsReply {
    pub native: bool,
    pub repos: Vec<RepoStatus>,
}

/// The host's `GetBackups` reply (`{"native": bool, "repos": [RepoStatus]}`)
/// as a human list: one repository per block, newest snapshot first, the id
/// `homelab restore stacks/<name> <id>` takes.
pub fn render(native: bool, repos: &[RepoStatus]) -> String {
    let mut out = String::new();
    if repos.is_empty() {
        out.push_str("no repositories (nothing has backed up this stack yet)\n");
        return out;
    }
    for r in repos {
        out.push_str(&format!(
            "{}{}\n",
            r.owner,
            if native { " (native unit)" } else { "" }
        ));
        if let Some(e) = &r.error {
            out.push_str(&format!("  error: {}\n", e));
            continue;
        }
        if r.snapshots.is_empty() {
            out.push_str("  no snapshots\n");
            continue;
        }
        for s in &r.snapshots {
            let run = s.run.map(|r| format!(" run {}", r)).unwrap_or_default();
            out.push_str(&format!("  {}  {}{}\n", s.short_id, s.time, run));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use homelab_core::ops::backup::SnapRun;

    fn repo(owner: &str, snaps: Vec<SnapRun>) -> RepoStatus {
        RepoStatus {
            owner: owner.into(),
            newest_snapshot: snaps.first().cloned(),
            snapshot_count: snaps.len(),
            snapshots: snaps,
            size_bytes: None,
            drill: None,
            error: None,
            measured_at: None,
        }
    }

    #[test]
    fn fix_64_lists_every_snapshot_newest_first_as_handed_in() {
        let snaps = vec![
            SnapRun {
                id: "aaaa".into(),
                short_id: "aaaa".into(),
                time: 2000,
                run: Some(2000),
            },
            SnapRun {
                id: "bbbb".into(),
                short_id: "bbbb".into(),
                time: 1000,
                run: Some(1000),
            },
        ];
        let out = render(true, &[repo("kyu", snaps)]);
        assert!(out.contains("kyu (native unit)"));
        let aaaa_at = out.find("aaaa").unwrap();
        let bbbb_at = out.find("bbbb").unwrap();
        assert!(
            aaaa_at < bbbb_at,
            "newest-first order from the host stands: {}",
            out
        );
    }

    #[test]
    fn fix_64_no_repositories_says_so() {
        assert!(render(true, &[]).contains("nothing has backed up"));
    }

    #[test]
    fn fix_64_an_empty_repository_says_no_snapshots_not_nothing() {
        let out = render(false, &[repo("paperless", Vec::new())]);
        assert!(out.contains("paperless"));
        assert!(out.contains("no snapshots"));
    }

    #[test]
    fn fix_64_a_repository_error_is_shown_not_silently_skipped() {
        let mut r = repo("media", Vec::new());
        r.error = Some("restic snapshots failed".into());
        let out = render(true, &[r]);
        assert!(out.contains("restic snapshots failed"));
    }
}
