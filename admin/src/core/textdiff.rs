//! fix-219 (drift-finding-names-only-filenames, 2026-10-02): the line-diff
//! engine moved to `homelab_core::textdiff` so the CLI (`homelab
//! check`/`today`) and the dashboard's Health page can build the same diff
//! this crate's own Apply page plan always has — one engine, never two that
//! could disagree about what "changed" means. This file stays as a
//! re-export so every existing `super::textdiff::…` path here keeps working
//! unchanged.
pub use homelab_core::textdiff::*;
