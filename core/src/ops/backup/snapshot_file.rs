//! Reading one file out of a snapshot with `restic dump`, read-only.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};

use super::config::BackupCfg;
use super::restic::restic_cmd;

// ── fix-241: one file from a snapshot, read-only ─────────────────────────

/// fix-241: the most of one file [`read_snapshot_file`] hands back. A
/// settings file is kilobytes; anything past this is cut, and the answer
/// says so (`truncated`) rather than pretending the start is the whole.
pub const SNAPSHOT_FILE_CAP: usize = 1024 * 1024;

/// fix-241: one file as a snapshot holds it, read with `restic dump` —
/// nothing is restored, nothing is written next to live data.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotFile {
    /// The repository (owning app or native unit) it was read from.
    pub owner: String,
    /// The snapshot as asked for: an id or `latest`.
    pub snapshot: String,
    /// The absolute path inside the snapshot.
    pub path: String,
    /// The bytes handed back (at most [`SNAPSHOT_FILE_CAP`]).
    pub shown_bytes: u64,
    /// The file is larger than the cap: `text` is only its start.
    pub truncated: bool,
    /// The cap that applied, so a reader can say "first N bytes".
    pub cap_bytes: u64,
    /// Not text (a NUL byte or not UTF-8, e.g. a database, an image, or a
    /// directory, which `restic dump` sends as a tar): `text` is then None.
    pub binary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

/// fix-241: a snapshot reference `restic dump` may take: `latest`, or a
/// (short or full) hexadecimal snapshot id. Anything else is refused before
/// a command is built, so no flag or path can ride in as the snapshot.
pub fn valid_snapshot_ref(s: &str) -> bool {
    s == "latest" || ((4..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit()))
}

/// fix-241: an absolute path inside a snapshot, plain enough to pass as one
/// argument: no `..`, no control characters.
pub fn valid_snapshot_path(p: &str) -> bool {
    p.starts_with('/')
        && p.len() > 1
        && !p.chars().any(|c| c.is_control())
        && !p.split('/').any(|seg| seg == "..")
}

/// fix-241: the command [`read_snapshot_file`] runs. `restic dump` writes
/// the file to stdout; `head -c` stops reading one byte past the cap (so a
/// 10 GB file never lands in the host's memory, and the extra byte is how
/// the cut is known), and `base64` carries the bytes exactly through the
/// executor's text output. bash, for `PIPESTATUS`: restic's own exit code,
/// not head's. Quiet (fix-39): a settings file can hold a secret, so the
/// transcript names the command and never echoes what came back.
pub fn snapshot_file_cmd(
    cfg: &BackupCfg,
    owner: &str,
    snapshot: &str,
    path: &str,
) -> Result<Cmd, CoreError> {
    check_dump_request(owner, snapshot, path)?;
    let inner = restic_cmd(cfg, owner, &["dump", snapshot, path], 300);
    let cap = (SNAPSHOT_FILE_CAP + 1).to_string();
    let mut args: Vec<&str> = vec![
        "-c",
        "cap=$1; shift; \"$@\" | head -c \"$cap\" | base64 -w0; exit \"${PIPESTATUS[0]}\"",
        "snapshot-file",
        &cap,
        &inner.program,
    ];
    args.extend(inner.args.iter().map(String::as_str));
    Ok(Cmd::new("bash", &args, 300).quiet())
}

/// Refuse an owner, snapshot or path that could ride in as a flag.
fn check_dump_request(owner: &str, snapshot: &str, path: &str) -> Result<(), CoreError> {
    if owner.is_empty()
        || !owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(CoreError::Other(format!(
            "'{}' is not a repository name (the owner `homelab snapshots` lists)",
            owner
        )));
    }
    if !valid_snapshot_ref(snapshot) {
        return Err(CoreError::Other(format!(
            "'{}' is not a snapshot :: give `latest` or an id `homelab snapshots` lists",
            snapshot
        )));
    }
    if !valid_snapshot_path(path) {
        return Err(CoreError::Other(format!(
            "'{}' is not an absolute path inside the snapshot (no `..`)",
            path
        )));
    }
    Ok(())
}

/// fix-241: what [`snapshot_file_cmd`]'s output means. Pure, so the cut, the
/// binary verdict and the error reading are tested without a host.
pub fn parse_snapshot_file(
    owner: &str,
    snapshot: &str,
    path: &str,
    out: &crate::executor::CmdOutput,
) -> Result<SnapshotFile, CoreError> {
    use base64::Engine;
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(out.stdout.trim())
        .map_err(|e| CoreError::Other(format!("the dump of {} did not decode: {}", path, e)))?;
    let truncated = bytes.len() > SNAPSHOT_FILE_CAP;
    // A cut file's restic dies of the closed pipe; that exit is the cut, not
    // a failure. An uncut one must have exited 0.
    if !truncated && !out.success() {
        return Err(CoreError::Other(format!(
            "restic dump {} {} for '{}' failed (exit {}): {}",
            snapshot,
            path,
            owner,
            out.code,
            out.stderr.trim()
        )));
    }
    bytes.truncate(SNAPSHOT_FILE_CAP);
    let text = dumped_text(&bytes, truncated);
    Ok(SnapshotFile {
        owner: owner.to_string(),
        snapshot: snapshot.to_string(),
        path: path.to_string(),
        shown_bytes: bytes.len() as u64,
        truncated,
        cap_bytes: SNAPSHOT_FILE_CAP as u64,
        binary: text.is_none(),
        text,
    })
}

/// The bytes as text, or `None` when they are binary (a NUL byte or not UTF-8).
fn dumped_text(bytes: &[u8], truncated: bool) -> Option<String> {
    if bytes.contains(&0) {
        return None;
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => Some(s.to_string()),
        // The cut can fall inside one multi-byte character: the text up
        // to it is still text.
        Err(e) if truncated && e.error_len().is_none() => {
            Some(String::from_utf8_lossy(&bytes[..e.valid_up_to()]).into_owned())
        }
        Err(_) => None,
    }
}

/// fix-241: read one file from one snapshot of one repository, without a
/// restore: `restic dump` to stdout, capped at [`SNAPSHOT_FILE_CAP`]. Never
/// writes a file, never stops a container, never touches `/appdata`.
pub async fn read_snapshot_file(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    owner: &str,
    snapshot: &str,
    path: &str,
) -> Result<SnapshotFile, CoreError> {
    let cmd = snapshot_file_cmd(cfg, owner, snapshot, path)?;
    let out = exec.run(&cmd).await?;
    parse_snapshot_file(owner, snapshot, path, &out)
}
