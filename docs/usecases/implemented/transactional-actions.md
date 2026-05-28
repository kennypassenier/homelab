# Use Case: Transactional Actions

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

A persisted transaction ledger is now used for stack create and stack delete operations.

Tracked operations:

- add_stack
- delete_stack

Tracked phases:

- scaffold_git_files or delete_stack_scaffold
- git_push

The ledger records in_progress, completed, and failed phase states with timestamps.

---

## Ledger Storage

Path:

- .client-state/transactions/

Format:

- JSON file per operation instance

---

## Shared Module

Implemented in:

- client-app/src/transactions.rs

Functions:

- begin(operation, stack_name)
- record_phase(path, phase_name, status, error)
- finish(path, ok)

---

## Integration

Wired in:

- client-app/src/events.rs

This provides resumable/auditable phase history for core destructive and scaffolding flows.
