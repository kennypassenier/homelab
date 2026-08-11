# Planned Use Case: One-Shot Self-Destruct Hooks

**Tier:** CLIENT + HOST + LXC
**Status:** Planned

## Goal

Support explicitly one-time operational scripts that are automatically removed (or marked consumed) after successful and confirmed execution.

## Problem Statement

Some operational changes are one-time by nature (for example migration, cache purge, or data repair) and should not be left as reusable hooks that can run accidentally on future syncs.

## Candidate Use Cases

- one-time migration after schema/image breaking change
- emergency data repair command that must never repeat automatically
- one-off bootstrap fix for legacy stacks after architecture transitions
- temporary remediation for known bad release state

## Safety Model

- store one-shot hooks in a dedicated folder separate from persistent pre-sync hooks
- require explicit success marker before destruction
- keep an execution audit log (who/when/result/hash)
- fail closed: if execution fails, do not self-destruct

## Is This Necessary?

Not strictly required for basic operations: current GitOps + pre-sync + restore pathways already provide a strong baseline.

It becomes high-value when:

- operational maturity requires controlled one-time remediations
- you want cleaner separation between recurring automation and surgical recovery logic
- you want lower risk of rerunning one-off scripts by accident

## Dependencies

- one-shot hook state tracking and integrity checks
- audit/event logging path surfaced in CLIENT UI
