# Use Case: Deploy Active Stacks

**Tier:** CLIENT
**Status:** Implemented

---

## Overview

Batch deploy/update of active stacks is implemented as a queue-based CLIENT flow.

Triggers in Scaffolding tab:

- D (batch deploy active stacks)
- u (batch update active stacks)

Both keys enqueue deploy-enabled stacks for sync.

---

## Implemented Behavior

- Active stacks are discovered from lxc-compose deploy.enabled=true.
- Sync requests are queued in FIFO order.
- Main loop executes queued sync jobs sequentially.
- Duplicate queue entries are prevented.
- Status line shows queue outcomes.

Files:

- client-app/src/scaffold.rs
- client-app/src/app.rs
- client-app/src/main.rs
- client-app/src/events.rs
- client-app/src/ui.rs

---

## Shared Primitives

- list_deploy_enabled_stacks(stacks)
- sync_queue state in App

These primitives support future expansion to richer multi-stack orchestration.
