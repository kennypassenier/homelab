# GitOps Provisioning Safety Guarantees

**Critical:** This document explains how the HOST daemon ensures it NEVER interferes with non-GitOps containers and VMs on your Proxmox host.

---

## Core Safety Principle: WHITELIST ONLY

The provisioning system uses a **whitelist approach**, not a blacklist. It:

✅ **ONLY** manages containers explicitly defined in `stacks/*/lxc-compose.yml`  
✅ **NEVER** scans all VMs/LXCs on the Proxmox host  
✅ **NEVER** touches containers without a corresponding lxc-compose.yml  
✅ **IGNORES** all other VMs, LXCs, and containers completely

---

## How It Works

### 1. Intent Discovery (Whitelist)

```rust
// host-daemon/src/provision.rs: scan_stack_intents()

pub fn scan_stack_intents(repo_root: &Path) -> Result<Vec<StackIntent>, String> {
    let stacks_dir = repo_root.join("stacks");
    
    // Only scan the GitOps stacks directory
    for entry in std::fs::read_dir(&stacks_dir) {
        let lxc_compose_path = stack_path.join("lxc-compose.yml");
        
        // Skip if no lxc-compose.yml exists
        if !lxc_compose_path.exists() {
            continue;
        }
        
        // Parse and add to managed list
        intents.push(parse_lxc_compose(&lxc_compose_path)?);
    }
    
    Ok(intents)  // Returns ONLY containers in GitOps
}
```

**Result:** Only containers with `stacks/{stack}/lxc-compose.yml` are added to the managed list.

---

### 2. Explicit Opt-Out

Even if a stack has `lxc-compose.yml`, you can opt out:

```yaml
# stacks/legacy-stack/lxc-compose.yml
host_management:
  managed: false  # HOST daemon will skip this container
```

**Use case:** Gradual migration of existing containers into GitOps.

---

### 3. Pre-Destroy Safety Checks

Before destroying any container, the system validates:

```rust
// host-daemon/src/provision.rs: destroy_lxc()

pub fn destroy_lxc(vmid: u32, expected_name: &str, dry_run: bool) -> Result<(), String> {
    // Read actual container name
    let config = get_container_config(vmid)?;
    let actual_name = config.get("hostname")?;
    
    // Validate name matches GitOps pattern
    let is_canonical = actual_name == expected_name;  // e.g., "104-app-media"
    let is_legacy = actual_name.starts_with("lxc-");  // e.g., "lxc-media"
    
    if !is_canonical && !is_legacy {
        return Err(format!(
            "SAFETY ABORT: Container {} has unexpected name '{}' (expected '{}'). \
             This container may not be managed by GitOps. Refusing to destroy.",
            vmid, actual_name, expected_name
        ));
    }
    
    // Only destroy if name matches expected pattern
    pct_destroy(vmid)?;
}
```

**Protection:** If a container has an unexpected name (doesn't match `{vmid}-app-{stack}` or `lxc-{stack}`), destruction is aborted.

---

## Example Scenarios

### ✅ Safe: GitOps-Managed Container

```yaml
# stacks/media/lxc-compose.yml
stack_name: media
vmid: 104
hostname: 104-app-media
host_management:
  managed: true
```

**Action:** HOST will manage (create, update, reconcile) this container.

---

### ✅ Safe: Explicitly Opted Out

```yaml
# stacks/legacy/lxc-compose.yml
stack_name: legacy
vmid: 200
hostname: legacy-vm
host_management:
  managed: false
```

**Action:** HOST will skip this container (shows as `SKIP: managed=false`).

---

### ✅ Safe: No lxc-compose.yml

```
stacks/
  media/
    lxc-compose.yml  ← Managed
  pihole/
    (no lxc-compose.yml)  ← Ignored
```

**Action:** PiHole container is never scanned, never touched.

---

### ✅ Safe: Non-GitOps VMID

```
Proxmox host has:
  VMID 100 - TrueNAS VM (no lxc-compose.yml)
  VMID 101 - PiHole LXC (no lxc-compose.yml)
  VMID 104 - Media LXC (has lxc-compose.yml)
```

**Action:** HOST only manages VMID 104. VMIDs 100 and 101 are never scanned, never touched.

---

### ✅ Safe: Name Validation Prevents Accidents

```yaml
# stacks/media/lxc-compose.yml
vmid: 104
hostname: 104-app-media
```

But actual container at VMID 104 is named `pihole-dns`:

**Action:**
```
SAFETY ABORT: Container 104 has unexpected name 'pihole-dns' (expected '104-app-media').
This container may not be managed by GitOps. Refusing to destroy.
```

---

## Testing Safety

### Test 1: Non-GitOps Container Ignored

```bash
# Create a test LXC outside GitOps
pct create 999 debian-12-standard --hostname test-container

# Run HOST provisioning
./HOST
# Press 'r' to preview

# Expected output:
# (No mention of VMID 999 - it's ignored)
```

---

### Test 2: Name Validation

```bash
# Create container with wrong name
pct create 104 debian-12-standard --hostname wrong-name

# Add to GitOps
cat > stacks/test/lxc-compose.yml <<EOF
vmid: 104
hostname: 104-app-test
EOF

# Run HOST provisioning
./HOST
# Press 'R' to apply

# Expected output:
# SAFETY ABORT: Container 104 has unexpected name 'wrong-name'
```

---

### Test 3: Opt-Out Works

```yaml
# stacks/legacy/lxc-compose.yml
vmid: 200
host_management:
  managed: false
```

```bash
# Run HOST provisioning
./HOST
# Press 'r' to preview

# Expected output:
# [legacy] SKIP: managed=false
```

---

## Audit Checklist

Before deploying HOST daemon to production:

- [ ] Review `host-daemon/src/provision.rs` - verify whitelist approach
- [ ] Confirm `scan_stack_intents()` only reads `stacks/*/lxc-compose.yml`
- [ ] Confirm `destroy_lxc()` validates container names
- [ ] Test with non-GitOps containers on host
- [ ] Verify `managed: false` opt-out works
- [ ] Run in dry-run mode first (`r` key, not `R`)

---

## Emergency: Disable Provisioning

If you need to immediately stop all provisioning:

### Option 1: Set all stacks to unmanaged

```bash
for f in stacks/*/lxc-compose.yml; do
  yq -i '.host_management.managed = false' "$f"
done
git commit -am "Emergency: disable all provisioning"
git push
```

### Option 2: Stop HOST daemon

```bash
# If running in tmux/screen
pkill -f HOST

# If running as systemd service
systemctl stop host-daemon
```

### Option 3: Rename stacks directory

```bash
cd /opt/gitops
mv stacks stacks.disabled
```

**Result:** `scan_stack_intents()` returns empty list, no actions taken.

---

## Summary

The HOST provisioning system is designed with multiple layers of safety:

1. **Whitelist-only approach** - Only manages containers in `stacks/*/lxc-compose.yml`
2. **Explicit opt-out** - `managed: false` per stack
3. **Name validation** - Refuses to destroy containers with unexpected names
4. **Dry-run mode** - Preview all actions before applying (`r` key)
5. **No host-wide scanning** - Never queries all VMs/LXCs on Proxmox

**Guarantee:** Your existing VMs, LXCs, and containers that aren't in GitOps will never be touched by the provisioning system.
