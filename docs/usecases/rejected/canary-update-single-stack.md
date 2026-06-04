# Planned Use Case: Canary Update (Single Stack First)

**Tier:** CLIENT + LXC
**Status:** Planned

## Goal

Test update behavior on one designated canary stack before rolling to all active stacks.

## Why Useful

Ansible/Portainer operators often stage rollouts manually. A built-in canary flow gives safer updates without enterprise complexity.

## Candidate Scope

- mark one stack as canary target
- run update/sync only on canary first
- require explicit confirmation before broad rollout
- auto-stop rollout if canary health checks fail

## Open Questions

- **Canary designation:** Is the canary stack a fixed config value (e.g. in `.env` or a manifest), or chosen interactively per rollout? A fixed canary is simpler but less flexible.
- **Health check definition:** What constitutes a passing health check after a canary update? Container exit code 0? A HTTP probe? Docker healthcheck status? There is currently no per-app health probe mechanism.
- **Rollback on failure:** If the canary fails, should the system automatically roll back (re-pull previous image tag) or just halt and alert? Automatic rollback requires image tag pinning, which conflicts with Watchtower's latest-tag model.
- **Scope of "broad rollout":** Does confirmation trigger an immediate sync of all other LXCs, or does it just let the next `node-sync.sh` cycle pick it up naturally? The latter requires no new machinery but gives no timing guarantee.
