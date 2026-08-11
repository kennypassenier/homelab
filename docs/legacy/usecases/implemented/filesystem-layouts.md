# Use Case: Filesystem Layouts

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

Filesystem layout enforcement is now implemented as pre-sync validation before deploy queueing.

Checks now enforced:

- stack directory exists
- lxc-compose.yml exists
- each discovered app has docker-compose.yml

If validation fails, deploy is blocked (selected deploy) or skipped (batch deploy/update).

---

## Shared Primitive

Implemented in:

- client-app/src/stack_features.rs

Function:

- validate_stack_filesystem_layout(stack_name)

---

## Integration

Enforced in:

- client-app/src/events.rs

This makes layout issues fail-closed before HTTP sync dispatch.
