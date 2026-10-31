//! The audit file and the private modes of the daemon's records.

/// fix-36: remote exec checks the no-touch list the daemon actually runs
/// with. It used `SafetyConfig::default()`, so a vmid host.toml added to the
/// list (F8 lets config widen it) was still reachable through `homelab exec`.
/// Append one line to audit.log.
///
/// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27): a new
/// file is created 0600. It was created with the default mode, 0644, and it
/// records every exec command.
pub(crate) fn append_audit(path: &str, line: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)?
        .write_all(line.as_bytes())
}

/// fix-125: at start, take group and world access off the daemon's private
/// records that already exist: audit.log, journal.jsonl and the incident
/// bundles (0700 directories, 0600 files, the replay scripts 0700). Before
/// fix-125 all of them were created readable by every account on pve, and a
/// new mode for new files leaves the old ones as they were. Best effort: a
/// path that cannot be changed is said once and skipped.
pub(crate) fn tighten_private_paths(state_dir: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let set = |p: &std::path::Path, mode: u32| {
        let Ok(meta) = std::fs::symlink_metadata(p) else {
            return;
        };
        if meta.file_type().is_symlink() || meta.permissions().mode() & 0o777 == mode {
            return;
        }
        if let Err(e) = std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)) {
            tracing::warn!("could not make {} private :: {}", p.display(), e);
        }
    };
    let base = std::path::Path::new(state_dir);
    set(&base.join("audit.log"), 0o600);
    set(&base.join("journal.jsonl"), 0o600);
    let incidents = base.join("incidents");
    set(&incidents, 0o700);
    for bundle in std::fs::read_dir(&incidents)
        .into_iter()
        .flatten()
        .flatten()
    {
        let path = bundle.path();
        if !path.is_dir() {
            set(&path, 0o600);
            continue;
        }
        set(&path, 0o700);
        for file in std::fs::read_dir(&path).into_iter().flatten().flatten() {
            let f = file.path();
            let is_script = f.extension().is_some_and(|e| e == "sh");
            set(&f, if is_script { 0o700 } else { 0o600 });
        }
    }
}
