# Use Case: Uniform LXC Naming Scheme

**Tier:** CLIENT + DHCP integration  
**Status:** Implemented

---

## Implemented Scope

The CLIENT naming flow now supports canonical LXC names in the format:

- `vmid-app-<stack>`

Implementation highlights:

- scaffold now provides canonical and legacy naming helpers
- default `hostname` in stack `lxc-compose.yml` uses canonical naming
- stack config read fallback now resolves to canonical naming
- SSH/LXC API dispatch resolves aliases in migration-safe order:
  configured hostname -> canonical alias -> legacy alias
- DHCP ownership detection includes canonical, configured, and legacy hostnames
- Host Management mesh view shows configured hostname instead of hardcoded legacy format

---

## Migration Safety

- legacy aliases (`lxc-<stack>`) remain supported for compatibility
- canonical names are preferred for new and normalized stack config reads

---

## Files

- client-app/src/scaffold.rs
- client-app/src/main.rs
- client-app/src/opnsense.rs
- client-app/src/ui.rs
