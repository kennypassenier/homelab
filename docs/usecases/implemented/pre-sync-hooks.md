# Use Case: Pre-Sync Hooks

**Tier:** CLIENT scaffold + CLIENT pre-flight validation
**Status:** Implemented

---

## Implemented Scope

Stack scaffold now includes a setup.sh pre-sync hook stub and validation guardrails.

Implemented behavior:

- create_stack scaffolds setup.sh with strict bash mode
- setup.sh is marked executable on unix targets
- pre-sync hook validation runs before sync queue actions

---

## Shared Primitives

Implemented in:

- client-app/src/stack_features.rs

Functions:

- create_stack(stack_name)
- validate_setup_hook(stack_name)

setup.sh scaffold defaults:

- #!/usr/bin/env bash shebang
- set -euo pipefail
- idempotent placeholder body

---

## Runtime Integration

Validation enforcement wired in:

- client-app/src/events.rs

This enables fail-closed behavior before selected or batch sync enqueue actions.
