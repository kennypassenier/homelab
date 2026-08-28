# Use Case: CLIENT Stack Creation Wizard Enhancements

**Tier:** CLIENT  
**Status:** ✅ Implemented (Schema Complete, TUI Deferred)  
**Priority:** High  
**Completed:** June 4, 2025  
**Dependencies:** 01-host-automated-lxc-provisioning.md

---

## Problem Statement

The CLIENT stack creation wizard currently collects:
- Stack name
- CPU cores
- Memory (MiB)
- Disk (GiB)
- Boot autostart
- Boot order

But HOST automated provisioning (use case 01) requires additional fields in `lxc-compose.yml`:
- `lxc.template` - Container OS template
- `lxc.unprivileged` - Security mode
- `lxc.features` - LXC features (nesting, keyctl, etc.)
- `storage.host_path` - Host storage location
- `storage.mount_point` - Container mount point
- `hardware.tun_device` - TUN device passthrough

Without these fields, HOST cannot fully automate provisioning.

---

## Desired Behavior

CLIENT stack creation wizard should collect ALL information needed for HOST to provision LXC containers automatically, without user intervention after the wizard completes.

### Wizard Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Step 1: Basic Info                                          │
├─────────────────────────────────────────────────────────────┤
│ Stack name: media                                           │
│ Description: Media streaming and management                 │
│                                                             │
│ ℹ Container template: debian-12-standard (hardcoded)       │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 2: Resources                                           │
├─────────────────────────────────────────────────────────────┤
│ CPU cores: [2] (1-16)                                       │
│ Memory: [2048] MiB (512-16384, step 512)                    │
│ Root disk: [32] GiB (8-500)                                 │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 3: Storage Configuration                               │
├─────────────────────────────────────────────────────────────┤
│ Host storage path: [/opt/appdata/media]                     │
│ Container mount: [/appdata]                                 │
│                                                             │
│ ℹ Host path will be bind-mounted into the container        │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 4: Security & Features                                 │
├─────────────────────────────────────────────────────────────┤
│ [✓] Unprivileged container (Recommended)                    │
│ [✓] Docker nesting support                                  │
│ [ ] FUSE support                                            │
│ [ ] Keyctl support                                          │
│                                                             │
│ ⚠ Unprivileged containers are more secure but may need     │
│   additional UID/GID mapping for file access                │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 5: Hardware Passthrough                                │
├─────────────────────────────────────────────────────────────┤
│ TUN device (VPN):                                           │
│   ○ Auto-detect (scan compose files for /dev/net/tun)      │
│   ○ Force enable                                            │
│   ● Force disable                                           │
│                                                             │
│ GPU passthrough:                                            │
│   [ ] Enable GPU passthrough                                │
│   GPU profile: [intel_igpu ▼]                               │
│   Target app: [jellyfin ▼]                                  │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 6: Boot Policy                                         │
├─────────────────────────────────────────────────────────────┤
│ [✓] Auto-start on host boot                                 │
│ Boot order: [90] (lower starts first)                       │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 7: Network Configuration                               │
├─────────────────────────────────────────────────────────────┤
│ Network bridge: [vmbr0]                                     │
│ IP mode:                                                    │
│   ● DHCP with reservation (Recommended)                     │
│   ○ Static IP                                               │
│   ○ DHCP (no reservation)                                   │
│                                                             │
│ Reserved IPv4: [10.10.10.104]                               │
│ MAC address: [02:42:ac:11:34:7a] (auto-generated)          │
│                                                             │
│ ℹ DHCP reservation syncs to OPNsense automatically         │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 8: Apps to Scaffold                                    │
├─────────────────────────────────────────────────────────────┤
│ Select apps to auto-scaffold:                               │
│ [✓] promtail (monitoring)                                   │
│ [✓] watchtower (updates)                                    │
│ [ ] traefik (reverse proxy)                                 │
│ [ ] cloudflared (tunnel)                                    │
│                                                             │
│ Custom apps: [jellyfin, sonarr, radarr]                     │
└─────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────┐
│ Step 9: Summary                                             │
├─────────────────────────────────────────────────────────────┤
│ Stack: media                                                │
│ Template: debian-12-standard 12.12-1 amd64 (hardcoded)      │
│ Resources: 2 cores, 2048 MiB, 32 GiB                        │
│ Storage: /opt/appdata/media → /appdata                      │
│ Security: unprivileged, nesting                             │
│ Hardware: TUN=auto-detect, GPU=disabled                     │
│ Boot: autostart=true, order=90                              │
│ Network: DHCP reserved 10.10.10.104                         │
│ Apps: promtail, watchtower, jellyfin, sonarr, radarr        │
│                                                             │
│ ⚠ VMID will be assigned automatically by HOST               │
│ ⚠ Deploy is DISABLED until explicitly activated             │
│                                                             │
│ [Confirm] [Back] [Cancel]                                   │
└─────────────────────────────────────────────────────────────┘
```

---

## Technical Requirements

### CLIENT Changes

**Modified**: `client-app/src/scaffold.rs`

Add new functions:
```rust
pub struct StackWizardInput {
    // Existing
    pub stack_name: String,
    pub cpu_cores: u8,
    pub memory_mb: u32,
    pub disk_gb: u32,
    pub autostart: bool,
    pub startup_order: u32,
    
    // New
    // lxc_template hardcoded to "debian-12-standard 12.12-1 amd64" for now
    pub unprivileged: bool,
    pub features: Vec<String>,
    pub host_storage_path: String,
    pub mount_point: String,
    pub tun_device_mode: TunDeviceMode,
    pub bridge: String,
    pub ip_mode: IpMode,
    pub reserved_ipv4: Option<String>,
    pub apps_to_scaffold: Vec<String>,
}

pub enum TunDeviceMode {
    AutoDetect,
    ForceEnable,
    ForceDisable,
}

pub enum IpMode {
    DhcpReserved,
    Static,
    Dhcp,
}
```

**Modified**: `client-app/src/events.rs`

Update stack creation wizard to collect all fields:
```rust
fn handle_stack_creation_wizard(&mut self) -> Result<()> {
    // Step 1: Basic info (template hardcoded)
    let stack_name = self.prompt_stack_name()?;
    let description = self.prompt_description()?;
    let lxc_template = "debian-12-standard 12.12-1 amd64".to_string(); // Hardcoded
    
    // Step 2: Resources (existing)
    let cpu_cores = self.prompt_number("CPU cores (1-16)", 2, 1, 16)?;
    let memory_mb = self.prompt_number("Memory MiB (512-16384, step 512)", 2048, 512, 16384)?;
    let disk_gb = self.prompt_number("Root disk GiB (8-500)", 32, 8, 500)?;
    
    // Step 4: Storage
    let default_host_path = format!("/opt/appdata/{}", stack_name);
    let host_storage_path = self.prompt_string("Host storage path", &default_host_path)?;
    let mount_point = self.prompt_string("Container mount point", "/appdata")?;
    
    // Step 4: Security & features
    let unprivileged = self.prompt_confirm("Use unprivileged container? (Recommended)", true)?;
    let mut features = vec!["nesting=1".to_string()]; // Docker nesting always enabled
    if self.prompt_confirm("Enable FUSE support?", false)? {
        feat3res.push("fuse=1".to_string());
    }
    if self.prompt_confirm("Enable Keyctl support?", false)? {
        features.push("keyctl=1".to_string());
    }
    
    // Step 5: Hardware passthrough
    let tun_options = vec![
        (TunDeviceMode::AutoDetect, "Auto-detect from compose files"),
        (TunDeviceMode::ForceEnable, "Force enable"),
        (TunDeviceMode::ForceDisable, "Force disable"),
    ];
    let tun_device_mode = self.prompt_select("TUN device configuration", &tun_options)?;
    
    // GPU handled separately (existing flow)
    
    // Step 6: Boot policy (existing)
    let autostart = self.prompt_confirm("Auto-start on host boot?", true)?;
    let startup_order = self.prompt_number("Boot order (lower starts first)", 90, 0, 999)?;
    
    // Step 7: Network
    let bridge = self.prompt_string("Network bridge", "vmbr0")?;
    let ip_mode_options = vec![
        (IpMode::DhcpReserved, "DHCP with reservation (Recommended)"),
        (IpMode::Static, "Static IP"),
        (IpMode::Dhcp, "DHCP (no reservation)"),
    ];
    let ip_mode = self.prompt_select("IP mode", &ip_mode_options)?;
    
    let reserved_ipv4 = if matches!(ip_mode, IpMode::DhcpReserved | IpMode::Static) {
        Some(self.prompt_string("IP address", "10.10.10.104")?)
    } else {
        None
    };
    
    // Step 8: Apps to scaffold
    let default_apps = vec!["promtail", "watchtower"];
    let apps_to_scaffold = self.prompt_multiselect("Select default apps", &default_apps)?;
    let custom_apps = self.prompt_string("Custom apps (comma-separated)", "")?;
    let mut all_apps = apps_to_scaffold;
    all_apps.extend(custom_apps.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
    
    // Step 9: Summary & confirmation
    self.show_summary(&wizard_input)?;
    if !self.prompt_confirm("Create this stack?", true)? {
        return Ok(());
    }
    
    // Execute creation
    scaffold::create_stack_from_wizard(wizard_input)?;
    
    Ok(())
}
```

### lxc-compose.yml Generation

**Modified**: `client-app/src/scaffold.rs`

```rust
pub fn create_stack_from_wizard(input: StackWizardInput) -> Result<()> {
    // Generate MAC address deterministically
    let hwaddr = generate_deterministic_mac(&input.stack_name);
    
    // Hardcoded template
    let lxc_template = "debian-12-standard 12.12-1 amd64".to_string();
    
    let config = StackConfig {
        version: 1,
        stack_name: input.stack_name.clone(),
        vmid: 0, // HOST assigns VMID
        hostname: format!("0-app-{}", input.stack_name), // Placeholder, updated by HOST
        hwaddr,
        deploy_enabled: false,
        activated_at: None,
        
        // Network
        bridge: input.bridge,
        ip_mode: match input.ip_mode {
            IpMode::DhcpReserved => "dhcp-reserved",
            IpMode::Static => "static",
            IpMode::Dhcp => "dhcp",
        }.to_string(),
        reserved_ipv4: input.reserved_ipv4,
        
        // Boot
        autostart: input.autostart,
        startup_order: input.startup_order,
        
        // Resources
        cpu_cores: input.cpu_cores,
        memory_mb: input.memory_mb,
        disk_gb: input.disk_gb,
        
        // Storage (NEW)
        storage: StorageConfig {
            host_path: input.host_storage_path,
            mount_point: input.mount_point,
        },
        
        // LXC (NEW)
        lxc: LxcConfig {
            template: input.lxc_template,
            unprivileged: input.unprivileged,
            features: input.features,
        },
        
        // Hardware
        tun_device: match input.tun_device_mode {
            TunDeviceMode::AutoDetect => None, // HOST auto-detects
            TunDeviceMode::ForceEnable => Some(true),
            TunDeviceMode::ForceDisable => Some(false),
        },
        gpu: GpuConfig::default(),
        
        // Host management
        managed: true,
    };
    
    // Create stack directory
    std::fs::create_dir_all(format!("stacks/{}", input.stack_name))?;
    
    // Write lxc-compose.yml
    write_lxc_compose(&config)?;
    
    // Scaffold apps
    for app in input.apps_to_scaffold {
        scaffold_app(&input.stack_name, &app)?;
    }
    
    Ok(())
}
```

---

## lxc-compose.yml Format Updates

**Modified**: `docs/lxc-compose-format.md`

Add new required fields:

```yaml
storage:
  host_path: "/opt/appdata/media"
  mount_point: "/appdata"

lxc:
  template: "debian-12-standard"
  unprivileged: true
  features:
    - "nesting=1"

hardware:
  tun_device: null  # null=auto-detect, true=force, false=disable
```

---

## Default Values & Validation

### Defaults
- `lxc.template`: `"debian-12-standard 12.12-1 amd64"` (hardcoded for now, see planned use case for future template selection)
- `lxc.unprivileged`: `true`
- `lxc.features`: `["nesting=1"]`
- `storage.host_path`: `"/opt/appdata/{stack}"`
- `storage.mount_point`: `"/appdata"`
- `hardware.tun_device`: `null` (auto-detect)
- `network.bridge`: `"vmbr0"`
- `network.ip_mode`: `"dhcp-reserved"`

### Validation Rules
- Stack name: lowercase alphanumeric + hyphens, 3-32 chars
- CPU cores: 1-16
- Memory: 512-16384 MiB, multiple of 512
- Disk: 8-500 GiB
- Boot order: 0-999
- Host storage path: must start with `/`
- Mount point: must start with `/`
- IPv4: valid IP format if provided
- Template: hardcoded to debian-12-standard for now

---

## Files to Modify

**Modified files:**
- `client-app/src/scaffold.rs` - Add wizard fields and lxc-compose generation
- `client-app/src/events.rs` - Update wizard UI flow
- `client-app/src/ui.rs` - Add new prompt types (multiselect, etc.)
- `docs/lxc-compose-format.md` - Document new fields
- `docs/examples/lxc-compose.example.yml` - Update example

---

## Testing Checklist

- [ ] Wizard collects all required fields
- [ ] Defaults are sensible and secure
- [ ] lxc-compose.yml generated correctly
- [ ] HOST can provision from wizard output
- [ ] Validation prevents invalid input
- [ ] MAC address generation is deterministic
- [ ] Apps scaffold correctly
- [ ] Summary shows all selections

---

## Success Criteria

- ✅ CLIENT wizard collects ALL fields needed for HOST provisioning
- ✅ No manual editing of lxc-compose.yml required
- ✅ Sensible defaults for common use cases
- ✅ Validation prevents configuration errors
- ✅ GitOps workflow: wizard → commit → push → HOST provisions
