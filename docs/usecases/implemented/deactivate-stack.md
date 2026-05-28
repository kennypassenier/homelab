# Use Case: Deactivate Stack

**Tier:** CLIENT
**Status:** Implemented

---

## 1. Overview

Deactivation is the inverse of activation in the current architecture:

- CLIENT updates stack lxc-compose.yml.
- Sets deploy.enabled=false.
- Deployment command no longer runs for that stack.

---

## 2. Flow

1. Select stack in Scaffolding.
2. Press x.
3. CLIENT ensures lxc-compose.yml exists.
4. CLIENT writes deploy.enabled=false and clears activated_at.
5. Status line confirms deactivation.

---

## 3. Notes

This is non-destructive and idempotent.

- Repeating x keeps deploy.enabled=false.
- No stack/app files are deleted.

---

## 4. Shared APIs

Implemented via client-app/src/scaffold.rs:

- set_stack_deploy_enabled(stack, false)
