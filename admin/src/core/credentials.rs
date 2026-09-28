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

/// GitHub's ssh host keys, as GitHub publishes them
/// (docs.github.com, "GitHub's SSH key fingerprints", and `api.github.com/meta`
/// `ssh_keys`; compared 2026-09-28). One `known_hosts` line each.
pub const GITHUB_KNOWN_HOSTS: &str = "\
github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl
github.com ecdsa-sha2-nistp256 AAAAE2VjZHNhLXNoYTItbmlzdHAyNTYAAAAIbmlzdHAyNTYAAABBBEmKSENjQEezOmxkZMy7opKgwFB9nkt5YRrYMjNuG5N87uRgg6CLrbo5wAdT/y6v0mKV0U2w0WZ2YB/++Tpockg=
github.com ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQCj7ndNxQowgcQnjshcLrqPEiiphnt+VTTvDP6mHBL9j1aNUkY4Ue1gvwnGLVlOhGeYrnZaMgRK6+PKCUXaDbC7qtbW8gIkhL7aGCsOr/C56SJMy/BCZfxd1nWzAOxSDPgVsmerOBYfNqltV9/hWCqBywINIR+5dIg6JTJ72pcEpEjcYgXkE2YEFXV1JHnsKgbLWNlhScqb2UmyRkQyytRLtL+38TGxkxCflmO+5Z8CSSNY7GidjMIZ7Q4zMjA2n1nGrlTDkzwDCsw+wqFPGQA179cnfGWOWRVruj16z6XyvxvjJwbz0wQZ75XK5tKSb7FNyeIEs4TT4jk+S4dhPeAUC5y+bDYirYgM4GC7uEnztnZyaVWQ7B381AK4Qdrwt51ZqExKbQpTUNn+EjqoTwvqNj4kqx5QUCI0ThS/YkOxJCXmPUWZbhjpCg56i+2aB6CmK2JGhn57K5mj0MNdBXA4/WnwH6XoPWJzK5Nyu2zB3nAZp+S5hpQs+p1vN1/wsjk=
";

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
/// keys the ones ssh needs).
pub fn is_github_ssh(remote: &str) -> bool {
    remote.starts_with("git@github.com:") || remote.starts_with("ssh://git@github.com/")
}
