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

/// fix-241: what `homelab snapshot-file` was asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotFileArgs {
    pub stack: String,
    pub snapshot: String,
    pub app: String,
    pub path: String,
    pub json: bool,
}

pub const SNAPSHOT_FILE_USAGE: &str =
    "usage: homelab snapshot-file stacks/<name> <snapshot|latest> --app <app> <path> [--json]";

/// fix-241: the arguments after `homelab snapshot-file`. Flags may stand
/// anywhere, as for `restore`; the words are the stack, the snapshot and
/// the path, in that order. `--app` is required: each app has its own
/// repository, so without it there is no one place to read from.
pub fn snapshot_file_args(rest: &[String]) -> Result<SnapshotFileArgs, String> {
    let mut positional: Vec<String> = Vec::new();
    let mut app = None;
    let mut json = false;
    let mut it = rest.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--json" => json = true,
            "--app" => match it.next() {
                Some(name) if !name.starts_with("--") && !name.is_empty() => {
                    app = Some(name.clone())
                }
                _ => {
                    return Err(format!(
                        "--app needs an app name :: {}",
                        SNAPSHOT_FILE_USAGE
                    ));
                }
            },
            other if other.starts_with("--") => {
                return Err(format!("unknown flag {} :: {}", other, SNAPSHOT_FILE_USAGE));
            }
            other => positional.push(other.to_string()),
        }
    }
    let [stack, snapshot, path]: [String; 3] = positional
        .try_into()
        .map_err(|_| SNAPSHOT_FILE_USAGE.to_string())?;
    let app = app.ok_or_else(|| {
        format!(
            "--app is required: each app has its own repository :: {}",
            SNAPSHOT_FILE_USAGE
        )
    })?;
    Ok(SnapshotFileArgs {
        stack,
        snapshot,
        app,
        path,
        json,
    })
}

/// fix-241: the file for stdout (exactly its text, so it can be redirected
/// into a file to compare), and the note for stderr: what was read, and
/// whether it was cut at the cap or is not text.
pub fn render_snapshot_file(f: &homelab_core::ops::backup::SnapshotFile) -> (String, String) {
    let mut note = format!(
        "{} from snapshot {} of repository {} — read only, nothing restored",
        f.path, f.snapshot, f.owner
    );
    if f.truncated {
        note.push_str(&format!(
            "\nCUT: the file is larger than {} KiB; this is only its first {} bytes",
            f.cap_bytes / 1024,
            f.shown_bytes
        ));
    }
    if f.binary {
        note.push_str(&format!(
            "\nnot text ({} bytes read: a database, an image, or a directory) — nothing printed",
            f.shown_bytes
        ));
    }
    (f.text.clone().unwrap_or_default(), note)
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
                size_bytes: None,
                file_count: None,
                trigger: None,
            },
            SnapRun {
                id: "bbbb".into(),
                short_id: "bbbb".into(),
                time: 1000,
                run: Some(1000),
                size_bytes: None,
                file_count: None,
                trigger: None,
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
