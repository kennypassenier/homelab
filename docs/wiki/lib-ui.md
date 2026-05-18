# lib-ui — Shared UI Library

> `scripts/shared/lib-ui.sh` provides all output, prompts, spinners, and TUI components. Every script routes through this library — no raw `echo` with ANSI codes, no direct `gum` calls.

## Overview

`lib-ui.sh` is the single source of truth for all user-facing output in the homelab scripts. It auto-detects two things at load time:
1. **Is stdout a TTY?** — If not (cron, CI, piped output), all colors, spinners, and interactive components are disabled automatically.
2. **Is [Gum](https://github.com/charmbracelet/gum) installed?** — If yes and stdout is a TTY, rich TUI components are used. If not, identical POSIX fallbacks take over.

This means scripts work correctly in all environments with zero conditional logic in the callers.

## Sourcing

```bash
source "scripts/shared/lib-ui.sh"
# or from a subdirectory:
source "$(dirname "$0")/../../shared/lib-ui.sh"
```

The library guards against being sourced multiple times in the same shell session via `_LIB_UI_LOADED`.

## Color Variables

Set automatically based on TTY detection. Always use these — never hardcode ANSI codes.

| Variable | Color | Meaning |
|---|---|---|
| `$C_GREEN` | Green | Success |
| `$C_RED` | Red | Errors, destructive actions |
| `$C_YELLOW` | Yellow | Warnings |
| `$C_CYAN` | Cyan | Info, prompts, steps |
| `$C_BLUE` | Blue | Info |
| `$C_NC` | Reset | End of color |

## Logging Functions

All logging functions write to **stderr** so they never pollute `stdout` (important for functions that return values via `$()` substitution).

| Function | Purpose | Example |
|---|---|---|
| `ui_info "msg"` | Blue info message with ℹ️ | Informational notices |
| `ui_success "msg"` | Green success with ✅ | Operation completed |
| `ui_warning "msg"` | Yellow warning with ⚠️ | Non-fatal issues |
| `ui_error "msg"` | Red error with ❌ | Fatal issues |
| `ui_step "msg"` | Cyan step indicator with ➡️ | Progress through a workflow |

## Interactive Prompt Functions

### `ui_choose` — Single Selection Menu

```bash
CHOICE=$(ui_choose [--header "Prompt"] "Item 1" "Item 2" "Exit")
```

- With Gum: styled interactive picker (arrow keys + Enter)
- Without Gum: numbered list, user types a number
- Returns: the chosen item's text via stdout
- Exit code 1 if user presses Esc (Gum only) — callers use `|| return 1` or `|| exit 0`

### `ui_multiselect` — Multi-Select Checkbox

```bash
mapfile -t SELECTED < <(ui_multiselect [--header "Prompt"] [--selected "Item 1,Item 2"] "Item 1" "Item 2" ...)
```

- With Gum: checkbox picker (Space to toggle, Enter to confirm)
- Without Gum: space-separated number input; empty = nothing selected
- Returns: each selected item on its own line via stdout

### `ui_input` — Text Input

```bash
VALUE=$(ui_input "Prompt" [placeholder] [default])
```

- With Gum: styled single-line input with placeholder and pre-filled default
- Without Gum: `read -r -p` with optional default shown in brackets
- Returns exit code 1 on Esc (Gum only)

### `ui_input_required` — Required Text Input

```bash
VALUE=$(ui_input_required "Prompt" [placeholder] [default])
```

Like `ui_input`, but loops until the user provides a non-empty value.

### `ui_confirm` — Yes/No Confirmation

```bash
if ui_confirm "Are you sure?" [true]; then
    # yes
fi
```

- Second argument `"true"` makes Yes the default (for non-destructive prompts)
- Omit or `"false"` defaults to No (always use for destructive actions)
- Returns 0 for yes, 1 for no/cancel

### `ui_trim` — Strip Whitespace

```bash
trimmed=$(ui_trim "  my input  ")
```

Strips leading and trailing whitespace. Used by removal scripts to compare typed confirmation values.

## Layout Functions

### `ui_header` — Full-Width Page Header

```bash
ui_header "My Page Title" [color_var]
```

- With Gum: rounded border box, centered, scaled to terminal width
- Without Gum: two dividers with the title between them

### `ui_section` — In-Page Section Heading

```bash
ui_section "Section Name"
```

Underlines the heading with `─` characters matching the title length exactly.

### `ui_divider` — Horizontal Rule

```bash
ui_divider [color_var]   # defaults to C_CYAN
```

Prints a full-width `─` line scaled to terminal width.

## Spinner Functions

### `ui_spin` — Preferred Spinner (new code)

```bash
ui_spin "Loading message..." command args...
```

- With Gum: animated dot spinner with title
- Without Gum: delegates to `ui_run_pacman`
- Prints success or error on completion

### `ui_run_pacman` — Pacman Spinner (legacy)

```bash
ui_run_pacman "Doing something..." command args...
```

Animates a Pac-Man eating dots while `command` runs in the background. Used in `bootstrap-lxc.sh` and older code. Prefer `ui_spin` for new scripts.

## Gum Installation Hint

When running in an interactive terminal without Gum installed, the library prints a one-time install hint appropriate for the detected package manager (pacman, brew, dnf, apt). In non-interactive environments, this hint is suppressed entirely.

## See also

- [lib-stack.md](lib-stack.md) — stack management library (sources this library)
- [script-create-new-stack.md](script-create-new-stack.md)
- [script-create-new-app.md](script-create-new-app.md)
- [Architecture Overview](architecture-overview.md)
