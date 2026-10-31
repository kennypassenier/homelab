//! Resolving a typed path to the absolute path a snapshot holds.

use crate::manifest::StackManifest;

use super::owners::owner_groups;
use super::snapshot_file::valid_snapshot_path;

/// fix-241: the directories repository `owner` of stack `m` backs up — the
/// ones a typed path is resolved against. Empty when `owner` is no
/// repository of this stack.
pub fn snapshot_dirs(m: &StackManifest, owner: &str) -> Vec<String> {
    owner_groups(m)
        .into_iter()
        .find(|(o, _)| o == owner)
        .map(|(_, paths)| paths)
        .unwrap_or_default()
}

/// fix-241: where a typed path points inside `owner`'s snapshots, from the
/// stack's manifest as host state records it. A native unit's snapshot is
/// one tar stream, not a tree of files, so it is refused with what to use
/// instead.
pub fn snapshot_file_target(
    state: &crate::state::HostState,
    stack: &str,
    owner: &str,
    typed: &str,
) -> Result<String, String> {
    let Some(st) = state.stacks.get(stack) else {
        return Err(format!("stack '{}' is not in host state", stack));
    };
    if st.is_native() {
        return Err(format!(
            "'{}' is a native service: its snapshot is one tar stream, not a tree of files :: \
             restore it with `homelab restore-native`",
            stack
        ));
    }
    let Some(m) = st.manifest.as_ref() else {
        return Err(format!(
            "stack '{}' has no manifest in host state :: deploy it once",
            stack
        ));
    };
    let dirs = snapshot_dirs(m, owner);
    if dirs.is_empty() {
        let owners: Vec<String> = owner_groups(m).into_iter().map(|(o, _)| o).collect();
        return Err(format!(
            "'{}' is no repository of stack '{}' :: one of: {}",
            owner,
            stack,
            owners.join(", ")
        ));
    }
    resolve_snapshot_path(&dirs, typed)
}

/// fix-241: a path as a person types it, made into the absolute path the
/// snapshot holds. `paths` are the directories the app's repository backs
/// up (its `storage` host paths). A relative path is taken inside the one
/// directory when there is one; an absolute path must lie under one of
/// them, so a typo names the directories instead of asking restic for a
/// path that cannot be there.
pub fn resolve_snapshot_path(paths: &[String], typed: &str) -> Result<String, String> {
    if paths.is_empty() {
        return Err("this app has no backed-up directory, so no snapshot holds its files".into());
    }
    let abs = absolute_typed_path(paths, typed)?;
    if !valid_snapshot_path(&abs) {
        return Err(format!(
            "'{}' is not a plain path inside the snapshot (no `..`)",
            typed
        ));
    }
    let under = paths.iter().any(|p| {
        let p = p.trim_end_matches('/');
        abs == p || abs.starts_with(&format!("{}/", p))
    });
    if !under {
        return Err(format!(
            "'{}' is not under a directory this app backs up :: one of: {}",
            abs,
            paths.join(", ")
        ));
    }
    Ok(abs)
}

/// A typed path made absolute: relative only when the app has one directory.
fn absolute_typed_path(paths: &[String], typed: &str) -> Result<String, String> {
    if typed.starts_with('/') {
        Ok(typed.to_string())
    } else if paths.len() == 1 {
        Ok(format!(
            "{}/{}",
            paths[0].trim_end_matches('/'),
            typed.trim_start_matches("./")
        ))
    } else {
        Err(format!(
            "'{}' is relative and this app backs up more than one directory :: give the \
             absolute path, under one of: {}",
            typed,
            paths.join(", ")
        ))
    }
}
