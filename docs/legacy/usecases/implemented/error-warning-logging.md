# Use Case: Error, Warning, and Logging

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

CLIENT now emits structured logfmt-style lines for key sync lifecycle events.

Implemented events include:

- sync dispatch
- sync result
- deploy gate warnings/errors
- fail-closed pre-sync validation errors

---

## Canonical Helper

Implemented in:

- client-app/src/app.rs

Helpers:

- logfmt(component, level, stack, phase, msg, error)
- push_client_logfmt(level, stack, phase, msg, error)

---

## Integration Points

- client-app/src/main.rs
- client-app/src/events.rs

These paths now use a centralized formatter instead of ad-hoc plain text for critical operation logs.
