//! milestone edit: changing the fleet from the browser.
//!
//! * feat-stacks-2: a stack's settings (and any of its files, raw) with a
//!   plan before the commit: the diff, what homelab would change on the
//!   machines, and what the host last applied; then optionally a deploy of
//!   exactly that commit through the action queue.
//! * feat-firewall-1 / feat-firewall-2: the firewall editor per stack, and
//!   every stack's rules and the fleet's matrix on one page.
//! * feat-stacks-3: a new stack, or a preset's app in an existing stack,
//!   from the repository's presets with the client's own scaffold.
//! * feat-settings-1: every host.toml key, read and written with commands
//!   answered to this session only (the TUI's settings screen keeps its
//!   unsaved edits); arch-self keys are read-only.
//!
//! Every change of the repository goes through the working copy's one
//! transaction (`workcopy`).
//!
//! Layout (one responsibility per file):
//!
//! * `http`, `routes`: the shared route helpers and the router;
//! * `repo`: the working copy's status, sync, unpushed choice, text check;
//! * `check`, `applied`, `planned`, `follow`: validation, the host's applied
//!   files, the plan's JSON, and the commit's follow-up job;
//! * `stack`, `stack_read`: one stack's edit, planned, committed and read;
//! * `presets`, `preset_edit`, `taken`, `new_stack`, `new_appdata`, `import`: presets, new
//!   stacks and bundle imports;
//! * `fleet_read`, `fleet_views`: the firewall matrix, topology and
//!   dependencies;
//! * `host_settings`, `host_save`, `tokens`: host.toml and the tokens.

use std::sync::Arc;

use super::actions::{Actions, HostPort, Publish};
use super::host_link::Shared;
use super::workcopy::WorkingCopy;

mod applied;
mod check;
mod fleet_read;
mod fleet_views;
mod follow;
mod host_save;
mod host_settings;
mod http;
mod import;
mod new_appdata;
mod new_stack;
mod planned;
mod preset_edit;
mod presets;
mod repo;
mod routes;
mod stack;
mod stack_read;
mod taken;
mod tokens;

pub use follow::subject_with_id;
pub use host_save::save_host_settings;
pub use host_settings::{
    HOST_SETTINGS_SINCE, OLD_HOST_SETTINGS_ADVICE, read_host_settings, version_triple,
};
pub use import::{IMPORT_MAX, ImportCommitBody, ImportReq, commit_import, plan_import};
pub use new_appdata::{AppdataBody, appdata_paths};
pub use new_stack::{NewCommitBody, commit_new, plan_new};
pub use preset_edit::{PresetCommitBody, commit_preset, plan_preset};
pub use presets::{read_preset_edit, read_presets};
pub use routes::router;
pub use stack::{CommitBody, EditCommitter, commit_stack, plan_stack};
pub use stack_read::read_stack_edit;
pub use tokens::{IssueTokenBody, issue_token, read_tokens, revoke_token};

#[derive(Clone)]
pub struct EditCtx {
    pub wc: Arc<WorkingCopy>,
    pub actions: Actions,
    pub host: Arc<dyn HostPort>,
    pub shared: Shared,
    pub publish: Arc<dyn Publish>,
}
