# Scope (Phase 0) — written retroactively 2026-08-11

The scope was agreed conversationally on 2026-08-10 (plan approved before
any code); this document distils that agreement for the paper trail. It
matches what was actually built — deviations went through the feature/AR
forms, not through silent scope drift.

## Problem

The homelab was managed by a fragile Ansible repo that could not even run
(dead rclone token hard-blocked every play), plus an abandoned but largely
functional 3-binary Rust predecessor. Kenny wanted one system that manages
the fleet flawlessly, makes adding services trivial, and is fun to operate.

## Goals

1. Revive the Rust system as THE management layer, restructured to two
   binaries: CLIENT (CLI + cyberpunk TUI) and HOST (Proxmox daemon).
   Containers run zero agent code.
2. Everything travels over one secured CLIENT↔HOST line (TLS pinned +
   token): compose files, manifests, `.env` secrets.
3. Adding/removing a service is trivially easy (wizard + preset catalog).
4. Idempotent, journaled, fail-closed operations with systematic
   debuggability (incident bundles, replay scripts).
5. Full lifecycle: provision, deploy, update w/ rollback, backup/restore
   (restic → Google Drive), destroy, self-update, fleet patching.
6. Traefik stays as the reverse proxy; existing HA notification system is
   the notification channel.

## Non-goals

- No per-container daemon, no GHCR image pipeline, no latch secret sync
  (latch-rs stays a separate standalone project).
- No git checkouts inside containers; GitHub is an optional mirror, never
  in the critical path.
- No multi-user/RBAC, no policy-as-code, no canary updates (single-admin
  homelab).
- No rebuild of monitoring (Grafana/Loki/Uptime-Kuma exist); the
  orchestrator reports what it *does*, monitoring reports what *runs*.
- No Kubernetes; the k3s cluster is out of scope and untouchable.
- PBS/vzdump whole-container backups: separate infrastructure project
  (tracked in the vault, not in this codebase).

## Hard constraints

- **No-touch fleet** (enforced in code, `core/src/safety.rs`): VM 100
  (OPNsense), VM 101 (Home Assistant + Mosquitto + SBFspot), CT 102
  (omada), CT 103 (fileserver), VMs 201–203 (k3s), and the un-migrated
  stacks CT 104–107/111 until their explicit migration step.
- The Proxmox host is never touched outside an agreed step; the gridsim
  demo (Aug 15–31, 2026) must never be endangered.
- vmid 108 is the dedicated automated-test container until Kenny
  reassigns it.
- Language: Rust (revival of an existing Rust codebase was itself a scope
  decision). Platform: the existing standalone Proxmox host.
- Subscription-tier tooling only (no credit-billed extras).

## Success criteria

- The full loop proven on a fresh, harmless pilot stack (syncthing) before
  any existing service is touched. ✔ (achieved on vmid 108)
- Existing stacks (104/105/106) keep running untouched until migration. ✔
- Every Must feature live with its FEATURES.md test scenario passing. ✔
- Migration (M5) moves platform/media/downloader with zero config loss —
  the completeness rule of MIGRATION_INVENTORY.md. (open)
