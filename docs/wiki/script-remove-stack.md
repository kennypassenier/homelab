# remove-stack.sh

> Removes an entire stack from Git with double confirmation, then pushes to trigger Garbage Collection for all apps in the stack.

## Overview

`scripts/client/remove-stack.sh` deletes the entire `stacks/<STACK_NAME>/` directory from Git and pushes. Every app in the stack is decommissioned by [Garbage Collection](gitops-flow.md) on the next sync (~5 minutes): all containers are stopped and all host data at `/opt/appdata/<STACK>/` is deleted.

**This operation is permanent.** All container data for the entire stack is deleted.

## Usage

```bash
./scripts/client/remove-stack.sh
# or via menu:
./client.sh → Remove an entire Stack
```

## Interactive Flow

1. **Select stack** — picker of all stacks in `stacks/`
2. **App count** — the script counts apps in the stack so the user knows the blast radius
3. **Confirmation summary** — red-bordered box listing all `APP_COUNT` containers that will be stopped, removed, and deleted
4. **First confirmation** — `ui_confirm "Are you sure you want to proceed?"`
5. **Second confirmation** — must type the exact stack name to proceed

## Rollback Behaviour

Same as [remove-app.sh](script-remove-app.md): if `git push` fails, the trap restores the deleted files from Git via `git restore`.

## Git Commit Message

```
feat(core): remove stack <STACK_NAME> and trigger garbage collection
```

## Libraries Used

- [lib-stack.sh](lib-stack.md) — `require_repo_root`, `prompt_stack_selection`
- [lib-ui.sh](lib-ui.md)

## See also

- [script-remove-app.md](script-remove-app.md)
- [GitOps Flow — Garbage Collection](gitops-flow.md)
- [script-node-sync.md](script-node-sync.md)
