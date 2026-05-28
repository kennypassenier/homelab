# Use Case: Activate Stack

**Tier:** CLIENT (local state + stack config)
**Status:** Implemented

---

## 1. Overview

Activation is intentionally simple:

- The CLIENT updates stack-level lxc-compose.yml.
- It sets deploy.enabled=true for the selected stack.
- The deploy command only runs when deploy.enabled=true.

This makes activation idempotent, GitOps-friendly, and easy to extend in later features.

---

## 2. Source of Truth

File:

- stacks/<stack_name>/lxc-compose.yml

Field:

- deploy.enabled

State mapping:

- false -> stack inactive for deploy command
- true -> stack active for deploy command

---

## 3. Flow

1. User selects a stack in Scaffolding.
2. User presses a.
3. CLIENT ensures lxc-compose.yml exists for the stack.
4. CLIENT writes deploy.enabled=true.
5. User presses s to queue deploy.
6. CLIENT checks deploy.enabled before queueing.

If deploy.enabled is false, deploy is blocked with an in-UI status message.

---

## 4. Idempotency

- Re-activating an already active stack keeps deploy.enabled=true.
- Repeated activate actions do not create duplicate resources.

---

## 5. Implementation Notes

Implemented in CLIENT:

- Reusable lxc-compose helper module in scaffold logic:
  - ensure_lxc_compose(stack)
  - is_stack_deploy_enabled(stack)
  - set_stack_deploy_enabled(stack, enabled)
- Activation hotkey in Scaffolding:
  - a -> set deploy.enabled=true
- Deploy guard:
  - s only queues sync when deploy.enabled=true

---

## 6. Related Docs

- docs/lxc-compose-format.md
- docs/examples/lxc-compose.example.yml
