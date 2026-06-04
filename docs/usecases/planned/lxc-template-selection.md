# Use Case: LXC Template Selection in Stack Wizard

**Tier:** CLIENT  
**Status:** Planned  
**Priority:** Low  
**Dependencies:** 04-client-stack-wizard-enhancements.md (must be implemented first)

---

## Problem Statement

Currently, the CLIENT stack creation wizard hardcodes the LXC template to `"debian-12-standard 12.12-1 amd64"`. This works for most use cases but lacks flexibility for:
- Different Debian versions (11, 13 in future)
- Ubuntu-based stacks (22.04, 24.04)
- Alpine Linux for minimal containers
- Testing/development environments

Users must manually edit `lxc-compose.yml` after creation to change templates.

---

## Desired Behavior

Add a template selection step to the CLIENT stack creation wizard:

```
┌─────────────────────────────────────────────────────────────┐
│ Step 2: Container Template                                  │
├─────────────────────────────────────────────────────────────┤
│ Select OS template:                                         │
│   ● debian-12-standard (Recommended)                        │
│   ○ debian-11-standard                                      │
│   ○ ubuntu-24.04-standard                                   │
│   ○ ubuntu-22.04-standard                                   │
│   ○ alpine-3.18-standard (Advanced)                         │
│                                                             │
│ ℹ Debian 12 is recommended for Docker workloads            │
└─────────────────────────────────────────────────────────────┘
```

### Features

1. **Dynamic Template Discovery**: Query Proxmox host for available templates
2. **Version Detection**: Parse template names to show versions (e.g., "12.12-1")
3. **Filtering**: Show only LXC templates (exclude VM templates)
4. **Recommendations**: Mark preferred templates based on use case
5. **Validation**: Verify selected template exists before provisioning

---

## Technical Requirements

### CLIENT Changes

**New Module**: `client-app/src/proxmox.rs`

Functions:
```rust
pub fn list_available_templates(host: &str) -> Result<Vec<Template>> {
    // Query Proxmox API: GET /api2/json/nodes/{node}/storage/{storage}/content
    // Filter by content type: vztmpl
}

pub struct Template {
    pub name: String,          // e.g., "debian-12-standard"
    pub version: String,       // e.g., "12.12-1"
    pub arch: String,          // e.g., "amd64"
    pub full_name: String,     // e.g., "debian-12-standard 12.12-1 amd64.tar.zst"
    pub size_mb: u64,
    pub os_type: OsType,       // Debian, Ubuntu, Alpine, etc.
    pub recommended: bool,
}

pub enum OsType {
    Debian,
    Ubuntu,
    Alpine,
    Other,
}
```

**Modified**: `client-app/src/events.rs`

```rust
fn handle_stack_creation_wizard(&mut self) -> Result<()> {
    // Step 1: Basic info
    let stack_name = self.prompt_stack_name()?;
    
    // Step 2: Template selection
    let templates = proxmox::list_available_templates(&self.proxmox_host)?;
    
    // Sort: recommended first, then by OS type and version
    let sorted_templates = sort_templates(templates);
    
    let template_choices: Vec<(String, String)> = sorted_templates
        .iter()
        .map(|t| {
            let label = if t.recommended {
                format!("{} {} (Recommended)", t.name, t.version)
            } else {
                format!("{} {}", t.name, t.version)
            };
            (t.full_name.clone(), label)
        })
        .collect();
    
    let selected_template = self.prompt_select("Container Template", &template_choices)?;
    
    // Rest of wizard...
}
```

### Proxmox API Integration

**Authentication**:
- Use existing Proxmox API credentials from CLIENT env
- Token-based auth: `PVEAPIToken=USER@REALM!TOKENID=UUID`

**API Endpoint**:
```
GET /api2/json/nodes/{node}/storage/local/content?content=vztmpl
```

**Response Parsing**:
```json
{
  "data": [
    {
      "content": "vztmpl",
      "ctime": 1234567890,
      "format": "tgz",
      "size": 123456789,
      "volid": "local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst"
    }
  ]
}
```

### Template Recommendations

**Recommended by default**:
- Latest Debian stable (currently 12)
- Latest Ubuntu LTS (currently 24.04)

**Advanced (not recommended)**:
- Alpine Linux (minimal, but requires more expertise)
- Older versions (Debian 11, Ubuntu 22.04)

**Criteria for recommendation**:
- Latest stable release
- Well-tested with Docker
- Good package availability
- Security updates available

---

## Validation

### Pre-Flight Checks

Before allowing provisioning:
1. Verify selected template exists on Proxmox host
2. Verify template is compatible with selected features (e.g., nesting)
3. Warn if template is outdated (>2 years old)
4. Warn if template is not recommended for Docker workloads

### Error Handling

- If Proxmox API unavailable, fall back to hardcoded list
- If template fetch fails, use default (debian-12-standard)
- Log all API errors for debugging

---

## lxc-compose.yml Updates

Store full template name for reproducibility:

```yaml
lxc:
  template: "debian-12-standard 12.12-1 amd64"  # Full name, not just "debian-12"
  unprivileged: true
  features:
    - "nesting=1"
```

---

## UI/UX Considerations

### Template Display

```
┌─────────────────────────────────────────────────────────────┐
│ Available Templates (5 found)                               │
├─────────────────────────────────────────────────────────────┤
│ ● Debian 12 (12.12-1) [Recommended]                         │
│   Size: 128 MB | Docker: ✓ | Updates: ✓                    │
│                                                             │
│ ○ Debian 11 (11.7-0)                                        │
│   Size: 125 MB | Docker: ✓ | Updates: ⚠ EOL 2026           │
│                                                             │
│ ○ Ubuntu 24.04 (24.04-0) [Recommended]                      │
│   Size: 145 MB | Docker: ✓ | Updates: ✓                    │
│                                                             │
│ ○ Ubuntu 22.04 (22.04-1)                                    │
│   Size: 142 MB | Docker: ✓ | Updates: ✓                    │
│                                                             │
│ ○ Alpine 3.18 (3.18-0) [Advanced]                           │
│   Size: 45 MB | Docker: ✓ | Updates: ✓                     │
└─────────────────────────────────────────────────────────────┘
```

### Keyboard Navigation

- Arrow keys: navigate
- Enter: select
- `/`: search/filter
- `i`: show template info
- `r`: refresh list
- Esc: cancel

---

## Implementation Phases

### Phase 1: Proxmox API Integration
- Implement `proxmox.rs` module
- Add template fetching from Proxmox API
- Parse and structure template data

### Phase 2: Wizard UI
- Add template selection step
- Implement sorting/filtering
- Add recommendations

### Phase 3: Validation
- Add pre-flight checks
- Implement fallback behavior
- Add error handling

### Phase 4: Testing
- Test with different Proxmox versions
- Test with offline Proxmox
- Test with missing templates

---

## Testing Checklist

- [ ] Fetch templates from Proxmox successfully
- [ ] Parse template names correctly
- [ ] Sort templates (recommended first)
- [ ] Display templates in wizard
- [ ] Select non-default template
- [ ] Verify lxc-compose.yml contains full template name
- [ ] Test fallback when API unavailable
- [ ] Test with missing template on host

---

## Files to Create/Modify

**New files:**
- `client-app/src/proxmox.rs`

**Modified files:**
- `client-app/src/events.rs` - Add template selection
- `client-app/src/scaffold.rs` - Remove hardcoded template
- `client-app/Cargo.toml` - Add HTTP client dependency

---

## Dependencies

- HTTP client library (e.g., `reqwest`)
- JSON parsing (already have `serde_json`)
- Proxmox API credentials in CLIENT env

---

## Success Criteria

- ✅ CLIENT wizard shows available Proxmox templates
- ✅ Templates sorted with recommendations
- ✅ Selected template stored in lxc-compose.yml
- ✅ HOST provisions with selected template
- ✅ Graceful fallback on API errors
- ✅ Validation prevents invalid template selection

---

## Future Enhancements

- Template caching (avoid fetching every wizard run)
- Template version update notifications
- Custom template upload from CLIENT
- Template compatibility matrix (kernel versions, features)
