# remove-app.sh

> Removes an app from Git with a double confirmation, then pushes to trigger automatic Garbage Collection on the next sync.

## Overview

`scripts/client/remove-app.sh` deletes an app's directory from the Git repository and pushes the commit. On the next [node-sync.sh](script-node-sync.md) run (~5 minutes), [Garbage Collection](gitops-flow.md) detects the missing folder, stops the container, and deletes the host data at `/opt/appdata/<STACK>/<APP>`.

**This operation is permanent.** Container data is deleted from the host — there is no undo beyond a Restic [backup restore](backups.md).

## Usage

```bash
./scripts/client/remove-app.sh
# or via menu:
./client.sh → Remove an App
```

## Interactive Flow

1. **Select stack** — picker of all stacks in `stacks/`
2. **Select app** — picker of all apps in the selected stack
3. **Confirmation summary** — red-bordered box showing exactly what will be stopped, removed, and deleted
4. **First confirmation** — `ui_confirm "Are you sure you want to proceed?"`
5. **Second confirmation** — must type the exact app name to proceed

## What Happens After Push

Within 5 minutes, [node-sync.sh](script-node-sync.md) runs in the LXC and:
1. Detects that `stacks/<STACK>/<APP>/` no longer exists in Git
2. Finds `/appdata/<STACK>/<APP>/` still on the host
3. Runs `docker compose -p <APP> down`
4. Deletes `/appdata/<STACK>/<APP>/` permanently

## Rollback Behaviour

A `trap cleanup_on_error EXIT` watches for unexpected failures. If the script fails after `git rm` but before `git push`, it runs:

```bash
git restore --staged "$APP_DIR"
git restore "$APP_DIR"
```

This restores the deleted files from Git, leaving the repository in its original state.

## Git Commit Message

```
feat(<STACK>): remove <APP> and trigger garbage collection
```

## Libraries Used

- [lib-stack.sh](lib-stack.md) — `require_repo_root`, `prompt_stack_selection`, `prompt_app_selection`
- [lib-ui.sh](lib-ui.md)

## See also

- [script-remove-stack.md](script-remove-stack.md)
- [GitOps Flow — Garbage Collection](gitops-flow.md)
- [script-node-sync.md](script-node-sync.md)
- [Backups](backups.md)
