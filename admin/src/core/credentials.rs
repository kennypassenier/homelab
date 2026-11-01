//! arch-push-credential on CT 120: the working copy's ssh key and GitHub's
//! host keys, decided without I/O.
//!
//! CT 120 gets the deploy key the way it gets every secret: latch fills
//! admin.env, so the key arrives as `HOMELAB_ADMIN_DEPLOY_KEY_B64` (the
//! private key file, base64). The working copy hands ssh a *file*, so at
//! start the shell writes that file (mode 0600) when it is missing. ssh also
//! refuses a host it has no key for; GitHub's published host keys are pinned
//! here (never fetched with `ssh-keyscan`, which would trust whoever
//! answered first) and written to the known-hosts file when it is missing.

/// The environment variable latch fills with the deploy key.
pub const DEPLOY_KEY_ENV: &str = "HOMELAB_ADMIN_DEPLOY_KEY_B64";

/// GitHub's ssh host keys: one pinned list, shared with the host
/// (key-move), in `homelab_core::ops::stackcommit`.
pub use homelab_core::ops::stackcommit::GITHUB_KNOWN_HOSTS;

/// The key file's bytes from the variable's value. The refusal never
/// carries any part of the value: it is a secret.
pub fn decode_deploy_key(b64: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let compact: String = b64.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return Err(format!("{DEPLOY_KEY_ENV} is empty"));
    }
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(compact.as_bytes())
        .map_err(|_| format!("{DEPLOY_KEY_ENV} is not base64 (its value is not shown)"))?;
    let text = String::from_utf8_lossy(&bytes);
    if !text.contains("-----BEGIN") || !text.contains("PRIVATE KEY-----") {
        return Err(format!(
            "{DEPLOY_KEY_ENV} does not decode to a private key file (-----BEGIN … PRIVATE KEY-----)"
        ));
    }
    // ssh refuses a key file whose last line has no newline.
    if bytes.last() != Some(&b'\n') {
        bytes.push(b'\n');
    }
    Ok(bytes)
}

/// Whether a remote reaches GitHub over ssh (only then are GitHub's host
/// keys the ones ssh needs); shared with the host (key-move).
pub use homelab_core::ops::stackcommit::is_github_ssh;
