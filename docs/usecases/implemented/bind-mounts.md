# Use Case: Bind Mounts

**Tier:** CLIENT lxc-compose management
**Status:** Implemented (stack metadata layer)

---

## 1. Overview

Bind-mount metadata is now managed through shared lxc-compose helpers.

Implemented capabilities:

- Ensure lxc-compose.yml exists per stack.
- Add app config mount metadata idempotently.
- Remove app config mount metadata when app is deleted.

---

## 2. Mount Contract

For app <app> in stack <stack>:

- source: /opt/appdata/<stack>/<app>-config
- target: /appdata/<app>-config
- mount identity key: name=<app>-config

---

## 3. Shared Module APIs

Implemented via client-app/src/scaffold.rs:

- ensure_app_config_mount(stack, app)
- remove_app_config_mount(stack, app)

These are reusable for future HOST-side mount sync features.
