# Use Case: Delete App from Stack

**Tier:** CLIENT
**Status:** Implemented

---

## 1. Overview

Deleting a non-core app is implemented through a shared deletion primitive.

Flow:
1. Open delete-app confirmation modal from app entry.
2. Type exact app name.
3. Remove app directories from stack.
4. Remove mount metadata from lxc-compose.yml.
5. Commit and push.

---

## 2. Implemented Guards

- Core apps are protected in this flow.
- Core app deletion is blocked with a clear error path.

---

## 3. Shared Module

Implemented via client-app/src/stack_features.rs:

- delete_app_from_stack(stack, app)
- is_core_app(app)

This module-first approach keeps delete logic centralized and extensible.
