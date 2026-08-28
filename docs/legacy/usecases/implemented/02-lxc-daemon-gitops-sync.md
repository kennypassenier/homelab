# Use Case: LXC Daemon GitOps Sync (Replace node-sync.sh)

**Tier:** LXC  
**Status:** ✅ Implemented  
**Priority:** Critical  
**Completed:** June 4, 2025  
**Dependencies:** 01-host-automated-lxc-provisioning.md

---

## Problem Statement

Currently, GitOps sync inside LXC containers uses `node-sync.sh` (Bash script) triggered by cron:
- Hard to maintain and test
- No structured error handling
- Limited observability
- Cron-based scheduling is inflexible
- Shell script doesn't integrate with LXC daemon

The LXC daemon (`lxc-daemon`) already exists in Rust but doesn't handle GitOps sync — it should be the primary sync orchestrator.

---

## Desired Behavior

LXC daemon should handle all GitOps sync operations internally:

1. **Continuous Sync Loop**: Built-in scheduler (configurable interval, default 30min)
2. **Git Operations**: Pull latest changes from GitOps repo
3. **Pre-Sync Hooks**: Execute stack `pre-sync.sh` scripts for secrets export
4. **Docker Compose**: Pull images and deploy/update containers
5. **Health Checks**: Verify containers started successfully
6. **Garbage Collection**: Remove orphaned apps no longer in Git
7. **Structured Logging**: Emit logfmt for Loki ingestion via Promtail
8. **HTTP API**: Expose endpoints for HOST/CLIENT to trigger manual sync

---

## Technical Requirements

### LXC Daemon Changes

**New Module**: `lxc-daemon/src/gitops.rs` (already exists, expand functionality)

Functions:
- `sync_loop(interval: Duration)` - Continuous background sync
- `git_pull() -> Result<()>` - Git fetch + pull with sparse checkout validation
- `run_pre_sync_hooks(stack_path: &Path) -> Result<Vec<HookResult>>` - Execute pre-sync.sh
- `sync_compose_apps(stack_path: &Path) -> Result<Vec<AppSyncResult>>` - Compose operations
- `garbage_collect_orphans(stack_path: &Path) -> Result<Vec<GcResult>>` - Remove deleted apps
- `health_check_services(app_path: &Path) -> Result<HealthStatus>` - Verify containers running

**Modified Module**: `lxc-daemon/src/api.rs`

New HTTP endpoints:
- `POST /sync/trigger` - Trigger immediate sync (CLIENT/HOST invoked)
- `GET /sync/status` - Get current sync state
- `GET /sync/history` - Get last N sync results
- `POST /sync/gc` - Trigger manual garbage collection

### Configuration

**New file**: `/etc/homelab/lxc-daemon.toml` (or env vars)

```toml
[sync]
interval_seconds = 1800       # 30 minutes
gitops_repo = "/opt/gitops"
stack_name = "media"

[git]
remote = "origin"
branch = "main"
sparse_checkout = true        # Always constrain to stacks/<stack>/

[logging]
format = "logfmt"             # For Loki ingestion
output = "/var/log/lxc-daemon-sync.log"
level = "info"

[api]
listen = "0.0.0.0:8080"
auth_token = "${LXC_API_TOKEN}"
```

### Logging Format

Replace Bash `log_sync()` with Rust structured logging:

```rust
info!(
    ts = %Utc::now().to_rfc3339(),
    level = "info",
    stack = %self.stack_name,
    app = %app_name,
    msg = "Syncing app"
);
```

Output (logfmt):
```
ts=2026-06-04T12:34:56Z level=info stack=media app=jellyfin msg="Syncing app"
ts=2026-06-04T12:35:02Z level=warn stack=media app=sonarr msg="Service not running after deploy"
```

### Git Operations

Preserve sparse checkout enforcement:
```rust
fn git_pull(&self) -> Result<()> {
    // Re-apply sparse checkout to prevent scope drift
    Command::new("git")
        .args(["sparse-checkout", "set", &format!("stacks/{}", self.stack_name)])
        .current_dir(&self.gitops_repo)
        .output()?;
    
    // Reset any local changes (GitOps single source of truth)
    Command::new("git")
        .args(["reset", "--hard", "origin/main"])
        .current_dir(&self.gitops_repo)
        .output()?;
    
    // Pull latest
    Command::new("git")
        .args(["pull", "origin", "main"])
        .current_dir(&self.gitops_repo)
        .output()?;
    
    Ok(())
}
```

### Pre-Sync Hook Execution

```rust
fn run_pre_sync_hooks(&self, stack_path: &Path) -> Result<Vec<HookResult>> {
    let mut results = Vec::new();
    
    for entry in WalkDir::new(stack_path).max_depth(2) {
        let entry = entry?;
        if entry.file_name() == "pre-sync.sh" {
            info!("Running pre-sync hook: {}", entry.path().display());
            
            let output = Command::new("bash")
                .arg(entry.path())
                .current_dir(entry.path().parent().unwrap())
                .output()?;
            
            results.push(HookResult {
                path: entry.path().to_path_buf(),
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            });
            
            if !output.status.success() {
                error!("Pre-sync hook failed: {}", entry.path().display());
                return Err(anyhow!("Pre-sync hook failed"));
            }
        }
    }
    
    Ok(results)
}
```

### Compose Operations

```rust
fn sync_compose_apps(&self, stack_path: &Path) -> Result<Vec<AppSyncResult>> {
    let mut results = Vec::new();
    
    for entry in WalkDir::new(stack_path).max_depth(2) {
        let entry = entry?;
        let path = entry.path();
        
        if path.file_name() == Some(OsStr::new("docker-compose.yml")) 
            || path.file_name() == Some(OsStr::new("compose.yaml")) {
            
            let app_dir = path.parent().unwrap();
            let app_name = app_dir.file_name().unwrap().to_string_lossy();
            
            info!(stack = %self.stack_name, app = %app_name, "Syncing app");
            
            // Pull images
            let pull_output = Command::new("docker")
                .args(["compose", "pull", "-q"])
                .current_dir(app_dir)
                .output()?;
            
            // Deploy
            let up_output = Command::new("docker")
                .args(["compose", "up", "-d", "--remove-orphans"])
                .current_dir(app_dir)
                .output()?;
            
            // Health check
            let health = self.health_check_services(app_dir)?;
            
            if health.has_exited_services {
                warn!(
                    stack = %self.stack_name,
                    app = %app_name,
                    msg = "Services not running after deploy",
                    exited = %health.exited_services.join(", ")
                );
            }
            
            results.push(AppSyncResult {
                app_name: app_name.to_string(),
                success: up_output.status.success(),
                health,
            });
        }
    }
    
    Ok(results)
}
```

### Garbage Collection

```rust
fn garbage_collect_orphans(&self, stack_path: &Path) -> Result<Vec<GcResult>> {
    let mut results = Vec::new();
    let appdata_path = Path::new("/appdata").join(&self.stack_name);
    
    if !appdata_path.exists() {
        return Ok(results);
    }
    
    for entry in std::fs::read_dir(&appdata_path)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        
        let app_name = entry.file_name().to_string_lossy().to_string();
        let git_app_path = stack_path.join(&app_name);
        
        // If app no longer exists in Git, it's an orphan
        if !git_app_path.exists() {
            warn!(
                stack = %self.stack_name,
                app = %app_name,
                msg = "App no longer in Git, removing container and data"
            );
            
            // Stop and remove containers
            let down_output = Command::new("docker")
                .args(["compose", "-p", &app_name, "down"])
                .output();
            
            // Fallback: stop by container name
            if down_output.is_err() || !down_output.unwrap().status.success() {
                Command::new("docker").args(["stop", &app_name]).output().ok();
                Command::new("docker").args(["rm", &app_name]).output().ok();
            }
            
            // Remove data directory
            std::fs::remove_dir_all(entry.path())?;
            
            info!(stack = %self.stack_name, app = %app_name, "Removed orphaned container and data");
            
            results.push(GcResult {
                app_name,
                removed: true,
            });
        }
    }
    
    Ok(results)
}
```

---

## Migration Path

### Phase 1: Parallel Operation
- LXC daemon implements sync logic
- Keep `node-sync.sh` cron job active
- Compare outputs, verify correctness

### Phase 2: Daemon Primary
- LXC daemon becomes primary sync mechanism
- Disable cron job (comment out in `/etc/cron.d/gitops-sync`)
- Monitor for issues

### Phase 3: Complete Removal
- Delete `scripts/container/node-sync.sh`
- Remove cron setup from bootstrap process
- Update documentation

---

## Bootstrap Integration

**Modified**: `host-daemon/src/provision.rs`

When creating new LXC, instead of installing cron job:

```rust
fn setup_daemon_service(&self, vmid: u32) -> Result<()> {
    // Install LXC daemon binary
    self.install_lxc_daemon(vmid)?;
    
    // Create systemd service
    let service_content = format!(r#"
[Unit]
Description=Homelab LXC GitOps Daemon
After=network.target docker.service
Requires=docker.service

[Service]
Type=simple
EnvironmentFile=/root/.env
ExecStart=/usr/local/bin/lxc-daemon --config /etc/homelab/lxc-daemon.toml
Restart=always
RestartSec=10
StandardOutput=append:/var/log/lxc-daemon.log
StandardError=append:/var/log/lxc-daemon.log

[Install]
WantedBy=multi-user.target
"#);
    
    self.pct_push_file(vmid, "/etc/systemd/system/lxc-daemon.service", &service_content)?;
    self.pct_exec(vmid, "systemctl daemon-reload")?;
    self.pct_exec(vmid, "systemctl enable lxc-daemon")?;
    self.pct_exec(vmid, "systemctl start lxc-daemon")?;
    
    Ok(())
}
```

---

## HTTP API Examples

### Trigger Manual Sync (from CLIENT)
```bash
curl -X POST http://10.10.10.104:8080/sync/trigger \
  -H "Authorization: Bearer ${LXC_API_TOKEN}"
```
Note: While automatic sync runs every 30 minutes, CLIENT can trigger immediate sync via API.
Response:
```json
{
  "status": "started",
  "sync_id": "sync-20260604-123456",
  "message": "Sync triggered successfully"
}
```

### Get Sync Status
```bash
curl http://10.10.10.104:8080/sync/status \
  -H "Authorization: Bearer ${LXC_API_TOKEN}"
```

Response:
```json
{
  "state": "running",
  "current_phase": "compose_sync",
  "started_at": "2026-06-04T12:34:56Z",
  "last_completed": "2026-06-04T12:30:00Z",
  "apps_synced": 4,
  "apps_total": 6
}
```

---

## Files to Create/Modify

**Modified files:**
- `lxc-daemon/src/gitops.rs` - Expand sync logic
- `lxc-daemon/src/api.rs` - Add sync endpoints
- `lxc-daemon/src/main.rs` - Add sync loop
- `host-daemon/src/provision.rs` - Replace cron with systemd service
- `client-app/src/ui.rs` - Add "Trigger Sync" button

**Deprecated files:**
- `scripts/container/node-sync.sh` - Delete after Phase 3

**Documentation:**
- `docs/lxc-features.md` - Document daemon sync
- `docs/deployment.md` - Update bootstrap instructions

---

## Testing Checklist

- [ ] LXC daemon syncs on startup
- [ ] LXC daemon syncs every 5 minutes
- [ ] Pre-sync hooks execute correctly
- [ ] Compose apps deploy successfully
- [ ] Health checks detect failed services
- [ ] Garbage collection removes orphaned apps
- [ ] HTTP API triggers manual sync
- [ ] Structured logs appear in Loki
- [ ] Daemon restarts on crash (systemd)
- [ ] Git sparse checkout enforced

---

## Success Criteria

- ✅ LXC daemon replaces node-sync.sh completely
- ✅ All sync operations in Rust with proper error handling
- ✅ Structured logging for observability
- ✅ HTTP API for external control
- ✅ No cron jobs required
- ✅ Daemon auto-starts via systemd
