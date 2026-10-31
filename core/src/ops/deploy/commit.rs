//! D4: intent into the host-local git repo (never secrets, A5).

use crate::error::CoreError;
use crate::executor::{Cmd, CmdOutput, run_ok};
use crate::runner::StepOutcome;

use super::run::Run;

impl Run<'_, '_> {
    pub(super) async fn commit_intent(&self) -> Result<StepOutcome, CoreError> {
        let (ctx, spec, m, exec) = self.parts();
        let repo = format!("{}/repo", ctx.state_dir);
        let stack_dir = format!("{}/stacks/{}", repo, m.stack_name);
        self.remove_dropped_from_repo(&stack_dir).await?;
        for f in &spec.files {
            exec.write_file(&format!("{}/{}", stack_dir, f.path), &f.content, 0o644)
                .await?;
        }
        self.ensure_repo(&repo).await?;
        run_ok(exec, &Cmd::new("git", &["-C", &repo, "add", "-A"], 30)).await?;
        let msg = self.commit_message();
        let commit = exec
            .run(&Cmd::new(
                "git",
                &["-C", &repo, "commit", "-q", "-m", &msg],
                30,
            ))
            .await?;
        Self::commit_outcome(&commit)
    }

    /// step-22: the copy holds what the stack declares and nothing else —
    /// a file the stack dropped used to stay here forever.
    async fn remove_dropped_from_repo(&self, stack_dir: &str) -> Result<(), CoreError> {
        let exec = self.exec;
        for rel in &self.dropped_repo_files {
            run_ok(
                exec,
                &Cmd::new("rm", &["-f", &format!("{}/{}", stack_dir, rel)], 30),
            )
            .await?;
        }
        if !self.dropped_repo_files.is_empty() {
            let _ = exec
                .run(&Cmd::new(
                    "find",
                    &[
                        stack_dir,
                        "-mindepth",
                        "1",
                        "-type",
                        "d",
                        "-empty",
                        "-delete",
                    ],
                    30,
                ))
                .await;
        }
        Ok(())
    }

    async fn ensure_repo(&self, repo: &str) -> Result<(), CoreError> {
        let exec = self.exec;
        let git_check = exec
            .run(&Cmd::new(
                "git",
                &["-C", repo, "rev-parse", "--git-dir"],
                20,
            ))
            .await?;
        if git_check.success() {
            return Ok(());
        }
        run_ok(exec, &Cmd::new("git", &["-C", repo, "init", "-q"], 30)).await?;
        run_ok(
            exec,
            &Cmd::new(
                "git",
                &["-C", repo, "config", "user.email", "host@homelab.local"],
                20,
            ),
        )
        .await?;
        run_ok(
            exec,
            &Cmd::new(
                "git",
                &["-C", repo, "config", "user.name", "homelab-host"],
                20,
            ),
        )
        .await?;
        Ok(())
    }

    /// fix-141: names which commit went live and whether files in the
    /// stack directory were uncommitted. Story: docs/deployment/REGISTER.md.
    fn commit_message(&self) -> String {
        let (_, spec, m, _) = self.parts();
        match spec.source.as_ref() {
            Some(src) => format!(
                "deploy {} ({})\n\n{}",
                m.stack_name,
                src.summary(),
                src.commit_body()
            ),
            None => format!(
                "deploy {}\n\nsource not reported (the client is too old to report it)\n",
                m.stack_name
            ),
        }
    }

    /// Hardening H15: ONLY the benign no-op case may pass silently. Any
    /// other commit failure (dubious ownership, index lock, missing
    /// identity) previously made every deploy green while the intent
    /// history — the whole rollback + mirror story — stayed empty.
    fn commit_outcome(commit: &CmdOutput) -> Result<StepOutcome, CoreError> {
        if commit.success() {
            return Ok(StepOutcome::Changed);
        }
        let noise = format!("{}{}", commit.stdout, commit.stderr);
        if noise.contains("nothing to commit")
            || noise.contains("nothing added to commit")
            || noise.contains("no changes added")
        {
            Ok(StepOutcome::Unchanged)
        } else {
            Err(CoreError::Command {
                rendered: "git commit (intent history)".into(),
                detail: format!(
                    "intent commit failed — history/rollback/mirror would silently stop: {}",
                    noise.trim()
                ),
            })
        }
    }
}
