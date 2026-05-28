# Use Case: Add Core App to Stack

**Tier:** CLIENT
**Status:** Implemented

---

## 1. Overview

Core app scaffolding is implemented as a reusable operation.

Trigger:
- Scaffolding tab hotkey c

Flow:
1. Detect missing core apps.
2. Scaffold only missing ones.
3. Commit and push.
4. If stack is active, queue deploy sync.

---

## 2. Implemented Core Apps

- promtail
- watchtower
- traefik

Each scaffold creates expected app directories and compose/config files where applicable.

---

## 3. Shared Module

Implemented via client-app/src/stack_features.rs:

- add_missing_core_apps(stack)
- AddCoreAppsResult

This keeps core-app behavior reusable across future workflows.
