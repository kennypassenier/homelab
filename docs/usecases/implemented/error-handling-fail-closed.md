# Use Case: Error Handling - Fail-Closed

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

Fail-closed guards are now enforced for stack sync queueing in the CLIENT:

- setup.sh pre-sync hook is validated before selected-stack deploy
- setup.sh pre-sync hook is validated before batch active deploy/update queueing
- invalid setup.sh blocks sync enqueue for selected stack
- invalid setup.sh skips affected stacks in batch mode

Validation is centralized in a shared module helper.

---

## Shared Primitive

Implemented in:

- client-app/src/stack_features.rs

Function:

- validate_setup_hook(stack_name)

Checks:

- shebang must be #!/usr/bin/env bash
- forbidden patterns for appdata directory creation
- legacy pre-sync.sh pattern guard

---

## Event Integration

Integrated in:

- client-app/src/events.rs

The UI status line and CLIENT log stream now report fail-closed abort reasons on validation failure.
