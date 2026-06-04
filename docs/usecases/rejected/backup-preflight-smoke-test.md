# Planned Use Case: Backup Preflight Smoke Test

**Tier:** HOST + LXC
**Status:** Planned

## Goal

Provide a fast preflight that verifies backup/restore prerequisites before scheduled backups or risky operations.

## Why Useful

Many ops stacks include backup health checks. For a homelab admin, a short smoke test catches broken paths, missing credentials, and unusable repos early.

## Candidate Scope

- check restic repository reachability and lock health
- verify rclone backend connectivity when configured
- validate critical restore tooling availability (for example rsync)
- emit pass/fail summary with fix hints

## Open Questions

- **Trigger point:** Should this run automatically before each scheduled backup, on demand via CLIENT TUI, or both? Automatic pre-run adds latency; on-demand is safer but easy to skip.
- **Failure policy:** If preflight fails, should the backup be aborted or just warned about? Abort is safer but could silently skip backups for a long time if a transient check fails.
- **Scope of reachability check:** Restic needs the actual repo passphrase to perform a real lock check (`restic check`). Does this mean preflight needs access to the SOPS-decrypted passphrase at check time, or is a lightweight TCP ping to the backend sufficient?
- **Rclone dependency:** Not all stacks use rclone. Is this check stack-specific or global? If stack-specific, how does HOST know which stacks use rclone?
