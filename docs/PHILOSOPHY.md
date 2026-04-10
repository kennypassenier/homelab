# LLM & Developer Philosophy - GitOps Proxmox Homelab

This document describes the core philosophy, design principles, and best practices we follow in this project. It is specifically written as a guideline for both LLMs (like Claude, ChatGPT, Gemini) and human developers. **Always adhere to these guidelines when writing new scripts or modifying existing code.**

## 1. DRY (Don't Repeat Yourself) & Shared Libraries

We strive to avoid duplicate code. Scripts should be as modular as possible.

- **Library Files:** Shared logic, variables, and helper functions must be placed in separate library files.
  - Example: Shared UI/UX components are in `scripts/shared/lib-ui.sh`.
  - Example: Specific stack and app-related functions for the client are in `scripts/client/lib/lib-stack.sh`.
- **Sourcing:** Scripts should load ("source") these libraries at the top instead of redefining functions over and over.
- Always check if a function already exists in one of the libraries before writing a new one.

## 2. User Experience (UX), Colors, and Spinners

Our command-line tools must be user-friendly, clear, and visually appealing.

- **Shared UI Library:** Always use the functions from `scripts/shared/lib-ui.sh` for output. Avoid raw `echo` or `printf` commands with hardcoded ANSI color codes in your main scripts.
- **Colors:** Use color to clarify the meaning of output (e.g., green for success, red for errors, yellow for warnings, blue or cyan for information). The shared UI library provides standard wrappers for this.
- **Spinners & Progress:** Long-running actions must be provided with visual feedback to indicate the script is still working. Use the standard "pacman" spinner wrapper (e.g., `ui_run_pacman`) from the shared UI library to wrap commands that take a while.
- **Interactive & Foolproof Menus:** Where possible, scripts should prompt users for input using foolproof interactive menus (e.g., numbered lists for selecting a stack) rather than relying strictly on free-text typing or positional arguments. This prevents typos and invalid selections, making the scripts watertight for manual use.
- **Non-interactive environments:** Ensure scripts gracefully handle environments without a TTY (like cronjobs or CI pipelines). The UI library should automatically disable colors and spinners if they are not supported, so log files are not polluted with unreadable control codes.

## 3. Safety, Error Handling & Idempotency

Scripts must be robust and have no unexpected or destructive consequences upon (repeated) execution.

- **Idempotency:** A script must be able to run safely multiple times in a row, with the same end result each time. Only adjust configurations if necessary (e.g., check if an alias is already in `~/.ssh/config` or if a folder already exists).
- **Traps & Rollbacks:** For scripts that perform multiple critical steps or create temporary files (like the bootstrap scripts), a `trap` MUST be used. This trap ensures temporary files or partially applied changes are neatly cleaned up or rolled back if the script fails prematurely or is canceled (Ctrl+C).
- **Graceful Exits:** Scripts must provide clear error messages and exit with a non-zero exit code if a critical requirement is missing or a command fails (use `set -e` where appropriate, or catch specific errors cleanly).
- **Secret Management:** NEVER hardcode passwords, API keys, or sensitive paths in scripts. Use the existing SOPS/Age infrastructure or local, uncommitted `.env` files (which are protected with `.gitignore` and have the correct `chmod 600` permissions).
- **Destructive Actions & Double Confirmation:** Any script that performs a destructive action (like deleting files, removing stacks/apps, or destroying data) MUST clearly state what is about to be deleted (using red text via `ui_warning` or `C_RED`) and require a double confirmation (e.g., "Are you sure?" followed by "Are you ABSOLUTELY sure?") to minimize typos and accidental data loss.

## 4. Naming Conventions & Structure

- **Action-Object Structure:** Use the `[action]-[object].sh` convention for scripts (e.g., `create-new-app.sh`, `sync-host.sh`, `remove-app.sh`).
- **Location:** Place scripts in the correct folder based on where they should be executed:
  - `scripts/client/` for local management from the workstation (Pop!_OS).
  - `scripts/host/` for management and configuration directly on the Proxmox server.
  - `scripts/container/` for scripts running inside the LXC (and thus in a Docker context).
  - `scripts/shared/` for code used across different environments.

## 5. Code Documentation & Arguments

- **Extensive Code Comments:** All code requires extensive documentation strings and comments. Explain *why* a piece of code exists or why a specific approach was taken, not just *what* it does.
- **English Only:** All code documentation, comments, docstrings, and commit messages MUST be written in English.
- **Help Menus:** Every significant script must support `--help` and `-h` arguments via `getopts` to explain usage and available flags.
- **Automation (CLI flags):** Make interactive scripts suitable for full automation by adding optional CLI flags that can skip prompts.
- **Readme Updates:** If you add, remove, or significantly change a script or its flags, `docs/README.md` (and `docs/LLM_CONTEXT.md` if relevant) must be updated immediately so documentation does not lag behind reality.