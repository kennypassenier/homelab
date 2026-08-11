# GitOps LXC Provisioning - Implementation Complete ✅

**Date:** June 4, 2025  
**Status:** Backend Implementation Complete, Ready for Testing

---

## What's Been Implemented

### ✅ Phase 1: CLIENT Schema Expansion
- **File:** `client-app/src/scaffold.rs`
- **Changes:** Expanded StackConfig from 15 to 21 fields
- **New Fields:**
  - `storage.host_path`, `storage.mount_point`
  - `lxc.template`, `lxc.unprivileged`, `lxc.features[]`
  - `hardware.tun_device`
- **Status:** Schema complete, TUI wizard deferred (manual YAML editing for now)

### ✅ Phase 2: HOST Automated Provisioning
- **File:** `host-daemon/src/provision.rs` (580+ lines)
- **Features:**
  - Whitelist-only approach (scans `stacks/*/lxc-compose.yml` only)
  - `scan_stack_intents()` - discovers managed containers
  - `validate_lxc()` - checks existence, naming, config drift
  - `create_lxc()` - provisions new containers
  - `destroy_lxc()` - **with hostname validation safety checks**
  - `reconcile_lxc()` - updates CPU/memory/autostart without recreate
  - `plan_provisioning_changes()` - dry-run preview
  - `apply_provisioning_changes()` - execute changes
- **Keybindings:** `r` (preview), `R` (apply)
- **Safety:** Multiple layers to prevent touching non-GitOps containers

### ✅ Phase 3: HOST Bootstrap Integration
- **File:** `host-daemon/src/bootstrap.rs` (560+ lines)
- **Replaces:** `bootstrap-lxc.sh` shell script
- **13-Step Flow:**
  1. ✅ Setup storage (`/opt/appdata/{stack}`)
  2. ✅ Setup TUN device (if required)
  3. ✅ Inject secrets (INFISICAL_* vars)
  4. ✅ Install dependencies (Docker, git, curl, jq, Infisical CLI)
  5. ✅ Setup Git sparse checkout (`stacks/{stack}`)
  6. ✅ Setup SSH access (fetch keys from GitHub)
  7. ✅ Install LXC daemon binary
  8. ✅ Create daemon config (`/etc/homelab/lxc-daemon.toml`)
  9. ✅ Create systemd service
  10. ✅ Enable/start service
  11. ✅ Wait for daemon ready
  12. ✅ Verify sync working
  13. ✅ Cleanup temp files
- **Integration:** Called automatically from `provision.rs` after `create_lxc()`

### ✅ Phase 4: LXC Daemon GitOps Sync
- **File:** `lxc-daemon/src/gitops.rs`
- **Replaces:** `node-sync.sh` shell script
- **Features:**
  - ✅ 30-minute sync intervals (configurable via `FAILSAFE_SYNC_INTERVAL_SECS=1800`)
  - ✅ Git sparse checkout enforcement
  - ✅ Pre-sync hooks (`pre-sync.sh` instead of `setup.sh`)
  - ✅ Docker Compose pull + up for all apps
  - ✅ Health checks (basic - container running)
  - ✅ Garbage collection (auto-remove orphaned apps)
  - ✅ Structured logging (logfmt for Loki)
- **HTTP API:** (Planned, not yet implemented)
  - `POST /sync/trigger`
  - `GET /sync/status`
  - `GET /sync/history`

---

## Safety Guarantees

### 🔒 Whitelist-Only Approach
- **ONLY** manages containers in `stacks/*/lxc-compose.yml`
- **NEVER** scans all VMs/LXCs on Proxmox host
- **IGNORES** all non-GitOps containers completely

### 🔒 Explicit Opt-Out
```yaml
host_management:
  managed: false  # Skip this stack
```

### 🔒 Pre-Destroy Hostname Validation
Before destroying any container:
1. Read actual hostname from `pct config {vmid}`
2. Verify it matches expected pattern (`{vmid}-app-{stack}` or `lxc-{stack}`)
3. **ABORT** if name doesn't match (prevents accidental deletion)

**See:** [`docs/SAFETY.md`](../docs/SAFETY.md) for full details

---

## Environment Variables

### HOST Daemon (`.env` or `/etc/homelab/host-daemon.toml`)
```bash
# Required
GITOPS_REPO=/opt/gitops
GITOPS_REPO_URL=https://github.com/kennypassenier/homelab.git
GITHUB_PAT=ghp_your_github_personal_access_token
GITHUB_USERNAME=kennypassenier

# Secrets (injected into LXCs)
INFISICAL_TOKEN=st.xxx
INFISICAL_PROJECT_ID=xxx
INFISICAL_ENVIRONMENT=prod

# Optional
LXC_DAEMON_URL=https://github.com/.../lxc-daemon/releases/latest/download/LXC
```

**See:** [`config/.env.example`](../config/.env.example)

### LXC Daemon (created by HOST bootstrap)
```bash
# Set by HOST
STACK_NAME=media

# GitOps
GITOPS_REPO_URL=https://github.com/kennypassenier/homelab.git
GITOPS_REPO_TOKEN=ghp_xxx

# Sync
FAILSAFE_SYNC_INTERVAL_SECS=1800  # 30 minutes
HEARTBEAT_TTL_SECS=180            # 3 minutes

# Secrets (injected by HOST)
INFISICAL_TOKEN=st.xxx
INFISICAL_PROJECT_ID=xxx
INFISICAL_ENVIRONMENT=prod

# Optional
LXC_API_TOKEN=secret  # For HTTP API
```

**See:** [`config/.env.example`](../config/.env.example)

---

## Compilation Status

All three daemons compile successfully:

```bash
$ cargo check --all
   Compiling client-app...
   Compiling host-daemon...
   Compiling lxc-daemon...
    Finished `dev` profile in 10.31s
```

**Minor warnings remaining (cosmetic only):**
- Unused variables in provision.rs
- Unused fields in structs

---

## What's NOT Implemented (Deferred/Planned)

### 🔮 CLIENT TUI Wizard
- **Current:** Manual YAML editing of `lxc-compose.yml`
- **Planned:** Step-by-step TUI wizard in CLIENT
- **Status:** Schema is ready, UI implementation deferred
- **Workaround:** Copy `docs/examples/lxc-compose.example.yml` and edit

### 🔮 LXC Daemon HTTP API
- **Current:** No external control API
- **Planned:** REST endpoints for sync control, status, history
- **Status:** Deferred
- **Workaround:** Manual `systemctl restart lxc-daemon@{stack}`

### 🔮 LXC Template Selection
- **Current:** Hardcoded to `"debian-12-standard 12.12-1 amd64"`
- **Planned:** Support Alpine, Ubuntu, etc.
- **Status:** Tracked in `usecases/planned/lxc-template-selection.md`

---

## Testing Checklist

Before deploying to production Proxmox host:

### Pre-Flight Safety
- [ ] Review `docs/SAFETY.md`
- [ ] Verify non-GitOps containers exist on host
- [ ] Test dry-run mode (`r` key in HOST daemon)
- [ ] Verify `destroy_lxc()` hostname validation works

### Phase 1: Manual lxc-compose.yml
- [ ] Create `stacks/test/lxc-compose.yml` manually
- [ ] Set `vmid: 900` (safe test ID)
- [ ] Set `hostname: 900-app-test`
- [ ] Commit and push

### Phase 2: HOST Provisioning
- [ ] Run HOST daemon
- [ ] Press `r` to preview (should show "CREATE: test")
- [ ] Press `R` to apply
- [ ] Verify container created: `pct list | grep 900`
- [ ] Verify hostname: `pct config 900 | grep hostname`

### Phase 3: HOST Bootstrap
- [ ] Verify storage: `ls -la /opt/appdata/test`
- [ ] Verify TUN: `cat /etc/pve/lxc/900.conf | grep lxc.cgroup2`
- [ ] Verify secrets: `pct exec 900 -- cat /root/.env | grep INFISICAL`
- [ ] Verify dependencies: `pct exec 900 -- which docker git infisical`
- [ ] Verify Git: `pct exec 900 -- ls /opt/gitops/stacks/test`
- [ ] Verify SSH: `pct exec 900 -- cat /root/.ssh/authorized_keys`
- [ ] Verify daemon: `pct exec 900 -- systemctl status lxc-daemon`

### Phase 4: LXC Daemon Sync
- [ ] Add test app: `stacks/test/nginx/docker-compose.yml`
- [ ] Commit and push
- [ ] Wait 30 minutes OR `systemctl restart lxc-daemon@test`
- [ ] Verify app running: `pct exec 900 -- docker ps`
- [ ] Check logs: `pct exec 900 -- journalctl -u lxc-daemon -f`

### Safety Tests
- [ ] Create non-GitOps container: `pct create 999 debian-12-standard --hostname unmanaged`
- [ ] Verify HOST ignores it (`r` key should not list it)
- [ ] Set wrong hostname on test container: `pct set 900 -hostname wrong-name`
- [ ] Verify HOST refuses to destroy it (name validation abort)

---

## Rollback Plan

If issues arise:

1. **Stop HOST daemon:** `pkill -f HOST`
2. **Stop LXC daemons:** `for id in $(pct list | awk 'NR>1 {print $1}'); do pct exec $id -- systemctl stop lxc-daemon 2>/dev/null; done`
3. **Revert to shell scripts:**
   - Use `scripts/host/bootstrap-lxc.sh` for new containers
   - Use `scripts/container/node-sync.sh` for sync (legacy)
4. **Set all stacks to unmanaged:**
   ```bash
   for f in stacks/*/lxc-compose.yml; do
     yq -i '.host_management.managed = false' "$f"
   done
   ```

---

## Next Steps

1. **Test on non-production Proxmox host first**
2. **Verify safety guarantees work as expected**
3. **Monitor LXC daemon logs during first sync cycles**
4. **Implement HTTP API for better observability** (optional)
5. **Build CLIENT TUI wizard** (quality-of-life improvement)
6. **Add LXC template selection** (future enhancement)

---

## Documentation

- **Architecture:** [`docs/architecture.md`](../docs/architecture.md)
- **Safety:** [`docs/SAFETY.md`](../docs/SAFETY.md)
- **lxc-compose.yml:** [`docs/lxc-compose-format.md`](../docs/lxc-compose-format.md)
- **Example:** [`docs/examples/lxc-compose.example.yml`](../docs/examples/lxc-compose.example.yml)
- **Use Cases:** [`docs/usecases/implemented/`](../docs/usecases/implemented/)

---

## Summary

✅ **Backend infrastructure is complete and ready for testing.**  
✅ **All safety guarantees implemented.**  
✅ **Environment variable templates created.**  
✅ **Documentation updated.**  

**Next:** Deploy to test Proxmox host and validate real-world behavior.
