//! What a deploy changes, file by file, against what the host applied last,
//! and the wipe list: secret-free reads for the review pages.

use std::time::Duration;

use homelab_proto::{Command, FileBlob};

use super::Actions;
use crate::core::actions::Refusal;
use crate::core::stackedit::FileChange;

impl Actions {
    /// The working copy's files of `stack`, read off the async threads.
    async fn local_files(&self, stack: &str, what: &str) -> Result<Vec<FileBlob>, Refusal> {
        let files = self.inner.files.clone();
        let s2 = stack.to_string();
        tokio::task::spawn_blocking(move || files.stack_files(&s2))
            .await
            .map_err(|e| Refusal::new(what, e.to_string(), "report this"))?
    }

    /// The files the host applied last (`GetApplied`).
    async fn applied_files(&self, stack: &str, what: &str) -> Result<Vec<FileBlob>, Refusal> {
        let r = self
            .inner
            .host
            .ask_traced(
                Command::GetApplied {
                    stack: stack.to_string(),
                },
                Duration::from_secs(30),
                None,
            )
            .await
            .map_err(|e| Refusal::new(what, e, "check that the host answers"))?;
        if !r.ok {
            return Err(Refusal::new(
                what,
                r.message,
                "look at the host's intent repository",
            ));
        }
        serde_json::from_str(&r.message).map_err(|e| {
            Refusal::new(
                what,
                format!("the host's applied files do not read: {e}"),
                "update the host and the dashboard to the same release",
            )
        })
    }

    /// Whether the host has applied this stack before.
    fn applied_before(&self, stack: &str) -> bool {
        self.inner
            .shared
            .try_read()
            .ok()
            .and_then(|s| {
                s.fleet.as_ref().map(|f| {
                    f.stacks
                        .iter()
                        .any(|x| x.name == stack && !x.applied_hash.is_empty())
                })
            })
            .unwrap_or(false)
    }

    /// TUI parity (the plan in the plain Deploy review, fix-100): what a
    /// deploy of `stack` changes, file by file, against the files the host
    /// applied last (`GetApplied`), with each file's diff. Secrets are never
    /// part of it: neither side carries them.
    pub async fn deploy_diff(&self, stack: &str) -> Result<serde_json::Value, Refusal> {
        let local = self.local_files(stack, "the plan").await?;
        let known = self.applied_before(stack);
        let applied = if known {
            self.applied_files(stack, "the plan").await?
        } else {
            Vec::new()
        };
        let changes = changed_files(&local, &applied);
        let summary = homelab_client::apply::file_changes(&local, &applied);
        // fix-159: the running native units this deploy restarts, and why.
        let restarts = if known {
            homelab_client::apply::native_restarts(&local, &applied)
        } else {
            Vec::new()
        };
        Ok(serde_json::json!({
            "new_stack": !known,
            "files": crate::core::editplan::file_diffs(&changes),
            "summary": summary,
            "restarts": restarts,
            "note": if changes.is_empty() {
                "the files are as the host applied them; secrets or settings may still differ"
            } else {
                "the files as they change; secrets are not shown (neither side carries them here)"
            },
        }))
    }

    /// fix-219 (drift-finding-names-only-filenames): the actual per-file
    /// change behind a `homelab check`/`today` repo-drift finding, read only
    /// when the Health page's finding is expanded — the normal, no-drift
    /// fleet check never calls this. Same pairing `deploy_diff` already uses
    /// for the Apply page (this working copy's pinned files against the
    /// host's intent-repository copy, `GetApplied`, secret-free by
    /// construction, D6), capped short through
    /// `homelab_client::apply::file_diffs` instead of shown in full.
    pub async fn repo_drift_file_diff(&self, stack: &str) -> Result<serde_json::Value, Refusal> {
        let local = self.local_files(stack, "the drift diff").await?;
        let applied = self.applied_files(stack, "the drift diff").await?;
        let diffs = homelab_client::apply::file_diffs(&local, &applied);
        Ok(serde_json::json!({
            "stack": stack,
            "files": diffs,
            "note": "secrets are never part of this: neither side carries them",
        }))
    }

    /// fix-wipe-1: what a wipe of `key` deletes, as the host lists it
    /// (a `WipeRetired` without a name only lists, never deletes) — the
    /// preview shows it before the name is typed.
    pub async fn wipe_plan(&self, key: &str) -> Result<String, Refusal> {
        let r = self
            .inner
            .host
            .ask_traced(
                Command::WipeRetired {
                    name: key.to_string(),
                    confirm: None,
                },
                Duration::from_secs(30),
                None,
            )
            .await
            .map_err(|e| Refusal::new("the wipe list", e, "check that the host answers"))?;
        if r.ok {
            Ok(r.message)
        } else {
            Err(Refusal::new(
                "the wipe list",
                r.message,
                "read the Retired page",
            ))
        }
    }
}

/// Every file that differs between the working copy and the host's copy,
/// sorted by path.
fn changed_files(local: &[FileBlob], applied: &[FileBlob]) -> Vec<FileChange> {
    let mut changes: Vec<FileChange> = Vec::new();
    for f in local {
        match applied.iter().find(|x| x.path == f.path) {
            None => changes.push(FileChange {
                path: f.path.clone(),
                old: None,
                new: Some(f.content.clone()),
            }),
            Some(x) if x.content != f.content || x.mode != f.mode => changes.push(FileChange {
                path: f.path.clone(),
                old: Some(x.content.clone()),
                new: Some(f.content.clone()),
            }),
            Some(_) => {}
        }
    }
    for x in applied {
        if !local.iter().any(|f| f.path == x.path) {
            changes.push(FileChange {
                path: x.path.clone(),
                old: Some(x.content.clone()),
                new: None,
            });
        }
    }
    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}
