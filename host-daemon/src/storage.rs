//! HOST storage operations — health checks, mount validation, and status surfacing.
//!
//! Provides utilities for inspecting stack storage, validating bind mount prerequisites,
//! and exposing storage status to CLIENT and deploy workflows.

use std::path::{Path, PathBuf};
use std::fs;

/// Storage health status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageHealth {
    Healthy,
    Warning,
    Critical,
}

impl std::fmt::Display for StorageHealth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageHealth::Healthy => write!(f, "healthy"),
            StorageHealth::Warning => write!(f, "warning"),
            StorageHealth::Critical => write!(f, "critical"),
        }
    }
}

/// Storage status for a stack
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StackStorageStatus {
    pub stack_name: String,
    pub host_path: PathBuf,
    pub health: StorageHealth,
    pub exists: bool,
    pub writable: bool,
    pub message: String,
}

/// Bind mount requirement for a stack
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BindMountRequirement {
    pub host_path: PathBuf,
    pub container_path: PathBuf,
    pub writable: bool,
}

/// Preflight check result for bind mounts
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BindMountPreflight {
    pub requirement: BindMountRequirement,
    pub exists: bool,
    pub writable: bool,
    pub message: String,
}

/// Inspect storage health for a stack
pub fn inspect_stack_storage(
    stack_name: &str,
    host_path: &Path,
) -> Result<StackStorageStatus, String> {
    if !host_path.exists() {
        return Ok(StackStorageStatus {
            stack_name: stack_name.to_string(),
            host_path: host_path.to_path_buf(),
            health: StorageHealth::Critical,
            exists: false,
            writable: false,
            message: format!("Path does not exist: {}", host_path.display()),
        });
    }

    let exists = true;
    let writable = is_writable(host_path);

    let health = if !writable {
        StorageHealth::Warning
    } else {
        StorageHealth::Healthy
    };

   let message = if !writable {
       format!("Path exists but is not writable: {}", host_path.display())
   } else {
       format!("Ready for stacks: {}", host_path.display())
   };

    Ok(StackStorageStatus {
        stack_name: stack_name.to_string(),
        host_path: host_path.to_path_buf(),
        health,
       exists,
        writable,
        message,
    })
}

/// Validate bind mount prerequisites before deploy/restore
#[allow(dead_code)]
pub fn check_bind_mount_preflight(
    requirement: &BindMountRequirement,
) -> Result<BindMountPreflight, String> {
    let exists = requirement.host_path.exists();
   let writable = if exists { is_writable(&requirement.host_path) } else { false };

   let message = if !exists {
       format!("Path does not exist: {}", requirement.host_path.display())
   } else if !writable && requirement.writable {
       format!("Path exists but is not writable: {}", requirement.host_path.display())
   } else if !writable {
       format!("Path is read-only (OK for read-only mounts)")
   } else {
       "Ready for binding".to_string()
   };

    Ok(BindMountPreflight {
        requirement: requirement.clone(),
        exists,
        writable,
        message,
    })
}

/// Check if a path is writable
fn is_writable(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    // Try to create a test file
    let test_file = path.join(".write_test_tmp");
    match fs::write(&test_file, b"") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
            true
        }
        Err(_) => false,
    }
}

/// Get storage metrics summary for all stacks
pub fn get_storage_summary(appdata_root: &Path) -> Result<Vec<StackStorageStatus>, String> {
    if !appdata_root.exists() {
        return Err(format!(
            "Appdata root does not exist: {}",
            appdata_root.display()
        ));
    }

    let mut results = Vec::new();

    // Iterate over stack directories
    if let Ok(entries) = fs::read_dir(appdata_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(stack_name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(status) = inspect_stack_storage(stack_name, &path) {
                        results.push(status);
                    }
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_health_display() {
        assert_eq!(StorageHealth::Healthy.to_string(), "healthy");
        assert_eq!(StorageHealth::Warning.to_string(), "warning");
        assert_eq!(StorageHealth::Critical.to_string(), "critical");
    }

    #[test]
    fn test_nonexistent_path() {
        let result = inspect_stack_storage("test", Path::new("/nonexistent/path"));
        assert!(result.is_ok());
        let status = result.unwrap();
        assert_eq!(status.health, StorageHealth::Critical);
       assert!(!status.exists);
    }
}
