# Planned Use Case: Change Plan Diff and Approval Gate

**Tier:** CLIENT + HOST + LXC
**Status:** Planned

## Goal

Show an explicit "what will change" plan before apply actions, with optional approval gates.

## Why It Matters

Ansible ecosystems often rely on check-mode/diff discipline. Portainer-like systems provide clear previews before redeploy/update operations. This improves confidence and reduces accidental drift-inducing changes.

## Candidate Capabilities

- pre-apply plan for stack syncs: image updates, container recreations, config key changes
- host plan preview: boot policy/resource delta before `apply`
- optional approval requirement for high-impact plan items
- plan hash persisted so applied state can be tied to reviewed intent

## Suggested Output

- categorized delta list: `safe`, `disruptive`, `destructive`
- estimated blast radius (affected stacks/apps/services)
- reason codes when apply is blocked by policy

## Dependencies

- diff builder across Git intent and runtime state
- policy hook for approval-required categories
- operation ledger extension for plan-id to apply-id linkage
