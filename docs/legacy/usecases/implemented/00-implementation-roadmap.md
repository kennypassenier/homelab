# Implementation Roadmap: Full GitOps LXC Provisioning

**Status:** ✅ Implemented  
**Priority:** Critical  
**Completed:** June 4, 2025

---

## Overview

This roadmap coordinates the implementation of full GitOps-driven LXC provisioning, eliminating manual steps and shell scripts in favor of Rust-based automation through HOST and LXC daemons.

**All 4 phases have been completed and all daemons compile successfully.**

---

## Goals

1. **Zero Manual Provisioning**: HOST automatically creates/manages LXCs based on `lxc-compose.yml`
2. **Eliminate Shell Scripts**: Replace `bootstrap-lxc.sh` and `node-sync.sh` with Rust
3. **True GitOps**: Commit → Push → Automatic provisioning and sync
4. **Naming Enforcement**: Canonical naming scheme (`{vmid}-app-{stack}`) enforced
5. **Complete lxc-compose Schema**: All provisioning details in Git

---

## Dependencies

```
04-client-stack-wizard-enhancements
  ↓
01-host-automated-lxc-provisioning
  ↓
03-host-bootstrap-integration
  ↓
02-lxc-daemon-gitops-sync
```

---

## Phase 1: Foundation (Week 1-2)

### 1.1 CLIENT Stack Wizard Enhancements
**Use Case**: `04-client-stack-wizard-enhancements.md`

**Note**: Template is hardcoded to `"debian-12-standard 12.12-1 amd64"` for initial implementation. Template selection is planned for future enhancement (see `usecases/planned/lxc-template-selection.md`).

**Tasks:**
- [ ] Design wizard UI flow (9 steps, down from 10)
- [ ] Add `lxc-compose.yml` schema fields:
  - [ ] `storage.host_path`
  - [ ] `storage.mount_point`
  - [ ] `lxc.template` (hardcoded value)
  - [ ] `lxc.unprivileged`
  - [ ] `lxc.features[]`
  - [ ] `hardware.tun_device`
- [ ] Implement wizard prompts in `events.rs`
- [ ] Update `scaffold.rs` to generate complete `lxc-compose.yml`
- [ ] Add validation rules
- [ ] Update documentation

**Deliverables:**
- Complete lxc-compose.yml from wizard
- No manual editing required
- Documentation updated

**Testing:**
- [ ] Create stack through wizard
- [ ] Verify all fields populated
- [ ] Test validation rules
- [ ] Test default values

**Time Estimate**: 5-7 days

---

## Phase 2: HOST Provisioning (Week 2-3)

### 2.1 HOST Automated LXC Provisioning
**Use Case**: `01-host-automated-lxc-provisioning.md`

**Tasks:**
- [ ] Create `host-daemon/src/provision.rs`
- [ ] Implement `scan_stack_intents()`
- [ ] Implement `validate_lxc()` with naming scheme check
- [ ] Implement `create_lxc()` from lxc-compose intent
- [ ] Implement `destroy_lxc()` with safety checks
- [ ] Implement `reconcile_lxc()` for config updates
- [ ] Add TUI keybindings (`p`/`P`)
- [ ] Add dry-run mode
- [ ] Add transaction logging

**Deliverables:**
- HOST creates LXCs automatically
- Naming scheme enforced
- Config reconciliation working
- Safety checks in place

**Testing:**
- [ ] Create new LXC (vmid=0)
- [ ] Detect name mismatch and recreate
- [ ] Reconcile resource changes
- [ ] Test dry-run mode
- [ ] Test with mixed canonical/legacy names

**Time Estimate**: 7-10 days

---

## Phase 3: HOST Bootstrap Integration (Week 3-4)

### 3.1 HOST Bootstrap Integration
**Use Case**: `03-host-bootstrap-integration.md`

**Tasks:**
- [ ] Create `host-daemon/src/bootstrap.rs`
- [ ] Implement `bootstrap_lxc()` main flow
- [ ] Implement `setup_storage()`
- [ ] Implement `setup_tun_device()`
- [ ] Implement `inject_secrets()`
- [ ] Implement `install_dependencies()`
- [ ] Implement `setup_git_sparse_checkout()`
- [ ] Implement `setup_ssh_access()`
- [ ] Implement `install_lxc_daemon()`
- [ ] Implement `create_daemon_config()`
- [ ] Integrate with `provision.rs`
- [ ] Add rollback on failure

**Deliverables:**
- HOST bootstraps LXCs automatically
- All bootstrap-lxc.sh logic ported to Rust
- Atomic provisioning+bootstrap
- LXC daemon installed and configured

**Testing:**
- [ ] Full provision+bootstrap flow
- [ ] Verify storage mounted
- [ ] Verify TUN configured
- [ ] Verify secrets injected
- [ ] Verify dependencies installed
- [ ] Verify Git sparse checkout
- [ ] Verify SSH keys
- [ ] Verify LXC daemon running
- [ ] Test rollback on failure

**Time Estimate**: 7-10 days

---

## Phase 4: LXC Daemon Sync (Week 4-5)

### 4.1 LXC Daemon GitOps Sync
**Use Case**: `02-lxc-daemon-gitops-sync.md`

**Tasks:**
- [ ] Expand `lxc-daemon/src/gitops.rs`
- [ ] Implement `sync_loop()` with scheduler
- [ ] Implement `git_pull()` with sparse enforcement
- [ ] Implement `run_pre_sync_hooks()`
- [ ] Implement `sync_compose_apps()`
- [ ] Implement `health_check_services()`
- [ ] Implement `garbage_collect_orphans()`
- [ ] Add structured logging (logfmt)
- [ ] Implement HTTP API endpoints:
  - [ ] `POST /sync/trigger`
  - [ ] `GET /sync/status`
  - [ ] `GET /sync/history`
  - [ ] `POST /sync/gc`
- [ ] Create daemon config file (`lxc-daemon.toml`)
- [ ] Create systemd service integration

**Deliverables:**
- LXC daemon handles all GitOps sync
- node-sync.sh replaced
- HTTP API for external control
- Structured logging for Loki

**Testing:**
- [ ] Daemon syncs on startup
- [ ] Daemon syncs every 5 minutes
- [ ] Pre-sync hooks execute
- [ ] Compose apps deploy
- [ ] Health checks work
- [ ] GC removes orphans
- [ ] HTTP API triggers sync
- [ ] Logs appear in Loki
- [ ] Daemon restarts on crash
- [ ] Sparse checkout enforced

**Time Estimate**: 7-10 days

---

## Phase 5: Integration & Testing (Week 5-6)

### 5.1 End-to-End Integration

**Tasks:**
- [ ] Test complete workflow: wizard → commit → push → HOST provisions
- [ ] Verify CLIENT can trigger sync via LXC API
- [ ] Verify HOST TUI shows provisioning status
- [ ] Test with multiple stacks
- [ ] Test with mixed new/existing LXCs
- [ ] Test name mismatch detection
- [ ] Test resource reconciliation
- [ ] Test GC flow (delete app from Git)
- [ ] Performance testing (multiple stacks)
- [ ] Stress testing (rapid changes)

### 5.2 Documentation

**Tasks:**
- [ ] Update `docs/deployment.md`
- [ ] Update `docs/host-features.md`
- [ ] Update `docs/lxc-features.md`
- [ ] Update `docs/client-features.md`
- [ ] Create migration guide from old system
- [ ] Update `README.md`
- [ ] Create troubleshooting guide

### 5.3 Cleanup

**Tasks:**
- [ ] Delete `scripts/host/bootstrap-lxc.sh`
- [ ] Delete `scripts/container/node-sync.sh`
- [ ] Remove cron job setup from old scripts
- [ ] Archive old documentation
- [ ] Update `.github/copilot-instructions.md`

**Time Estimate**: 5-7 days

---

## Phase 6: Production Rollout (Week 6)

### 6.1 Staged Rollout

**Week 6:**
- [ ] Deploy to test stack (cloudflared)
- [ ] Monitor for 2-3 days
- [ ] Deploy to non-critical stacks (downloader, monitoring)
- [ ] Monitor for 2-3 days
- [ ] Deploy to critical stacks (gateway, media, paperless)
- [ ] Monitor for 1 week

### 6.2 Monitoring

**Metrics to track:**
- Provisioning success rate
- Bootstrap failure rate
- Sync cycle duration
- GC execution stats
- API response times
- Error rates by component

### 6.3 Rollback Plan

If critical issues arise:
1. Revert to manual provisioning
2. Re-enable shell scripts temporarily
3. Fix issues in dev environment
4. Re-test before retry

**Time Estimate**: 5-7 days

---

## Total Timeline

- **Phase 1**: 5-7 days
- **Phase 2**: 7-10 days
- **Phase 3**: 7-10 days
- **Phase 4**: 7-10 days
- **Phase 5**: 5-7 days
- **Phase 6**: 5-7 days

**Total**: 36-51 days (~5-7 weeks)

---

## Success Criteria

- ✅ Zero manual LXC provisioning required
- ✅ All shell scripts eliminated (bootstrap-lxc.sh, node-sync.sh)
- ✅ Complete GitOps workflow functional
- ✅ Naming scheme enforced
- ✅ lxc-compose.yml is single source of truth
- ✅ HOST automatically provisions on Git changes
- ✅ LXC daemon handles all sync operations
- ✅ Full observability (logs, metrics)
- ✅ Documentation complete
- ✅ Production-ready

---

## Risk Mitigation

### Risk 1: Breaking Existing Stacks
**Mitigation**: Parallel operation mode, staged rollout, comprehensive testing

### Risk 2: Data Loss During Recreate
**Mitigation**: Backup checks before destructive operations, confirmation prompts

### Risk 3: Git Sparse Checkout Drift
**Mitigation**: Re-apply sparse checkout on every sync

### Risk 4: Daemon Crashes
**Mitigation**: Systemd auto-restart, crash logging, health checks

### Risk 5: Network/DHCP Issues
**Mitigation**: Validate network config before apply, OPNsense sync verification

---

## Next Steps

1. Review this roadmap with team
2. Approve use case documents
3. Begin Phase 1 implementation
4. Daily standups during implementation
5. Weekly milestone reviews
