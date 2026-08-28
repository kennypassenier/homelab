# Use Case: Delete Core App from Stack

**Tier:** CLIENT
**Status:** Implemented

---

## 1. Overview

Core app deletion now has a dedicated shared code path.

Core apps:

- promtail
- watchtower
- traefik

The delete flow still requires exact-name confirmation in the modal before removal.

---

## 2. Implemented Behavior

- Core app detection is centralized.
- Core app delete uses a dedicated function.
- App and app-config directories are removed when present.
- lxc-compose mount metadata is cleaned up.
- Commit message uses explicit core-app wording.

---

## 3. Shared Module

Implemented in client-app/src/stack_features.rs:

- is_core_app(app)
- delete_core_app_from_stack(stack, app)

Non-core delete remains separate in delete_app_from_stack.
