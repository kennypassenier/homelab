# Use Case: Add Stack

**Tier:** CLIENT
**Status:** Implemented

---

## Overview

A stack can now be scaffolded directly from the CLIENT TUI with a dedicated flow.

Trigger:

- Scaffolding tab key: n

Result:

- Creates stacks/<stack_name>/
- Creates stacks/<stack_name>/setup.sh (idempotent scaffold hook)
- Creates stacks/<stack_name>/lxc-compose.yml via shared scaffold helper
- Adds missing core apps (promtail, watchtower, traefik)
- Commits via existing GitOps helper

---

## Shared Module

Implemented reusable primitive:

- create_stack(stack_name)

File:

- client-app/src/stack_features.rs

This module-first primitive is reusable for future stack onboarding and batch operations.

---

## TUI Flow

Modal added:

- Stack creation wizard with Name -> Review -> Done steps

Files:

- client-app/src/blast_radius.rs
- client-app/src/events.rs
- client-app/src/ui.rs
