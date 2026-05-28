//! Restore execution backend — coordinate HOST/LXC restore workflows.
//!
//! Orchestrates safe, atomic restore operations across the HOST (storage layer)
//! and LXC (application layer), with granular progress reporting and fail-closed behavior.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Restore scope (what to restore)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RestoreScope {
    Stack,       // Restore single stack
    Environment, // Restore entire environment (all stacks)
}

/// Restore phase states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum RestorePhase {
    Queued,              // Restore request queued
    ValidatingBackup,    // Checking backup integrity
    QuiescingServices,   // Pausing LXC containers/apps
    RestoringStorage,    // Restoring data to host paths
    SyncingApplications, // Restarting apps + running post-restore hooks
    Completed,           // Restore finished
    Failed,              // Restore failed
}

impl std::fmt::Display for RestorePhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RestorePhase::Queued => write!(f, "queued"),
            RestorePhase::ValidatingBackup => write!(f, "validating-backup"),
            RestorePhase::QuiescingServices => write!(f, "quiescing-services"),
            RestorePhase::RestoringStorage => write!(f, "restoring-storage"),
            RestorePhase::SyncingApplications => write!(f, "syncing-applications"),
            RestorePhase::Completed => write!(f, "completed"),
            RestorePhase::Failed => write!(f, "failed"),
        }
    }
}

/// Restore operation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreRequest {
    pub scope: RestoreScope,
    pub stack_names: Vec<String>, // Single entry if Stack scope, multiple if Environment
    pub backup_id: String,        // Backup snapshot ID to restore from
    pub verify_only: bool,        // If true, validate but don't apply
    pub skip_post_hooks: bool,    // If true, skip post-restore scripts
}

/// Restore event (progress update)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreEvent {
    pub operation_id: String,
    pub timestamp: u64,
    pub phase: RestorePhase,
    pub stack_name: String,
    pub progress_percent: u8, // 0-100
    pub message: String,
    pub is_error: bool,
}

/// Overall restore operation status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreStatus {
    pub operation_id: String,
    pub created_at: u64,
    pub scope: RestoreScope,
    pub stack_names: Vec<String>,
    pub current_phase: RestorePhase,
    pub overall_progress: u8, // 0-100
    pub events: Vec<RestoreEvent>,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Backup validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupValidation {
    pub backup_id: String,
    pub valid: bool,
    pub size_bytes: u64,
    pub created_at: u64,
    pub contains_stacks: Vec<String>,
    pub message: String,
}

impl RestoreRequest {
    #[allow(dead_code)]
    pub fn new(backup_id: String, stack_names: Vec<String>) -> Self {
        RestoreRequest {
            scope: if stack_names.len() == 1 {
                RestoreScope::Stack
            } else {
                RestoreScope::Environment
            },
            stack_names,
            backup_id,
            verify_only: false,
            skip_post_hooks: false,
        }
    }
}

impl RestoreStatus {
    pub fn new(operation_id: String, request: &RestoreRequest) -> Self {
        RestoreStatus {
            operation_id,
            created_at: current_timestamp(),
            scope: request.scope,
            stack_names: request.stack_names.clone(),
            current_phase: RestorePhase::Queued,
            overall_progress: 0,
            events: Vec::new(),
            success: false,
            error_message: None,
        }
    }

    #[allow(dead_code)]
    pub fn add_event(&mut self, phase: RestorePhase, stack_name: &str, message: &str) {
        self.add_event_with_progress(phase, stack_name, self.overall_progress, message);
    }

    pub fn add_event_with_progress(
        &mut self,
        phase: RestorePhase,
        stack_name: &str,
        progress: u8,
        message: &str,
    ) {
        let event = RestoreEvent {
            operation_id: self.operation_id.clone(),
            timestamp: current_timestamp(),
            phase,
            stack_name: stack_name.to_string(),
            progress_percent: progress,
            message: message.to_string(),
            is_error: false,
        };
        self.events.push(event);
        self.current_phase = phase;
        self.overall_progress = progress;
    }

    pub fn add_error(&mut self, phase: RestorePhase, stack_name: &str, error: &str) {
        let event = RestoreEvent {
            operation_id: self.operation_id.clone(),
            timestamp: current_timestamp(),
            phase,
            stack_name: stack_name.to_string(),
            progress_percent: self.overall_progress,
            message: error.to_string(),
            is_error: true,
        };
        self.events.push(event);
        self.current_phase = RestorePhase::Failed;
        self.error_message = Some(error.to_string());
        self.success = false;
    }

    pub fn mark_complete(&mut self) {
        self.current_phase = RestorePhase::Completed;
        self.overall_progress = 100;
        self.success = true;
    }
}

/// Validate backup before attempting restore
pub fn validate_backup(backup_id: &str, backup_root: &str) -> Result<BackupValidation, String> {
    let backup_path = Path::new(backup_root).join(backup_id);
    if !backup_path.exists() {
        return Err(format!(
            "Backup path does not exist: {}",
            backup_path.display()
        ));
    }

    if !backup_path.is_dir() {
        return Err(format!(
            "Backup path is not a directory: {}",
            backup_path.display()
        ));
    }

    let mut contains_stacks = Vec::new();
    let mut size_bytes = 0_u64;

    let entries = std::fs::read_dir(&backup_path).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                contains_stacks.push(name.to_string());
            }
            size_bytes = size_bytes.saturating_add(dir_size_bytes(&p));
        }
    }

    let valid = !contains_stacks.is_empty();
    Ok(BackupValidation {
        backup_id: backup_id.to_string(),
        valid,
        size_bytes,
        created_at: current_timestamp(),
        contains_stacks,
        message: if valid {
            format!("Backup {} validation passed", backup_id)
        } else {
            format!("Backup {} is empty or malformed", backup_id)
        },
    })
}

/// Quiesce (pause) services before restore
pub async fn quiesce_services(stack_names: &[String], lxc_api_base: &str) -> Result<(), String> {
    let _ = lxc_api_base;

    let list = Command::new("docker")
        .args([
            "ps",
            "-q",
            "--filter",
            "label=com.homelab.backup.pause=true",
        ])
        .output()
        .map_err(|e| format!("failed to list pausable containers: {}", e))?;

    if !list.status.success() {
        return Err(String::from_utf8_lossy(&list.stderr).trim().to_string());
    }

    let ids: Vec<String> = String::from_utf8_lossy(&list.stdout)
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    for id in ids {
        let out = Command::new("docker")
            .args(["pause", &id])
            .output()
            .map_err(|e| format!("failed to pause {}: {}", id, e))?;
        if !out.status.success() {
            return Err(format!(
                "failed to pause {}: {}",
                id,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    for stack in stack_names {
        log_restore_event(format!("Pausing services for stack: {}", stack).as_str());
    }

    Ok(())
}

/// Restore storage data from backup
pub async fn restore_storage(
    stack_names: &[String],
    backup_id: &str,
    backup_root: &str,
    host_appdata_root: &str,
) -> Result<(), String> {
    let backup_base = Path::new(backup_root).join(backup_id);
    if !backup_base.exists() {
        return Err(format!(
            "backup root missing for restore: {}",
            backup_base.display()
        ));
    }

    let rsync_available = Command::new("which")
        .arg("rsync")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !rsync_available {
        return Err("rsync is required for restore execution but was not found".to_string());
    }

    for stack in stack_names {
        let source_path = backup_base.join(stack);
        let target_path = format!("{}/{}", host_appdata_root, stack);

        if !source_path.exists() {
            return Err(format!(
                "backup for stack '{}' not found at {}",
                stack,
                source_path.display()
            ));
        }

        std::fs::create_dir_all(&target_path)
            .map_err(|e| format!("failed to prepare target {}: {}", target_path, e))?;

        let source_with_trailing = format!("{}/", source_path.display());
        let sync = Command::new("rsync")
            .args(["-a", "--delete", &source_with_trailing, &target_path])
            .output()
            .map_err(|e| format!("failed to run rsync for '{}': {}", stack, e))?;

        if !sync.status.success() {
            return Err(format!(
                "restore failed for '{}': {}",
                stack,
                String::from_utf8_lossy(&sync.stderr).trim()
            ));
        }

        log_restore_event(
            format!(
                "Restoring stack '{}' from backup '{}' to {}",
                stack, backup_id, target_path
            )
            .as_str(),
        );
    }

    Ok(())
}

/// Sync applications after restore (restart containers + run post-restore hooks)
pub async fn sync_applications(
    stack_names: &[String],
    lxc_api_base: &str,
    skip_post_hooks: bool,
    gitops_root: &str,
) -> Result<(), String> {
    let _ = lxc_api_base;

    let list = Command::new("docker")
        .args(["ps", "-q", "--filter", "status=paused"])
        .output()
        .map_err(|e| format!("failed to list paused containers: {}", e))?;

    if !list.status.success() {
        return Err(String::from_utf8_lossy(&list.stderr).trim().to_string());
    }

    for id in String::from_utf8_lossy(&list.stdout)
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
    {
        let out = Command::new("docker")
            .args(["unpause", id])
            .output()
            .map_err(|e| format!("failed to unpause {}: {}", id, e))?;
        if !out.status.success() {
            return Err(format!(
                "failed to unpause {}: {}",
                id,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    for stack in stack_names {
        log_restore_event(format!("Resuming services for stack: {}", stack).as_str());

        let stack_root = Path::new(gitops_root).join("stacks").join(stack);
        if stack_root.exists() {
            let entries = std::fs::read_dir(&stack_root).map_err(|e| e.to_string())?;
            for entry in entries.flatten() {
                let app_dir = entry.path();
                if !app_dir.is_dir() {
                    continue;
                }
                let compose = app_dir.join("docker-compose.yml");
                if !compose.exists() {
                    continue;
                }

                let out = Command::new("docker")
                    .args(["compose", "up", "-d", "--remove-orphans"])
                    .current_dir(&app_dir)
                    .output()
                    .map_err(|e| {
                        format!(
                            "failed to run compose up for '{}': {}",
                            app_dir.display(),
                            e
                        )
                    })?;

                if !out.status.success() {
                    return Err(format!(
                        "compose up failed for '{}': {}",
                        app_dir.display(),
                        String::from_utf8_lossy(&out.stderr).trim()
                    ));
                }
            }
        }

        if !skip_post_hooks {
            let hook = stack_root.join("post-restore.sh");
            if hook.exists() {
                let out = Command::new("bash")
                    .arg(&hook)
                    .current_dir(&stack_root)
                    .output()
                    .map_err(|e| format!("failed to run post-restore hook: {}", e))?;
                if !out.status.success() {
                    return Err(format!(
                        "post-restore hook failed for '{}': {}",
                        stack,
                        String::from_utf8_lossy(&out.stderr).trim()
                    ));
                }
            }

            log_restore_event(format!("Running post-restore hooks for stack: {}", stack).as_str());
        }
    }

    Ok(())
}

/// Execute a full restore workflow
pub async fn execute_restore(
    request: &RestoreRequest,
    lxc_api_base: &str,
    backup_root: &str,
    host_appdata_root: &str,
) -> RestoreStatus {
    let operation_id = generate_operation_id();
    let mut status = RestoreStatus::new(operation_id.clone(), request);
    let gitops_root = std::env::var("GITOPS_REPO").unwrap_or_else(|_| "/opt/gitops".to_string());

    // Phase 1: Validate backup
    status.add_event_with_progress(
        RestorePhase::ValidatingBackup,
        &request.stack_names[0],
        10,
        "Validating backup integrity...",
    );

    match validate_backup(&request.backup_id, backup_root) {
        Ok(validation) => {
            if !validation.valid {
                status.add_error(
                    RestorePhase::Failed,
                    &request.stack_names[0],
                    "Backup validation failed",
                );
                return status;
            }
            status.add_event_with_progress(
                RestorePhase::ValidatingBackup,
                &request.stack_names[0],
                20,
                &format!("Backup valid: {} bytes", validation.size_bytes),
            );
        }
        Err(e) => {
            status.add_error(RestorePhase::Failed, &request.stack_names[0], &e);
            return status;
        }
    }

    // If verify_only, stop here
    if request.verify_only {
        status.mark_complete();
        return status;
    }

    // Phase 2: Quiesce services
    status.add_event_with_progress(
        RestorePhase::QuiescingServices,
        &request.stack_names[0],
        30,
        "Pausing containers...",
    );

    if let Err(e) = quiesce_services(&request.stack_names, lxc_api_base).await {
        status.add_error(RestorePhase::Failed, &request.stack_names[0], &e);
        return status;
    }

    status.add_event_with_progress(
        RestorePhase::QuiescingServices,
        &request.stack_names[0],
        40,
        "Services paused",
    );

    // Phase 3: Restore storage
    status.add_event_with_progress(
        RestorePhase::RestoringStorage,
        &request.stack_names[0],
        50,
        "Restoring data from backup...",
    );

    if let Err(e) = restore_storage(
        &request.stack_names,
        &request.backup_id,
        backup_root,
        host_appdata_root,
    )
    .await
    {
        status.add_error(RestorePhase::Failed, &request.stack_names[0], &e);
        // Try to resume services even on error to avoid paused workloads.
        let _ = sync_applications(&request.stack_names, lxc_api_base, true, &gitops_root).await;
        return status;
    }

    status.add_event_with_progress(
        RestorePhase::RestoringStorage,
        &request.stack_names[0],
        70,
        "Data restored successfully",
    );

    // Phase 4: Sync applications
    status.add_event_with_progress(
        RestorePhase::SyncingApplications,
        &request.stack_names[0],
        80,
        "Resuming services and running post-restore hooks...",
    );

    if let Err(e) = sync_applications(
        &request.stack_names,
        lxc_api_base,
        request.skip_post_hooks,
        &gitops_root,
    )
    .await
    {
        status.add_error(RestorePhase::Failed, &request.stack_names[0], &e);
        return status;
    }

    status.add_event_with_progress(
        RestorePhase::SyncingApplications,
        &request.stack_names[0],
        100,
        "Post-restore sync complete",
    );

    // Mark complete
    status.mark_complete();
    status
}

// Helper functions

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn generate_operation_id() -> String {
    format!(
        "restore-{}",
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    )
}

fn log_restore_event(message: &str) {
    eprintln!("[restore] {}", message);
}

fn dir_size_bytes(path: &Path) -> u64 {
    if !path.exists() {
        return 0;
    }

    let mut total = 0_u64;
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(_) => return 0,
    };

    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file() {
            if let Ok(meta) = entry.metadata() {
                total = total.saturating_add(meta.len());
            }
        } else if p.is_dir() {
            total = total.saturating_add(dir_size_bytes(&p));
        }
    }

    total
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, OnceLock};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn tmp_dir(name: &str) -> PathBuf {
        let base = std::env::temp_dir();
        let id = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = base.join(format!("homelab-restore-{}-{}", name, id));
        let _ = fs::create_dir_all(&path);
        path
    }

    fn write_executable(path: &Path, content: &str) {
        fs::write(path, content).expect("write executable");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).expect("chmod");
        }
    }

    #[test]
    fn test_restore_phase_display() {
        assert_eq!(RestorePhase::Queued.to_string(), "queued");
        assert_eq!(RestorePhase::Completed.to_string(), "completed");
        assert_eq!(RestorePhase::Failed.to_string(), "failed");
    }

    #[test]
    fn test_restore_status_creation() {
        let request = RestoreRequest::new("backup-123".to_string(), vec!["media".to_string()]);
        let status = RestoreStatus::new("op-123".to_string(), &request);

        assert_eq!(status.current_phase, RestorePhase::Queued);
        assert_eq!(status.overall_progress, 0);
        assert!(!status.success);
    }

    #[test]
    fn test_restore_status_add_event() {
        let request = RestoreRequest::new("backup-123".to_string(), vec!["media".to_string()]);
        let mut status = RestoreStatus::new("op-123".to_string(), &request);

        status.add_event_with_progress(
            RestorePhase::ValidatingBackup,
            "media",
            50,
            "Validating...",
        );

        assert_eq!(status.current_phase, RestorePhase::ValidatingBackup);
        assert_eq!(status.overall_progress, 50);
        assert_eq!(status.events.len(), 1);
    }

    #[test]
    fn test_restore_status_complete() {
        let request = RestoreRequest::new("backup-123".to_string(), vec!["media".to_string()]);
        let mut status = RestoreStatus::new("op-123".to_string(), &request);

        status.mark_complete();

        assert_eq!(status.current_phase, RestorePhase::Completed);
        assert_eq!(status.overall_progress, 100);
        assert!(status.success);
    }

    #[test]
    fn test_restore_request_scope() {
        let single = RestoreRequest::new("backup-123".to_string(), vec!["media".to_string()]);
        assert_eq!(single.scope, RestoreScope::Stack);

        let multiple = RestoreRequest::new(
            "backup-123".to_string(),
            vec!["media".to_string(), "download".to_string()],
        );
        assert_eq!(multiple.scope, RestoreScope::Environment);
    }

    #[tokio::test]
    async fn test_execute_restore_success_path() {
        let _guard = test_lock().lock().unwrap();
        let root = tmp_dir("success");
        let bin = root.join("bin");
        let backup_root = root.join("backups");
        let appdata_root = root.join("appdata");
        let gitops_root = root.join("gitops");

        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(backup_root.join("latest/media")).unwrap();
        fs::create_dir_all(&appdata_root).unwrap();
        fs::create_dir_all(&gitops_root).unwrap();
        fs::write(backup_root.join("latest/media/data.txt"), "ok").unwrap();

        write_executable(
            &bin.join("docker"),
            "#!/usr/bin/env bash\nif [[ \"$1\" == \"ps\" ]]; then exit 0; fi\nexit 0\n",
        );
        write_executable(
            &bin.join("rsync"),
            "#!/usr/bin/env bash\nsrc=\"$3\"\ndst=\"$4\"\nmkdir -p \"$dst\"\ncp -a \"$src\". \"$dst\"/\n",
        );
        write_executable(&bin.join("which"), "#!/usr/bin/env bash\nexit 0\n");

        let old_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", format!("{}:{}", bin.display(), old_path));
            std::env::set_var("GITOPS_REPO", gitops_root.display().to_string());
        }

        let request = RestoreRequest::new("latest".to_string(), vec!["media".to_string()]);
        let status = execute_restore(
            &request,
            "http://127.0.0.1:8080",
            backup_root.to_str().unwrap(),
            appdata_root.to_str().unwrap(),
        )
        .await;

        assert!(status.success);
        assert!(appdata_root.join("media/data.txt").exists());
    }

    #[tokio::test]
    async fn test_execute_restore_failure_triggers_resume_path() {
        let _guard = test_lock().lock().unwrap();
        let root = tmp_dir("failure");
        let bin = root.join("bin");
        let backup_root = root.join("backups");
        let appdata_root = root.join("appdata");
        let gitops_root = root.join("gitops");
        let marker = root.join("unpause.marker");

        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(backup_root.join("latest/media")).unwrap();
        fs::create_dir_all(&appdata_root).unwrap();
        fs::create_dir_all(&gitops_root).unwrap();

        let docker_script = format!(
            "#!/usr/bin/env bash\nif [[ \"$1\" == \"ps\" ]]; then echo cid1; exit 0; fi\nif [[ \"$1\" == \"unpause\" ]]; then echo done > \"{}\"; exit 0; fi\nexit 0\n",
            marker.display()
        );
        write_executable(&bin.join("docker"), &docker_script);
        write_executable(&bin.join("rsync"), "#!/usr/bin/env bash\nexit 1\n");
        write_executable(&bin.join("which"), "#!/usr/bin/env bash\nexit 0\n");

        let old_path = std::env::var("PATH").unwrap_or_default();
        unsafe {
            std::env::set_var("PATH", format!("{}:{}", bin.display(), old_path));
            std::env::set_var("GITOPS_REPO", gitops_root.display().to_string());
        }

        let request = RestoreRequest::new("latest".to_string(), vec!["media".to_string()]);
        let status = execute_restore(
            &request,
            "http://127.0.0.1:8080",
            backup_root.to_str().unwrap(),
            appdata_root.to_str().unwrap(),
        )
        .await;

        assert!(!status.success);
        assert!(marker.exists());
    }
}
