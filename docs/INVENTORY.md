# Inventory (Phase 1, brownfield) — written retroactively 2026-08-11

Distilled from the pre-project analysis of 2026-08-10: the complete sweep
of the predecessor (3-binary Rust system, ~22.4k LOC, 530 commits, 83
releases, abandoned mid-migration June 2026 with one open credential bug)
and the frozen Ansible repo. This inventory fed the Phase 2 feature
rating; the verdicts below are as decided then.

## Verdict summary over the 59 documented use-cases

**KEEP (cornerstones, survived the merge nearly unchanged):** automated
LXC provisioning with dry-run + whitelist-only safety; the 5-layer SAFETY
model; stack/app lifecycle + config editors; deploy/update with drift
detection; restic backup scheduling/restore; GPU/TUN/bind-mount/boot
policy; HOST self-update with rollback; Kea static-IP automation; naming
scheme `vmid-app-<stack>`; OS patching; live deploy telemetry; fail-closed
error handling; fleet-wide log rotation (added by Kenny 2026-08-10).

**CHANGE:** gitops sync (in-LXC sparse checkout → CLIENT→HOST push over
one line); 13-step bootstrap → ~6 steps; manifest schema v2 (intent only,
machine state → host state.json); transaction journal extended to all
destructive paths; `/api/exec` kept but HOST-routed, off by default;
notifications → minimal HA-webhook; template catalog + change-plan
preview revived lightweight (both had been rejected in the old project).

**DROP:** LXC daemon + GHCR pipeline; per-daemon TUIs (~700 lines dead
ratatui); pre-sync hooks; one-shot self-destruct hooks; maintenance
windows; resource-pressure alerts; latch secret flow (structurally
eliminates the open sync-stall bug); RBAC/policy/canary pack stays
rejected.

## Design flaws found (all addressed during the rewrite)

1. Open latch credential sync-stall bug → eliminated by design (no latch).
2. Unapplied age-based-retention patch → superseded (G8 tiered retention).
3. 155 `.unwrap()` mutex-poisoning cascade → new code, no shared-state
   panics of that class.
4. Machine-written state in git → state.json (AR4).
5. 44 MB binaries committed per release → removed; releases are the channel.
6. CI was `echo`-theatre → real fmt/clippy/test CI, red blocks.
7. Zero tests on destructive paths → Executor trait + 78 tests incl. every
   destructive op.
8. Docs described a dead architecture → M6 honesty pass; legacy archived
   to docs/legacy/.
9. WS channel double-duty without envelope → AR5 typed envelope.
10. Watchtower `:latest` without rollback → D9/B6 managed updates with
    digest capture + rollback.
11. Credential fragments in docs → scanned during the legacy move (no real
    key material found).
12. Zombie host-daemon crash-looping on the host → decommissioned
    2026-08-11 with Kenny's go.

## Live state at project start (2026-08-10)

Managed-scope candidates CT 104 (platform), 105 (downloader), 106 (media)
— healthy, untouchable until migration. CT 107 + 111 slated for deletion
(still pending Kenny's go). Old host-daemon crash-looping since June
(gone). rclone token dead (since recovered via Kenny's own OAuth client).
