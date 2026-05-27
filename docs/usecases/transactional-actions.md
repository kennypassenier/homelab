# Use Case: Transactional Actions

**Tier:** CLIENT (orchestration) + LXC (phase tracking + rollback) + HOST (cleanup)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

A "transactional action" is any multi-phase operation that must either complete fully or roll back to a clean starting state if any phase fails midway. The homelab system treats stack creation, deletion, updates, and provisioning as transactions with well-defined compensation steps.

This document defines:
1. The **phase ledger** pattern used to track progress.
2. The **compensation steps** (rollback) for each operation type.
3. The **idempotency guarantees** that make re-runs safe.

---

## 2. Phase Ledger Pattern

Every long-running operation maintains a **phase ledger** — an in-memory (and optionally persisted) record of completed phases. If the operation fails at phase N, the system can:
- Retry from phase N (for idempotent phases).
- Execute compensation steps from phase N backwards to phase 1 (rollback).

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseLedger {
    pub operation: String,          // e.g., "add_stack"
    pub stack_name: String,
    pub phases: Vec<PhaseRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhaseRecord {
    pub phase_id: u8,
    pub name: String,               // e.g., "git_push"
    pub status: PhaseStatus,        // Pending | InProgress | Completed | Failed
    pub completed_at: Option<DateTime<Utc>>,
    pub compensation: Option<String>, // Description of rollback action
}
```

The ledger is persisted to `~/.local/share/homelab/transactions/<operation>-<stack>-<timestamp>.json` on the CLIENT filesystem. If the CLIENT crashes mid-operation, the user can inspect the ledger and resume or roll back manually.

---

## 3. Add Stack — Transaction Phases and Rollbacks

| Phase | Name | Compensation (Rollback) |
|---|---|---|
| 1 | `scaffold_git_files` | `std::fs::remove_dir_all("stacks/<stack_name>/")` |
| 2 | `git_push` | `git revert HEAD --no-edit && git push` |
| 3 | `host_create_appdata_dir` | `POST /api/appdata/purge { stack_name }` to HOST |
| 4 | `host_pct_create` | `POST /api/lxc/destroy { vmid, purge_appdata: false }` to HOST |
| 5 | `host_pct_set_mounts` | Included in phase 4 compensation (`pct destroy`) |
| 6 | `host_pct_start` | `pct stop <vmid>` + phase 4 compensation |
| 7 | `host_bootstrap_exec` | `pct destroy <vmid> --purge` + phase 3 compensation |
| 8 | `lxc_first_sync` | LXC daemon retries (3x); if all fail → alert + leave LXC running; user must investigate |
| 9 | `ssh_config_update` | Restore `~/.ssh/config` from in-memory backup |

**Rollback trigger:** Any phase returning a non-`Ok` result triggers reverse execution of all completed phases, in reverse order. Rollback failures are logged but do not block subsequent compensation steps.

---

## 4. Delete Stack — Transaction Phases and Rollbacks

Delete stack is intentionally **non-reversible** after Phase 5 (`pct destroy`). The pre-delete backup (optional) is the only recovery mechanism.

| Phase | Name | Compensation |
|---|---|---|
| 1 | `user_confirmation` | Cancel (no state changed) |
| 2 | `optional_backup` | N/A — backup is additive |
| 3 | `lxc_graceful_shutdown` | N/A — containers can be restarted |
| 4 | `host_pct_destroy` | **No rollback.** LXC is gone. |
| 5 | `host_purge_appdata` | **No rollback.** Data is gone. |
| 6 | `git_remove_push` | Manual `git revert` (but data on host is gone) |
| 7 | `ssh_config_cleanup` | Restore `~/.ssh/config` from in-memory backup |

---

## 5. Update Active Stacks — Transaction Phases and Rollbacks

| Phase | Name | Compensation |
|---|---|---|
| 1 | `git_fetch` | N/A (read-only) |
| 2 | `lxc_git_pull` | LXC daemon aborts sync; previous state remains deployed |
| 3 | `setup_sh_hook` | LXC daemon aborts sync; previous state remains deployed |
| 4 | `secrets_refresh` | LXC daemon aborts sync; previous `.env` remains; apps continue running |
| 5 | `docker_compose_pull` | No rollback needed; only images pulled, nothing running changed |
| 6 | `docker_compose_up` | Rollback: restart each updated container from previous image ID (see `post-deploy-actions.md`) |
| 7 | `gc_orphaned_apps` | **No rollback** once containers are stopped; GC is a destructive final step |

---

## 6. LXC Daemon Sync Idempotency Guarantees

The LXC daemon's sync loop is designed so every phase can be safely re-run:

| Phase | Idempotency Mechanism |
|---|---|
| `git pull` | `--ff-only` + `git reset --hard origin/main`; never creates merge commits |
| `setup.sh` | Must be idempotent by design (Docker `network inspect || network create`) |
| `docker compose pull` | Image pull is always safe to repeat |
| `docker compose up -d` | Compose compares desired vs running state; only restarts changed containers |
| `docker compose up --remove-orphans` | Removes containers that disappeared from compose; safe to repeat |

---

## 7. CLIENT: Transaction Resume UI

If the CLIENT detects an incomplete transaction ledger on startup (status file with `InProgress` phases), it shows a recovery modal:

```
⚠  Incomplete Operation Detected

Operation:   add_stack
Stack:       media
Last phase:  host_pct_start (completed)
Failed at:   host_bootstrap_exec

Options:
  [ Retry from failed phase ]   — attempt phase 7 again
  [ Roll back ]                 — execute compensation steps 6 → 1
  [ Dismiss ]                   — leave as-is; investigate manually
```

Choosing "Retry from failed phase" re-executes only from the failed phase, skipping all already-completed phases.

---

## 8. HOST: Atomic State for Infrastructure Operations

The HOST daemon uses Rust `Drop` guards to guarantee cleanup even on panic:

```rust
struct LxcCreationGuard {
    vmid: u32,
    appdata_path: PathBuf,
    created: bool,
}

impl Drop for LxcCreationGuard {
    fn drop(&mut self) {
        if self.created && std::thread::panicking() {
            // Compensation: destroy LXC and remove appdata on panic
            let _ = Command::new("pct").args(["destroy", &self.vmid.to_string(), "--purge"]).status();
            let _ = std::fs::remove_dir_all(&self.appdata_path);
        }
    }
}
```

The guard is `mem::forget`-ted on success, so the cleanup only runs if the thread panics between creation and successful completion.

---

## 9. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `add-stack.md` | Transaction phases defined in Section 3 |
| `delete-stack.md` | Transaction phases defined in Section 4 |
| `update-active-stacks.md` | Transaction phases defined in Section 5 |
| `post-deploy-actions.md` | Container rollback mechanism (phase 6 in Section 5) |
| `error-handling-fail-closed.md` | When to abort vs. when to compensate |
