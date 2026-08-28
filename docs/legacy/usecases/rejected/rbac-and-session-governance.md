# Planned Use Case: RBAC and Session Governance

**Tier:** CLIENT + HOST + LXC
**Status:** Planned

## Goal

Add multi-user safety controls so operational power is scoped by role instead of full admin-by-default access.

## Why It Matters

Comparable platforms (Portainer, AWX/Ansible Tower) provide per-user permissions, scoped tokens, and action-level control. This reduces accidental high-impact actions and supports team workflows.

## Candidate Capabilities

- roles such as `viewer`, `operator`, `admin`
- action gates for destructive operations (restore, delete stack, host-level apply)
- short-lived API tokens with explicit scopes (sync only, logs only, restore)
- session activity metadata (who initiated what, from where, when)

## Suggested Guardrails

- deny-by-default for privileged operations
- explicit approval step for high-blast-radius actions
- immutable audit entries for authz decisions

## Dependencies

- identity source (local users first, optional SSO later)
- auth middleware for HOST/LXC APIs
- CLIENT UX for role-aware action visibility
