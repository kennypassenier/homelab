//! Small shared helpers for operations.

use crate::error::CoreError;
use crate::executor::{pct_sh, run_ok, Cmd, Executor};

/// Push literal content to a path inside an LXC. Returns true when the
/// destination changed (drives conditional restarts — B1).
pub async fn push_content(
    exec: &dyn Executor,
    vmid: u16,
    dest: &str,
    content: &str,
    perms: &str,
) -> Result<bool, CoreError> {
    let current = pct_sh(
        exec,
        vmid,
        &format!("cat '{}' 2>/dev/null || true", dest),
        30,
    )
    .await?
    .stdout;
    if current == content {
        return Ok(false);
    }
    if let Some(parent) = std::path::Path::new(dest).parent() {
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "exec",
                    &vmid.to_string(),
                    "--",
                    "mkdir",
                    "-p",
                    &parent.display().to_string(),
                ],
                30,
            ),
        )
        .await?;
    }
    let tmp = "/tmp/homelab-push";
    exec.write_file(tmp, content, 0o600).await?;
    run_ok(
        exec,
        &Cmd::new(
            "pct",
            &["push", &vmid.to_string(), tmp, dest, "--perms", perms],
            60,
        ),
    )
    .await?;
    Ok(true)
}
