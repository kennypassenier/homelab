//! Which stacks a `homelab check` / `homelab today` argument names.
//!
//! The argument has always been read as the stacks directory. Given one
//! stack's directory (`stacks/kyu`) it listed that stack's component
//! directories as if they were stacks, so the host reported every real stack
//! as gone from the repository (or, for a stack whose components carry no
//! `service.yml`, found no stack files at all and skipped the comparison).

use std::path::Path;

/// The stacks directory a check reads, and the one stack it is limited to
/// when the argument was a single stack directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckScope {
    pub base: String,
    pub only: Option<String>,
}

/// A stack directory holds its own `lxc-compose.yml` or `service.yml`; a
/// stacks directory holds neither.
fn is_stack_dir(dir: &Path) -> bool {
    dir.join("lxc-compose.yml").exists() || dir.join("service.yml").exists()
}

/// Reads `arg` as a stack directory when it is one, else as the stacks
/// directory it always was.
pub fn check_scope(arg: &str) -> CheckScope {
    let dir = Path::new(arg);
    let name = dir.file_name().map(|n| n.to_string_lossy().to_string());
    match (is_stack_dir(dir), name) {
        (true, Some(name)) => {
            let parent = dir.parent().filter(|p| !p.as_os_str().is_empty());
            CheckScope {
                base: parent.unwrap_or(Path::new(".")).display().to_string(),
                only: Some(name),
            }
        }
        _ => CheckScope {
            base: arg.to_string(),
            only: None,
        },
    }
}

impl CheckScope {
    /// The stack files whose digests are compared with the host: all of
    /// them for a stacks directory, the named stack's alone otherwise. The
    /// full list still goes to the host as the repository's contents, so
    /// "gone from the repository" stays true for the other stacks.
    pub fn digest_files(&self, files: &[(String, u16)]) -> Vec<(String, u16)> {
        let Some(only) = &self.only else {
            return files.to_vec();
        };
        files
            .iter()
            .filter(|(dir, _)| dir.trim_end_matches('/').rsplit('/').next() == Some(only))
            .cloned()
            .collect()
    }
}
