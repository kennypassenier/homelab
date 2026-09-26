# Test plan part A — headless run of 2026-09-26

Kenny's retrospective choice (item test-pass): Claude runs part A of
`docs/TEST_PLAN.md` headless and reports; part B is closed as an open item.

**How.** `homelab tui --offline` from the installed v3.56.0 binary, driven
through a pseudo-terminal at 120×36 with `xterm-256color`; every screen was
captured as text with the `pyte` terminal emulator (run with
`uv run --with pyte`, nothing installed). The TUI asks the terminal for its
cursor position at start-up, so the driver answers that query — without it
the TUI exits with "The cursor position could not be read". The run used a
throwaway copy of the repository (`git archive HEAD`), so the wizard's
scaffolds and the throwaway preset never touched this tree. 59 + 23 + 54
screens were captured; excerpts below are copied from them.

What a text capture cannot judge: colour, the glitch and flicker effects
themselves, and whether an animation feels right. Those stay Kenny's.

## Result per step

| step | result | evidence |
|---|---|---|
| A1 boot & chrome | pass | splash builds the logo, `▸ READY :: press any key`, a key lands on DASHBOARD |
| A2 tabs, AZERTY | pass | `1`–`4` and `& é " '` land on the same four tabs (key hints change with the tab); TAB and SHIFT+TAB move |
| A3 effects toggle | pass, plan wrong | F2 cycles `FX:FULL → FX:OFF → FX:SUBTLE → FX:FULL` (code: `fx.rs` `cycle`); the plan says FULL → SUBTLE → OFF |
| A4 dashboard + capacity | pass, finding 7 | HOST_MESH, LXC_MESH (3 nodes), CAPACITY leads with `RAM used … 39%`, `alloc … 1.2×`, `load 2.85 / 12 cores` |
| A5 fleet navigation | pass | `▶` follows DOWN, DOWN, UP |
| A6 log stream | pass, finding 3 | source filter narrows (platform lines, then media lines); `⏸ SCROLL -6` after three UP; SPACE back to `▶ FOLLOW`; `G` to the tail |
| A7 doctor | pass | `[Ok]`/`[Warn]` lines with `↳ run a backup; the scheduler may be stalled` |
| A8 command palette | pass | CTRL+K opens COMMAND_DECK, `doct` narrows to two entries, ENTER lands on DOCTOR |
| A9 new-stack wizard | pass, findings 4–6 | five steps (the plan knows four: STORAGE is missing), `scaffolded stacks/retrotest (4 files) — press SHIFT+D to deploy` |
| A10 change plan | pass | `dry-run — nothing runs until you confirm`, CREATE/ADD lines; ESC closes |
| A11 deploy focus | pass | FOCUS window streams the steps, `⇅ docker-compose.yml 24576B` during push files, title flips to `COMPLETE` with `● ALL GATES PASSED`; the feed continues in LOG_STREAM |
| A12 ticker | pass, finding 2 | only actionable items: `⚠ UPD pending: media`, `⚠ radarr down in media`, the host update |
| A13 small terminal | pass | at 70×20: `TERMINAL TOO SMALL — need 80x24, got 70x20`; restores at 120×36 |
| A14 CLI validation | pass, finding 8 | `✓ valid — syncthing would deploy vmid 108`; a corrupted `memory_mb` gives an error |
| A15 settings | pass | RIGHT turns tier 1 from 1d to 2d, `a` adds a tier (30d / 90 days), `● unsaved changes — SHIFT+S to apply`, SHIFT+S back to `● in sync with host`; `(` reaches the tab from elsewhere |
| A16 data-driven presets | pass, plan wrong | 10 presets, `custom` last, no fallback markers (the plan says 6); a throwaway `test-x` preset appears and scaffolds with every placeholder substituted |
| A17 stack bundles | not re-run | already marked live-proven 2026-08-11 |
| A18 remote shell | offline half passes | typing goes to the prompt, ENTER answers `ok (demo)`; the live half needs a host with `exec_enabled` |
| A19 metrics preset | not offline | listed in `homelab presets`; the rest is a live deploy to a test vmid, which part A cannot do |

## Findings

1. **The status message overwrites the key hints.** At 120 columns the
   footer reads `[H] help  [Q]link established`, `[Q]effects → FX:OFF`,
   `[R] relok (demo)`: the right-aligned status is drawn over the hint text
   instead of beside it.
2. **The ticker says "press U", but `U` is a different command.** The
   ticker reads `⬆ HOST UPDATE v3.56.0 beschikbaar — press U (…)`. The host
   update is lowercase `u` (`model.rs`); SHIFT+U is `StackOp::Update` for
   the selected stack. The same shape in the footer: `[R] refresh` is
   lowercase `r`, and SHIFT+R is restore (which does ask first). The line
   also carries a Dutch word in an English interface (`view/mod.rs:599`).
3. **The log stream's stack column is too narrow for `syncthing`**:
   `DEBUG syncthingsyncthing :: folder "obsidian-vault" in sync` — the column
   holds eight characters and the name has nine.
4. **The wizard still gives every new stack a promtail sidecar.** The
   scaffold writes `promtail/docker-compose.yml` (`grafana/promtail:3.0.0`)
   because `StackDefaults::core_apps` is `["promtail"]`
   (`client/src/scaffold.rs:73`). Since 2026-09-02 the deploy installs
   Grafana Alloy on every container itself (`core/src/ops/deploy.rs`, the
   "log shipper" step), and no stack in `stacks/` carries a promtail
   directory any more — so a new stack would run two log shippers, one of
   them end of life. (The scaffold's `template: "clone:998"` looked stale at
   first sight, but 13 of the 15 stacks in `stacks/` use 998 and 2 use 997;
   the Debian 13 pair waits on Kenny's go per container, so 998 is the
   consistent default.)
5. **Two rendering slips in the wizard.** The review step prints
   `resources1024 MiB · 2 cores` (no space after the label), and the storage
   step's hint line reads `[UP/DOWN] selectback` — text from a longer line
   left behind.
6. **The name field starts with the preset's name and typing appends.**
   Choosing `actual` and typing `retrotest` gives `actualretrotest`; the
   plan says "type a name".
7. **The CAPACITY panel is cut off at 120 columns**: `free 19064 M`,
   `ceilings 1.2× (LX`.
8. **The validation error points at the wrong line and gives no remedy.**
   With `memory_mb: lots` on line 14 the CLI says
   `manifest parse: invalid type: string "lots", expected u32 at line 2 column 1`
   — standing rule 11 asks every error to carry its remedy.

The plan text itself was also out of date (A3 order, lowercase keys, the
wizard's STORAGE step, 10 presets); `docs/TEST_PLAN.md` is corrected in the
same commit as this report.
