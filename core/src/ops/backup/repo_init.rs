//! Creating a restic repository and checking its password.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};
use crate::sink::{Level, PipelineEvent};

use super::restic::restic;

/// A warning line from inside a step closure, where the scope is borrowed.
pub(super) fn scope_log_warn(sink: &dyn crate::sink::Sink, msg: String) {
    sink.emit(PipelineEvent::Line {
        level: Level::Warn,
        source: "HOST".into(),
        msg,
    });
}

/// What `restic init` found (gap-24).
#[derive(Debug, PartialEq, Eq)]
pub enum InitOutcome {
    Created,
    Existed,
}

/// Run `restic init` and read its answer, which used to be thrown away.
///
/// "already exists" / "already initialized" is the normal nightly answer.
/// Anything else that fails (a permission error, a dead remote) is an error
/// now: the backup that followed it could only fail less clearly.
pub async fn init_repository(
    exec: &dyn Executor,
    init: &Cmd,
    repo: &str,
) -> Result<InitOutcome, CoreError> {
    let out = exec.run(init).await?;
    if out.code == 0 {
        return Ok(InitOutcome::Created);
    }
    let text = format!("{} {}", out.stderr.trim(), out.stdout.trim());
    if text.contains("already exists") || text.contains("already initialized") {
        return Ok(InitOutcome::Existed);
    }
    Err(CoreError::Other(format!(
        "restic init of {}-config failed: {} :: nothing was backed up; check the remote \
         (`rclone lsd` on the host) and the password file",
        repo,
        text.trim()
    )))
}

/// A repository created tonight must open with the password that opens
/// host-meta-config (gap-24). A regenerated password file would otherwise
/// encrypt every new repository with a key the offline copy cannot open,
/// and nothing would say so until a restore. When host-meta-config does not
/// exist yet (a first night) there is nothing to compare with.
pub async fn same_password_as_host_meta(
    exec: &dyn Executor,
    restic_base: &str,
    password_file: &str,
    created: &str,
) -> Result<(), CoreError> {
    let out = exec
        .run(&restic(
            restic_base,
            "host-meta",
            password_file,
            &["cat", "config"],
            120,
        ))
        .await?;
    let text = format!("{} {}", out.stderr, out.stdout);
    if out.code != 0 && (text.contains("wrong password") || text.contains("no key found")) {
        return Err(CoreError::Validation(format!(
            "{}-config was just created with a password that does not open host-meta-config \
             :: the password file on the host changed. Put back the password the offline copy \
             holds (docs/OPERATIONS_RUNBOOK.md op-17, lost-1), then remove the new repository \
             {}-config before the next run",
            created, created
        )));
    }
    Ok(())
}
