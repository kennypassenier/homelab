//! Browsing one snapshot's directory tree, read-only.

use crate::error::CoreError;
use crate::executor::Executor;

use super::config::BackupCfg;
use super::restic::restic_cmd;

/// feat-backup-3: one entry `restic ls --json` reports — a file or a
/// directory under the browsed path, read-only.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotEntry {
    pub name: String,
    pub path: String,
    /// "file" or "dir", restic's own `struct_type`/`type` vocabulary.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime: Option<String>,
}

/// feat-backup-3: parse `restic ls <id> --json [path]`'s newline-delimited
/// output into the entries directly under `path` (not every descendant —
/// restic `ls` without `--recursive` already stops there, this only drops
/// the leading `snapshot` summary line `ls` also emits).
pub fn parse_snapshot_ls(raw: &str) -> Vec<SnapshotEntry> {
    #[derive(serde::Deserialize)]
    struct Node {
        #[serde(default)]
        struct_type: String,
        #[serde(default)]
        name: String,
        #[serde(default)]
        path: String,
        #[serde(default, rename = "type")]
        node_type: String,
        #[serde(default)]
        size: Option<u64>,
        #[serde(default)]
        mtime: Option<String>,
    }
    raw.lines()
        .filter_map(|l| serde_json::from_str::<Node>(l.trim()).ok())
        .filter(|n| n.struct_type == "node")
        .map(|n| SnapshotEntry {
            name: n.name,
            path: n.path,
            kind: n.node_type,
            size: n.size,
            mtime: n.mtime,
        })
        .collect()
}

/// feat-backup-3: list one snapshot's files under `path` ("" = root),
/// read-only — never writes, never stops a container.
pub async fn browse_snapshot(
    exec: &dyn Executor,
    cfg: &BackupCfg,
    owner: &str,
    snapshot: &str,
    path: &str,
) -> Result<Vec<SnapshotEntry>, CoreError> {
    let target = if path.is_empty() {
        "/".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    };
    let out = exec
        .run(&restic_cmd(
            cfg,
            owner,
            &["ls", "--json", snapshot, &target],
            120,
        ))
        .await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "restic ls {} {} for '{}' failed (exit {}): {}",
            snapshot,
            target,
            owner,
            out.code,
            out.stderr.trim()
        )));
    }
    Ok(parse_snapshot_ls(&out.stdout))
}
