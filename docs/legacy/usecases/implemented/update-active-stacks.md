# Use Case: Update Active Stacks

**Tier:** CLIENT
**Status:** Implemented

---

## Overview

Update-active-stacks now shares the same batch queue path as deploy-active-stacks.

Trigger:

- u in Scaffolding tab

Behavior:

- Resolves deploy-enabled stacks
- Enqueues sync jobs
- Processes jobs sequentially through existing sync HTTP path

---

## Implementation Notes

The current implementation reuses the same sync endpoint and queue path for deploy/update requests.
This keeps behavior consistent and module-first while preserving room for future diff-aware update logic.

Core files:

- client-app/src/events.rs
- client-app/src/main.rs
- client-app/src/scaffold.rs
- client-app/src/app.rs
