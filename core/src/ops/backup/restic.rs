//! The restic command builders every backup, restore and drill shares.

use crate::executor::Cmd;

use super::config::BackupCfg;

/// Where restic keeps its index cache. Without it every single operation
/// re-downloads the repository index from Google Drive first.
///
/// It was not missing by choice: restic derives the path from `$XDG_CACHE_HOME`
/// or `$HOME`, and a systemd service has neither, so every backup in this
/// fleet has run with `unable to open cache: neither $XDG_CACHE_HOME nor
/// $HOME are defined` in its output — a line that reads as noise and costs a
/// full index fetch per repository, of which the gateway alone has six.
pub const RESTIC_CACHE_DIR: &str = "/var/lib/homelab/restic-cache";

/// Build a Cmd that runs restic with the repo env inline (via `env`).
pub(crate) fn restic(
    base: &str,
    stack: &str,
    password_ref: &str,
    args: &[&str],
    timeout: u64,
) -> Cmd {
    // The host wraps this so RESTIC_PASSWORD comes from its secret store; here
    // we pass a reference the host resolves. In tests the MockExecutor just
    // records the argv. Path join uses "/" — everything lives under one
    // gdrive folder (homelab-backups), not loose dirs in the drive root.
    let repo = format!("{}/{}-config", base, stack);
    let mut full = vec![
        "env".to_string(),
        format!("RESTIC_REPOSITORY={}", repo),
        format!("RESTIC_PASSWORD_FILE={}", password_ref),
        format!("RESTIC_CACHE_DIR={}", RESTIC_CACHE_DIR),
        "restic".to_string(),
    ];
    full.extend(args.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = full.iter().map(|s| s.as_str()).collect();
    Cmd::new(refs[0], &refs[1..], timeout)
}

/// Build a restic command from a BackupCfg (shared with deploy's E3
/// auto-restore step).
pub(crate) fn restic_cmd(cfg: &BackupCfg, stack: &str, args: &[&str], timeout: u64) -> Cmd {
    restic(&cfg.restic_base, stack, &cfg.password_file, args, timeout)
}
