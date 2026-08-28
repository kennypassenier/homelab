# tui-preview — Homelab v2 TUI mockup

An interactive mockup of the future CLIENT interface, driven entirely by
**simulated data**. Nothing here talks to Proxmox, the network, or any daemon —
it exists to experience the intended look & feel before the real rewrite.

```bash
cargo run --release
```

Best in a truecolor terminal at 100x30 or larger (minimum 80x24).

## What to try

| Key | Action |
|-----|--------|
| any key | skip the boot splash |
| `1`-`4` / `Tab` | switch tabs (Dashboard, Stacks, Backups, Logs) |
| `j`/`k` | move selection |
| `n` | new-stack wizard with the preset catalog |
| `D` | deploy: change-plan diff preview → live progress modal |
| `a` / `x` | activate / deactivate the selected stack |
| `b` | run a restic backup for the selected stack |
| `d` | delete a stack (typed-name confirmation) |
| `Ctrl+K` | command palette |
| `F2` | cycle effect intensity (off / subtle / full) |
| `space` / `f` | logs: toggle follow / cycle level filter |
| `?` | keymap help |
| `q` | quit |

## Effects showcase

Glitch bursts on titles, decrypt-reveal on tab switches, power-cycle flicker,
pulsing selection, scanline sweeps, live sparklines, telemetry ticker, boot
splash with ASCII logo — all budgeted to a 30 FPS animation tick and tunable
with `F2`.
