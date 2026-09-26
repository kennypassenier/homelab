# Orchestrator retrospective (Phase 10)

Opened 2026-09-26. This is the orchestrator track only (homelab v3, the
Rust client + host daemon). The deployment project in `docs/deployment/`
keeps its own track and its own retrospective inputs: the expert panel,
the build-it-ourselves round, the syncthing round and the running
`RETRO_DATA.md` all belong there, not here.

## 1 · The numbers first

Measured 2026-09-26 with `~/Projects/dev-procedure/hooks/delivery-metrics.sh 90`
over the last 90 days, next to every other repository in `~/Projects`:

| project | released | lead (d) | fix share | repair (h) |
|---|---|---|---|---|
| **homelab** | **78** | **24** | **30%** | **0** |
| latch-rs | 7 | 45 | 28% | 198 |
| kp-themes | 18 | 13 | 22% | 0 |
| almanac | 25 | 23 | 20% | 1 |
| http-switchboard | 5 | 20 | 20% | 0 |
| chassis-rs | 15 | 16 | 6% | 0 |
| kyu | 19 | 21 | 5% | 0 |

Homelab releases far more often than anything else (78 tags; the next is
almanac with 25) and has the highest fix share in the house: 22 of 78
releases were followed directly by a `fix` commit. Repair time is a median
of 0 hours, so the fixes arrive within the hour — they are caught, just
after the tag instead of before it.

### What the 22 fixes were

Listed with `git log --reverse --format=%s <tag>..main | head -1` per tag:

- **Data leaving a container (logs, metrics, backup checks) — 9 of 22**:
  after v3.4.0, v3.4.1, v3.4.2, v3.5.1, v3.9.0, v3.40.0, v3.40.1, v3.42.0
  and v3.42.1. Four of those nine were a mechanism reporting success while
  doing nothing: the growth check watched 5 of 9 containers (after v3.4.0),
  Prometheus was never told to read its discovery directory (after v3.4.2),
  the log shipper said "shipping" while every batch was dropped (after
  v3.40.0), and a backup watcher watched nothing (after v3.42.0).
- **Fix-of-a-fix chains — 4.** v3.3.0 → v3.3.1, v3.4.0 → v3.4.1 → v3.4.2,
  v3.40.0 → v3.40.1, v3.42.0 → v3.42.1: each time the first fix was tagged
  before its effect was read at the destination (the log store, the
  Prometheus target list, the restic repository).
- **Environment mismatch — 1.** v3.49.0 reverted the kyu migration because
  CT 109 could not run a binary built against GLIBC 2.39. Already handled
  at the source: chassis-rs releases are static musl and its release
  workflow refuses a dynamic binary (`ECOSYSTEM.md`, chassis-rs linkage).
- **The rest — 12** — single corrections without a shared shape (CI MSRV,
  adopt descriptions, a contested backup repository, …).

### The four features that were added after the hardening batch

Release-driven host updates, the per-stack enabled flag, ZFS snapshots +
replication and own Rust services via GHCR were added after Phase 7 and
never had a gate of their own. Counted with `git log -E --grep`:

| feature | commits | fix commits | span |
|---|---|---|---|
| ZFS snapshots + replication | 10 | 0 | 2026-08-27 … 09-02 |
| own Rust services via GHCR | 7 | 0 | 2026-08-12 … 09-02 |
| release-driven host updates | 8 | 1 | 2026-08-12 … 09-19 |
| per-stack enabled flag | 10 | 0 | 2026-08-12 … 09-04 |

35 commits, one fix — and that fix was wording (six places where the same
thing was said two ways). Built on a hardened codebase with its test
harness in place, small features held up without a phase pass of their own.

## 2 · The drop list

`hooks/rule-usage.sh` over 90 days: one silent rule, **44 · a status field
must be shown to vary**. It is not a dead rule here: the log shipper
reporting "shipping" while dropping every batch (v3.40.0) is exactly that
fault, a status that never took a second value. It was applied, not cited.

## 3 · Housekeeping found while resuming

- **Both host-OS rollback snapshots are dead.** `lvs` on pve, 2026-09-26:
  `root-pretest` and `root-v2-preinstall` are both `swi-I-s---` at 100% —
  the `I` means invalid; a full classic snapshot cannot roll anything back.
  They hold 16 GB of a volume group with 0 free. Removing them is a host
  change and waits on Kenny's go.
- **The old migration milestone is carried by the deployment project.**
  The no-touch list in `core/src/safety.rs` is 100–103 since 2026-08-30;
  the legacy stacks are integrated one by one there. CLAUDE.md said
  otherwise and is corrected.
- **The pre-test vzdump** of CT 108 from 2026-08-27 no longer exists on the
  host (searched with `find / -xdev -name 'vzdump-lxc-108-2026_08_27*'`).

## 4 · Outcome

Filled in after Kenny's form: lessons adopted, the ecosystem entry, and the
diff on `~/Projects/dev-procedure`.
