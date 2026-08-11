//! D5: offsite mirror of the host intent repo. Best-effort by design — a
//! failed push must never block or fail a deploy; the caller retries later.
//! Secrets never enter the repo (A5), so the mirror carries none.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};

/// Push the intent repo to `remote` under the git remote name "mirror",
/// configuring the remote on first use.
pub async fn mirror_push(
    exec: &dyn Executor,
    repo_dir: &str,
    remote: &str,
) -> Result<(), CoreError> {
    let has = exec
        .run(&Cmd::new(
            "git",
            &["-C", repo_dir, "remote", "get-url", "mirror"],
            30,
        ))
        .await?;
    if !has.success() {
        let add = exec
            .run(&Cmd::new(
                "git",
                &["-C", repo_dir, "remote", "add", "mirror", remote],
                30,
            ))
            .await?;
        if !add.success() {
            return Err(CoreError::Other(format!(
                "git remote add: {}",
                add.stderr.trim()
            )));
        }
    }
    let push = exec
        .run(&Cmd::new(
            "git",
            &["-C", repo_dir, "push", "--quiet", "mirror", "--all"],
            300,
        ))
        .await?;
    if !push.success() {
        return Err(CoreError::Other(format!(
            "git push mirror: {}",
            push.stderr.trim()
        )));
    }
    Ok(())
}
