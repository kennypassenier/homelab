# Contributing Guidelines — GitOps Proxmox Homelab

Welcome! This document describes the core philosophy, design principles, and best practices for this project. Whether you are a human contributor or an AI assistant helping to build or maintain this codebase — **follow these guidelines for every script or configuration you write or modify.**

---

## 1. DRY (Don't Repeat Yourself) & Shared Libraries

We strive to avoid duplicate code. Scripts should be as modular as possible.

- **Library files:** Shared logic, variables, and helper functions belong in dedicated library files:
  - `scripts/shared/lib-ui.sh` — shared UI/UX components (colors, spinners, prompts).
  - `scripts/client/lib/lib-stack.sh` — stack and app management helpers for the client.
- **Sourcing:** Scripts must load (`source`) these libraries at the top instead of redefining functions locally.
- Always check whether a function already exists in one of the libraries before writing a new one.

---

## 2. User Experience (UX), Colors & Spinners

Command-line tools must be user-friendly, clear, and visually consistent.

- **Use `lib-ui.sh` for all output.** Never use raw `echo`, `printf` with hardcoded ANSI codes, or direct `gum` calls in scripts.
- **Gum integration:** `lib-ui.sh` automatically detects whether [Gum](https://github.com/charmbracelet/gum) is installed and whether stdout is a TTY. When both are true, rich TUI components are used (styled menus, animated spinners, bordered headers). When either is false (cron, CI, SSH pipe), everything falls back to plain POSIX equivalents automatically — no script changes required.
- **Color conventions:**
  - Green → success
  - Red → errors
  - Yellow → warnings
  - Cyan/Blue → informational messages
- **Interactive prompts — always use the wrappers:**

  | Function | Purpose |
  |---|---|
  | `ui_choose` | Single-item selection menu (`gum choose` / numbered list fallback) |
  | `ui_multiselect` | Multi-item checkbox picker (`gum choose --no-limit` / numbered list fallback) |
  | `ui_input` | Single-line text input (`gum input` / `read` fallback) |
  | `ui_input_required` | Like `ui_input`, but loops until a non-empty value is provided |
  | `ui_confirm` | Yes/No confirmation (`gum confirm` / y/N fallback) |

- **Spinners & progress:** Wrap long-running commands with `ui_spin` (`gum spin` / pacman fallback). Use `ui_run_pacman` only in legacy code — prefer `ui_spin` for all new code.
- **Headers & sections:** Use `ui_header` for full-width page headers (`gum style` box / divider fallback) and `ui_section` for in-page section headings with underlines.
- **Non-TTY safety:** Scripts must behave correctly in environments without a terminal (cron, CI). The UI library auto-disables Gum, colors, and spinners when stdout is not a TTY, keeping log output clean.

---

## 3. Safety, Error Handling & Idempotency

Scripts must be robust and produce no unexpected side effects when run repeatedly.

- **Idempotency:** Running a script multiple times must always produce the same end state. Check before modifying — e.g., verify an `~/.ssh/config` entry or directory doesn't already exist before creating it.
- **Traps & rollbacks:** For scripts that perform multi-step critical operations or create temporary files, always add a `trap` to clean up or roll back on failure or `Ctrl+C`.
- **Graceful exits:** Exit with a non-zero code on any critical failure. Use `set -e` where appropriate, or explicitly catch and handle errors with clear messages.
- **Secret management:** Never hardcode passwords, API keys, tokens, or sensitive paths. Use the SOPS/Age infrastructure or local uncommitted `.env` files (`chmod 600`, listed in `.gitignore`).
- **Destructive actions require double confirmation:** Any script that deletes files, removes stacks/apps, or destroys data must:
  1. Clearly describe what will be deleted — in **red** via `ui_warning` or `C_RED`.
  2. Ask "Are you sure?" and then "Are you **ABSOLUTELY** sure?" before proceeding.

---

## 4. Naming Conventions & Script Placement

- **Naming:** Follow the `[action]-[object].sh` pattern — e.g., `create-new-app.sh`, `sync-host.sh`, `remove-stack.sh`.
- **Placement:** Scripts must live in the folder matching where they are executed:

  | Folder | Execution context |
  |---|---|
  | `scripts/client/` | Linux workstation (local management) |
  | `scripts/host/` | Proxmox VE host |
  | `scripts/container/` | Inside an LXC container (Docker context) |
  | `scripts/shared/` | Shared across multiple environments |

---

## 5. Code Documentation & CLI Arguments

- **Comment your code:** Explain *why* a piece of code exists or why a specific approach was chosen — not just *what* it does.
- **English only:** All comments, docstrings, commit messages, and documentation must be in English.
- **Help menus:** Every significant script must support `-h` / `--help` via `getopts`, clearly explaining usage and available flags.
- **Automation flags:** Interactive scripts must also support optional CLI flags to skip prompts, making them suitable for use in automation pipelines and cron jobs.
- **Keep docs in sync:** Whenever you add, remove, or significantly change a script or its flags, update `docs/README.md` (and `docs/LLM_CONTEXT.md` if the architecture or LLM context is affected) in the same commit.

---

## 6. Submitting Changes

If this project is ever opened to external contributors:

- Open an issue first to discuss larger changes before putting in the work.
- One logical change per pull request — keep PRs focused and easy to review.
- All scripts must pass a basic `shellcheck` lint before being merged.
- Match the style and conventions of the existing codebase (see sections above).
- Do not commit secrets, unencrypted `.env` files, or personal configuration.
