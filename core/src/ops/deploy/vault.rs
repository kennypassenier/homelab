//! Where a secret file a native unit reads is kept in the host vault.

/// A5: the vault filename for a file a unit reads.
///
/// fix-37: `<parent dir>/<name>`, so `/appdata/kyu/kyu-runner-config/token.env`
/// becomes `kyu-runner-config/token.env` under `<state_dir>/secrets/<stack>/`.
/// It used to be the bare file name, on the belief that two units on one
/// stack cannot name the same file. They can: kyu-runner and
/// http-switchboard both read `token.env`, so they shared ONE vault copy and
/// the vault kept whichever was copied last; a rebuild would have given both
/// the same token.
pub fn vault_key(path: &str) -> String {
    let mut parts = path.rsplit('/');
    let name = parts.next().unwrap_or(path);
    match parts.next() {
        Some(dir) if !dir.is_empty() => format!("{}/{}", dir, name),
        _ => name.to_string(),
    }
}

/// The flat key older deploys wrote, still read when it is unambiguous.
pub(super) fn legacy_vault_key(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}
