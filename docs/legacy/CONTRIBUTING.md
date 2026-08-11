
# Contributing Guidelines — GitOps Proxmox Homelab (Rust Architecture)

Welcome! This document describes the core philosophy, design principles, and best practices for contributing to this project. Whether you are a human contributor or an AI assistant, **follow these guidelines for every Rust module, TUI, or configuration you write or modify.**

---

## 1. Rust-First, Modular, and DRY

- All new code must be written in Rust (unless otherwise specified for a specific tool or integration).
- Avoid duplicate logic. Use modules, traits, and shared libraries for all reusable code.
- Prefer composition and clear separation of concerns. Each tier (CLIENT, HOST, LXC) should have its own well-defined modules.
- Use Rust’s error handling idioms (`Result`, `anyhow`, `color-eyre`) and avoid panics in production code.

---

## 2. TUI/UX Standards (Ratatui)

- All terminal UIs must use Ratatui for a consistent, modern experience.
- Use a centralized styling module for colors, layout, and feedback (Cyan/Magenta accents, dark backgrounds, animated spinners, floating modals, etc.).
- Ensure accessibility: clear contrast, keyboard navigation, and fallback for non-TTY environments.
- No use of Gum or shell-based TUI libraries.

---

## 3. Documentation & Code Comments

- All code, modules, and public functions must be documented in English.
- Every significant change must update the relevant documentation:
  - `docs/architecture.md` for global rules and architecture
  - The appropriate tier doc (`client-features.md`, `host-features.md`, `lxc-features.md`)
  - `docs/README.md` for user-facing changes
- Explain *why* a design or approach was chosen, not just *what* it does.

---

## 4. GitOps, Safety, and Idempotency

- All infrastructure, secrets, and destructive actions must be managed via code and GitOps — never by manual intervention or ad-hoc scripts.
- All changes must be idempotent: running the same operation multiple times should always produce the same result.
- Never hardcode secrets or credentials. Use the Ephemeral Secrets Container model for all secret injection.
- Destructive actions (deletes, wipes, resets) must require explicit, multi-step confirmation in the TUI.

---

## 5. Testing, CI, and Code Quality

- All Rust code must include unit tests for critical logic and error handling.
- Use `cargo test` and `cargo clippy` to ensure code quality before submitting changes.
- PRs should be focused: one logical change per PR, with clear commit messages.
- All code must pass CI checks before merging.

---

## 6. Contribution Process

- Open an issue or discussion for major changes before submitting a PR.
- Keep PRs small, focused, and easy to review.
- Do not commit secrets, unencrypted `.env` files, or personal configuration.
- Match the style, modularity, and documentation standards of the existing codebase.

---

## 7. Reference Docs

- For architecture, tier responsibilities, and advanced requirements, always consult:
  - `docs/architecture.md`
  - `docs/client-features.md`
  - `docs/host-features.md`
  - `docs/lxc-features.md`

---
