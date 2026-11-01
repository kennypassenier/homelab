//! key-move: the disk checks only the host can make — the deploy key and the
//! known hosts are in place, and no path of a request runs through a symlink
//! out of the clone — and writing the request's files.

use std::path::Path;

use homelab_core::ops::stackcommit::{
    GITHUB_KNOWN_HOSTS, StackCommitConfig, StackFileChange, is_github_ssh, is_local,
};

/// The key file exists, is a file, and only its owner can read it (ssh
/// refuses one others can); the known-hosts file exists, written from
/// GitHub's pinned keys when it is missing and the remote is GitHub.
/// Nothing here reads or returns the key's content.
pub(crate) fn credentials_ready(cfg: &StackCommitConfig) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt as _;
    if is_local(&cfg.remote) {
        return Ok(());
    }
    let meta = std::fs::metadata(&cfg.key).map_err(|e| {
        format!(
            "there is no deploy key at {} ({e}) :: put the key's private half there \
             (mode 0600, owned by root) and send the edit again",
            cfg.key
        )
    })?;
    if !meta.is_file() {
        return Err(format!(
            "{} is not a file :: point stack_commits.key at the key file",
            cfg.key
        ));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(format!(
            "{} can be read by other accounts and ssh refuses it :: chmod 600 {}",
            cfg.key, cfg.key
        ));
    }
    known_hosts_ready(cfg)
}

fn known_hosts_ready(cfg: &StackCommitConfig) -> Result<(), String> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = Path::new(&cfg.known_hosts);
    if path.exists() {
        return Ok(());
    }
    if !is_github_ssh(&cfg.remote) {
        return Err(format!(
            "there is no known-hosts file at {} :: write the remote's ssh host keys there",
            cfg.known_hosts
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o644)
        .open(path)
        .and_then(|mut f| f.write_all(GITHUB_KNOWN_HOSTS.as_bytes()))
        .map_err(|e| {
            format!(
                "GitHub's host keys could not be written to {}: {e}",
                cfg.known_hosts
            )
        })?;
    tracing::info!(
        "stack commits: GitHub's pinned host keys were written to {}",
        cfg.known_hosts
    );
    Ok(())
}

/// Every existing step from the clone down to `rel` is a plain directory or
/// file, never a symlink: with no `..` component (the core guard) that
/// keeps every write inside the clone.
pub(crate) fn symlink_problem(dir: &Path, rel: &str) -> Option<String> {
    let mut at = dir.to_path_buf();
    for part in rel.split('/') {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(m) if m.file_type().is_symlink() => {
                return Some(format!("{rel} runs through a symlink ({})", at.display()));
            }
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    None
}

/// Write (or delete) each file of the request under the clone.
pub(crate) fn write_all(dir: &Path, files: &[StackFileChange]) -> Result<(), String> {
    for f in files {
        if let Some(p) = symlink_problem(dir, &f.path) {
            return Err(p);
        }
        let target = dir.join(&f.path);
        match &f.content {
            Some(text) => {
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", f.path))?;
                }
                std::fs::write(&target, text).map_err(|e| format!("{}: {e}", f.path))?;
            }
            None => match std::fs::remove_file(&target) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", f.path)),
            },
        }
    }
    Ok(())
}
