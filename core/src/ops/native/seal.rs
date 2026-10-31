//! gap-27: sealing a container's secret file into the host's vault.

use crate::error::CoreError;
use crate::executor::Executor;

/// gap-27 / redesign-stackhub-2: copy one secret file from a container into
/// the host's vault, read without echoing it (fix-39); nothing in the
/// container is written. False when the file is absent or empty there.
pub async fn seal_one(
    exec: &dyn Executor,
    vmid: u16,
    on_container: &str,
    vault: &str,
) -> Result<bool, CoreError> {
    let got = crate::executor::pct_sh_secret(
        exec,
        vmid,
        &format!(
            "cat {} 2>/dev/null || true",
            crate::ops::util::shq(on_container)
        ),
        60,
    )
    .await?;
    if got.stdout.trim().is_empty() {
        return Ok(false);
    }
    exec.write_file(vault, &got.stdout, 0o600).await?;
    Ok(true)
}
