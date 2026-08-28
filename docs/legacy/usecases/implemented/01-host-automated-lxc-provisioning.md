# Use Case: HOST Automated LXC Provisioning from GitOps

**Tier:** HOST  
**Status:** ✅ Implemented  
**Priority:** Critical  
**Completed:** June 4, 2025  
**Dependencies:** None

---

## Problem Statement

Currently, LXC provisioning requires manual steps:
1. Manually run `pct create` or use Proxmox UI to create container
2. Manually run `bootstrap-lxc.sh` to configure the container
3. Manual verification of naming scheme compliance

This violates GitOps principles — the `lxc-compose.yml` file contains all necessary information, but HOST doesn't act on it automatically.

---

## Desired Behavior

HOST daemon should automatically provision and reconcile LXC containers based on `lxc-compose.yml` intent:

1. **Scan Phase**: On startup and periodically (configurable interval, default 30min), HOST scans all `stacks/*/lxc-compose.yml` files
2. **Validation Phase**: For each stack with `host_management.managed=true`:
   - Check if VMID exists in Proxmox
   - Validate naming scheme matches `{vmid}-app-{stack}` (canonical) or `lxc-{stack}` (legacy tolerated)
   - Validate configuration matches intent
3. **Reconciliation Phase**:
   - If VMID doesn't exist → **Create** new LXC with correct config
   - If VMID exists but wrong name → **Destroy and recreate** with correct config
   - If VMID exists with correct name but config drift → **Update** config
   - If VMID exists and matches → **No action**

---

## Technical Requirements

### Input: lxc-compose.yml

All provisioning parameters must be present in `lxc-compose.yml`:

```yaml
version: 1
stack_name: "media"
vmid: 104
hostname: "104-app-media"  # Canonical naming
hwaddr: "02:42:ac:11:34:7a"

deploy:
  enabled: false

network:
  bridge: "vmbr0"
  ip_mode: "dhcp-reserved"
  reserved_ipv4: "10.10.10.104"

boot:
  autostart: true
  order: 90

resources:
  cores: 2
  memory_mb: 2048
  disk_gb: 32

storage:
  host_path: "/opt/appdata/media"  # NEW: explicit host storage path
  mount_point: "/appdata"          # NEW: container mount point

lxc:
  template: "debian-12-standard"   # NEW: container template
  unprivileged: true               # NEW: security mode (default true)
  features:                        # NEW: LXC features
    - "nesting=1"                  # For Docker
    
hardware:
  tun_device: false                # NEW: auto-detect and enable if needed
  gpu:
    enabled: false
    profile: null
    target_app: null

host_management:
  managed: true
```

### Missing Fields in Current Schema

The following fields need to be added to CLIENT stack creation wizard:
- `storage.host_path` (default: `/opt/appdata/{stack}`)
- `storage.mount_point` (default: `/appdata`)
- `lxc.template` (dropdown: debian-11/12, ubuntu-22.04/24.04, alpine-3.18)
- `lxc.unprivileged` (checkbox, default: true)
- `lxc.features` (checkboxes: nesting for Docker, keyctl, fuse, etc.)
- `hardware.tun_device` (auto-detect from compose files OR manual override)

### HOST Daemon Changes

**New Module**: `host-daemon/src/provision.rs`

Functions:
- `scan_stack_intents() -> Vec<StackIntent>` - Read all lxc-compose.yml
- `validate_lxc(vmid: u32, intent: &StackIntent) -> ValidationResult` - Check name/config
- `create_lxc(intent: &StackIntent) -> Result<()>` - Create new container
- `destroy_lxc(vmid: u32) -> Result<()>` - Destroy container
- `reconcile_lxc(vmid: u32, intent: &StackIntent) -> Result<()>` - Update config
- `apply_provisioning_changes(dry_run: bool) -> Vec<ProvisionAction>` - Orchestrate

### HOST TUI Changes

Add new keybindings to Hardware tab:
- `p` / `P` - Preview/Apply LXC provisioning reconciliation

### Safety Features

1. **Dry-run mode**: Preview all actions before applying
2. **Name validation**: Refuse to destroy LXC if name doesn't match expected pattern (prevents accidental deletion of unrelated containers)
3. **Backup check**: Warn if destroying LXC without recent backup
4. **Confirmation prompt**: Require explicit confirmation for destructive actions
5. **Transaction log**: Write all provisioning actions to `/var/log/host-provision.log`

---

## Implementation Phases

### Phase 1: Read-Only Scanning
- Implement `scan_stack_intents()` and `validate_lxc()`
- Display validation results in HOST TUI
- No write operations

### Phase 2: Creation Flow
- Implement `create_lxc()` for new containers
- Test with `vmid=0` (not provisioned) stacks

### Phase 3: Reconciliation Flow
- Implement `reconcile_lxc()` for config updates
- Implement `destroy_lxc()` for name/config mismatches
- Add safety checks and confirmations

### Phase 4: Automated Mode
- Add periodic reconciliation loop (default: 5min)
- Add CLI flag `--provision-mode=manual|auto`
- Integration testing

---

## Example Reconciliation Output

```
PROVISION reconcile mode=preview stack_count=6

PROVISION [cloudflared] OK vmid=101 name=101-app-cloudflared config=match
PROVISION [downloader] CREATE vmid=102 name=102-app-downloader reason=not_exist
PROVISION [gateway] RECREATE vmid=103 current_name=lxc-gateway expected_name=103-app-gateway reason=name_mismatch
PROVISION [media] UPDATE vmid=104 name=104-app-media drift=cores:1->2,memory:1024->2048
PROVISION [monitoring] SKIP reason=host_management.managed=false
PROVISION [paperless] OK vmid=106 name=106-app-paperless config=match

Summary: 2 OK, 1 CREATE, 1 RECREATE, 1 UPDATE, 1 SKIP
```

---

## Files to Create/Modify

**New files:**
- `host-daemon/src/provision.rs`

**Modified files:**
- `host-daemon/src/main.rs` - Add provisioning keybindings
- `host-daemon/src/app.rs` - Add provisioning state
- `client-app/src/scaffold.rs` - Add missing lxc-compose fields
- `client-app/src/events.rs` - Update stack creation wizard
- `docs/lxc-compose-format.md` - Document new fields
- `docs/host-features.md` - Document provisioning feature

---

## Testing Checklist

- [ ] Create new stack with `vmid=0`, verify HOST creates it
- [ ] Rename existing LXC manually, verify HOST detects and recreates
- [ ] Change resources in lxc-compose, verify HOST updates
- [ ] Set `host_management.managed=false`, verify HOST skips
- [ ] Delete stack from Git, verify HOST doesn't destroy LXC (safety)
- [ ] Test dry-run mode doesn't modify anything
- [ ] Test with mix of canonical and legacy names

---

## Success Criteria

- ✅ HOST automatically provisions new LXCs from lxc-compose.yml
- ✅ HOST enforces naming scheme (canonical preferred, legacy tolerated)
- ✅ HOST reconciles config drift (resources, network, storage)
- ✅ No manual `pct create` or `bootstrap-lxc.sh` required
- ✅ Full GitOps workflow: commit lxc-compose.yml → push → HOST provisions
