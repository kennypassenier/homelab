//! arch-state: the dashboard's own files are typed JSON with a
//! `schema_version`, written to a temporary file, synced and renamed over
//! the old one, so a power cut leaves the old file or the new one and never
//! half of either.

use std::io::Write as _;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("{path}: {why}")]
    Io { path: String, why: String },
    #[error("{path} does not read as the dashboard's file: {why}")]
    Parse { path: String, why: String },
}

fn io(path: &Path, e: impl std::fmt::Display) -> StateError {
    StateError::Io {
        path: path.display().to_string(),
        why: e.to_string(),
    }
}

/// Write `value` to `path` atomically: temp + fsync + rename + dir fsync.
pub fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<(), StateError> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(|e| io(dir, e))?;
    let body = serde_json::to_vec_pretty(value).map_err(|e| io(path, e))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "state".into());
    let tmp = dir.join(format!(".{name}.tmp-{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp).map_err(|e| io(&tmp, e))?;
        f.write_all(&body).map_err(|e| io(&tmp, e))?;
        f.write_all(b"\n").map_err(|e| io(&tmp, e))?;
        f.sync_all().map_err(|e| io(&tmp, e))?;
    }
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(path, e)
    })?;
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Read `path`; a missing file is `None`, an unreadable one an error (never
/// silently replaced by an empty state, which would lose every schedule).
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>, StateError> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| StateError::Parse {
                path: path.display().to_string(),
                why: e.to_string(),
            }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io(path, e)),
    }
}
