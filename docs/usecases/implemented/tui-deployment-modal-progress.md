# Use Case: TUI Deployment Modal Progress

**Tier:** CLIENT
**Status:** Implemented

---

## Implemented Scope

A reusable operation progress modal is implemented for long-running orchestration actions.

Current consumers:

- Manual backup all ([b])
- Individual restore ([i])
- Full restore plan ([r])
- OS patching plan ([p])
- Unattended-upgrades status ([u])

Modal capabilities:

- Title + phase display
- Per-entry status rows
- Summary footer
- Close via Enter/Esc

---

## Files

- client-app/src/blast_radius.rs
- client-app/src/ui.rs
- client-app/src/events.rs

This establishes a shared modal surface for future live streamed progress integrations.
