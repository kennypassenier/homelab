# Use Case: Add App to Stack

**Tier:** CLIENT
**Status:** Implemented

---

## 1. Overview

Adding an app is implemented through a shared stack feature module.

Flow:
1. Open app wizard from Scaffolding actions.
2. Enter app name and Docker image.
3. Select default services (Watchtower, Promtail, Traefik).
4. CLIENT scaffolds app files and stack mount metadata.
5. CLIENT commits and pushes changes.
6. If stack is active, deploy can be triggered immediately with s.

---

## 2. Implemented Behaviors

- Creates:
  - stacks/<stack>/<app>/docker-compose.yml
  - stacks/<stack>/<app>-config/.gitkeep
- Ensures stack-level lxc-compose.yml exists.
- Adds mount metadata for app config in lxc-compose.yml.
- Commits and pushes scaffold changes.
- Uses deploy.enabled as activation guard for deployment.

---

## 3. Shared Module

Implemented via client-app/src/stack_features.rs:

- add_app_to_stack(...)
- AddAppOptions for reusable defaults handling

This module is designed for reuse by future app-related use cases.
