# Use Case: Host Storage Operations

**Tier:** HOST  
**Status:** Implemented

---

## Implemented Scope

HOST now provides active storage inspection and preflight visibility:

- inspects stack storage health under host appdata root
- classifies health (`healthy`, `warning`, `critical`) using existence + write checks
- validates bind mount prerequisites through dedicated preflight checks
- renders stack storage status in the HOST Storage tab

Storage behavior is idempotent and safe for repeated checks.

---

## Files

- host-daemon/src/storage.rs
- host-daemon/src/main.rs
