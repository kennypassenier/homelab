# Planned Use Case: Boot Policy Orchestration

**Tier:** CLIENT + HOST
**Status:** Planned

## Goal

Enforce stack boot policy (`autostart`, `order`) from `lxc-compose.yml` onto Proxmox LXC runtime config.

## Current State

- CLIENT already captures and stores boot policy in stack config
- host-side reconciliation of these fields is not implemented yet

## Desired Behavior

- reconcile drift between Git intent and Proxmox runtime config
- apply non-critical stack defaults with low boot priority
- include safety guardrails to avoid changing explicitly unmanaged containers