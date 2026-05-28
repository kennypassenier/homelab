# Use Case: Delete Stack

**Tier:** CLIENT
**Status:** Implemented

---

## Overview

Stack deletion is implemented in the CLIENT with exact-name confirmation and a shared module path.

Trigger:

- Scaffolding actions -> delete stack

Behavior:

- Requires exact stack name confirmation
- Removes stacks/<stack_name>/ if it exists
- Creates a GitOps commit via existing helper
- Reloads stack list in the UI

---

## Shared Module

Reusable primitive:

- delete_stack(stack_name)

File:

- client-app/src/stack_features.rs

---

## Notes

This implementation currently focuses on Git-scaffold deletion in the CLIENT flow.
Host/LXC destruction orchestration can be layered later on the same module-first path.
