# Planned Use Case: Hot-Apply LXC Resources

**Tier:** CLIENT + HOST
**Status:** Planned

## Goal

Apply CPU and memory changes to running Proxmox LXCs without waiting for a full recreate workflow.

## Scope

- live updates where Proxmox allows them
- explicit handling for settings that require restart or cannot be reduced online
- clear rollback/error path in CLIENT

## UX Expectations

- preview current vs. target resources
- show which fields are hot-applicable vs restart-required
- require confirmation with blast-radius summary

## Notes

- disk expansion may stay separate because storage backends and filesystem grow steps vary