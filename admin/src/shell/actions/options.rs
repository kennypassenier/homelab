//! The lists a form's choices come from: roll-back targets, release tags
//! and release notes.

use homelab_core::ops::deployguard;

use super::{Actions, CommitInfo};
use crate::core::actions::{self, Refusal};

impl Actions {
    /// feat-stacks-6: what a stack can go back to.
    pub async fn rollback_options(&self, stack: String) -> Result<serde_json::Value, Refusal> {
        if !actions::valid_stack_name(&stack) {
            return Err(Refusal::new(
                format!("roll-back list of {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ));
        }
        let applied = self.fleet_stack(&stack).and_then(|(_, a)| a);
        let files = self.inner.files.clone();
        let s = stack.clone();
        let (commits, natives, present) = tokio::task::spawn_blocking(move || {
            (
                files.commits(&s, 30),
                files.native_units(&s),
                files.present(),
            )
        })
        .await
        .map_err(|e| Refusal::new("roll-back list", e.to_string(), "report this"))?;
        let applied_commit = applied
            .as_deref()
            .and_then(deployguard::applied_commit)
            .map(str::to_string);
        Ok(rollback_view(RollbackFacts {
            stack,
            applied,
            applied_commit,
            present,
            natives,
            commits: commits.unwrap_or_default(),
        }))
    }

    /// dashboard-latest: the "release tag" dropdown of update-host (the
    /// host's own repository) and install-native (`unit`'s `release_repo`,
    /// from the stack's own files — never hard-coded).
    pub async fn release_options(
        &self,
        stack: &str,
        unit: Option<&str>,
    ) -> Result<serde_json::Value, Refusal> {
        let what = format!("the release list of {stack}");
        let repo = if stack == actions::HOST_TARGET {
            crate::shell::releases::HOMELAB_REPO.to_string()
        } else {
            let files = self.inner.files.clone();
            let (s, u) = (stack.to_string(), unit.map(str::to_string));
            let (m, _, _) =
                tokio::task::spawn_blocking(move || files.native_release(&s, u.as_deref()))
                    .await
                    .map_err(|e| Refusal::new(what.clone(), e.to_string(), "report this"))??;
            m.release_repo.unwrap_or_default()
        };
        if repo.is_empty() {
            return Err(Refusal::new(
                what,
                "this service declares no release_repo",
                "there is nothing to fetch a release list from",
            ));
        }
        let list = self
            .releases()?
            .list(&repo)
            .await
            .map_err(|why| Refusal::new(what, why, "try again in a moment"))?;
        let choices = crate::core::releaseoptions::dropdown(&list);
        Ok(serde_json::json!({ "releases": choices }))
    }

    /// ask-kp11be-6: a release's notes for the Update flow's gate.
    pub async fn release_notes(
        &self,
        repo: &str,
        tag: &str,
    ) -> Result<crate::core::releasenotes::Notes, Refusal> {
        let what = format!("the release notes of {tag}");
        if !crate::core::releasenotes::valid_repo(repo) || !crate::core::actions::valid_tag(tag) {
            return Err(Refusal::new(
                what,
                "that is not a GitHub repository and release tag",
                "open the release page from the link instead",
            ));
        }
        self.releases()?
            .notes(repo, tag)
            .await
            .map_err(|why| Refusal::new(what.clone(), why, "try again in a moment"))?
            .ok_or_else(|| {
                Refusal::new(
                    what,
                    format!("{repo} has no GitHub release for {tag}"),
                    "the project may only tag it; open its page from the link",
                )
            })
    }
}

struct RollbackFacts {
    stack: String,
    applied: Option<String>,
    applied_commit: Option<String>,
    present: bool,
    natives: Vec<String>,
    commits: Vec<CommitInfo>,
}

/// The roll-back list as the page reads it.
fn rollback_view(f: RollbackFacts) -> serde_json::Value {
    let commits: Vec<serde_json::Value> = f
        .commits
        .into_iter()
        .map(|c| {
            let is_applied = f
                .applied_commit
                .as_deref()
                .is_some_and(|a| c.commit.starts_with(a));
            serde_json::json!({
                "commit": c.commit, "at": c.at, "subject": c.subject, "applied": is_applied,
            })
        })
        .collect();
    serde_json::json!({
        "stack": f.stack,
        "applied_source": f.applied,
        "applied_commit": f.applied_commit,
        "working_copy": f.present,
        // deploy-commit: the stack's files as they were at that commit.
        "commits": commits,
        // rollback-native: the host keeps one previous binary per unit
        // (fix-114); it does not say which version that is.
        "native_units": f.natives,
        "missing": [
            "the host does not list whether a native unit has a kept previous binary, or which version it is",
            "for compose apps the host keeps the previous image only during an update (automatic roll back on a failed health check); going back later means deploying an earlier commit",
        ],
    })
}
