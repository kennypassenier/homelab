# Use Case: Post-Deploy Actions

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

After successful sync results, CLIENT now performs a lightweight post-deploy summary step.

Behavior:

- computes app count for stack
- checks for missing docker-compose.yml files
- emits structured logfmt-style post_deploy summary events
- emits warn-level post_deploy event if layout drift is detected

---

## Shared Primitive

Implemented in:

- client-app/src/stack_features.rs

Function:

- post_deploy_summary(stack_name)

---

## Integration

Wired in:

- client-app/src/main.rs

The summary is emitted immediately after sync success processing.
