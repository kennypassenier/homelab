//! Native-service operations: adopt a hand-built container (C7), install,
//! update, roll back, back up and restore a systemd service that lives
//! inside an LXC container rather than a compose stack.
//!
//! One operation is a plan of named steps run through the shared runner
//! (AR3); every side effect goes through the Executor (AR2). Each file is
//! named for what it does:
//!
//! - `adopt*`, `install*`: taking a service over, putting a binary on it
//! - `update*`, `rollback`, `release*`: supervised updates and releases
//! - `backup*`, `restore*`, `staging`, `chassis`: archive and bring back data
//! - `scripts`, `glibc`, `helpers`, `seal`, `steps`: shared building blocks

mod adopt;
mod adopt_record;
mod adopt_verify;
mod backup;
mod backup_pause;
mod backup_prepare;
mod backup_snapshot;
mod chassis;
mod glibc;
mod helpers;
mod install;
mod install_activate;
mod install_config;
mod install_scripts;
mod install_stage;
mod release;
mod release_install;
mod release_plan;
mod release_steps;
mod release_verify;
mod restore;
mod restore_empty;
mod restore_steps;
mod rollback;
mod scripts;
mod seal;
mod staging;
mod steps;
mod update;
mod update_current;
mod update_steps;

pub use adopt::{ADOPT_STEPS, adopt, adopt_plan_names};
pub use backup::{backup_native, backup_native_plan_names};
pub use backup_prepare::MAX_OWN_COPY_AGE_S;
pub use chassis::{
    CHASSIS_HEARTBEAT_INTERVAL_S, CHASSIS_PAUSE_FOR_S, ChassisPauseOutcome,
    MAX_CHASSIS_PAUSE_FOR_S, decide_chassis_pause,
};
pub use glibc::{glibc_probe_script, glibc_verdict};
pub use helpers::{merge_staged_binaries, select_unit};
pub use install::{INSTALL_STEPS, install_native, install_plan_names};
pub use install_scripts::{create_dirs_script, own_program_dir_script, seed_env_script, unit_user};
pub use release::{
    ReleaseListItem, ReleaseRefs, list_releases, listed_sha, newest_signed_release,
    parse_latest_release, valid_tag,
};
pub use release_install::{install_release, release_update};
pub use release_plan::{RELEASE_UPDATE_STEPS, release_install_plan_names, release_update_plan_len};
pub use restore::{restore_native, restore_native_plan_names};
pub use restore_empty::{EmptyUnit, restore_empty_unit};
pub use rollback::{ROLLBACK_NATIVE_STEPS, rollback_native};
pub use scripts::{
    drop_stale_rollback_script, health_script, keep_one_previous_script, preserve_script,
    rollback_script,
};
pub use seal::seal_one;
pub use staging::fits_staging;
pub use update::{UPDATE_NATIVE_STEPS, update_native, update_native_plan_names, update_skip_note};
