//! The nightly round's per-stack decisions.
//!
//! host-monolith-untested (expert panel, 2026-09-27): these lived inline in
//! the host's `scheduler_loop`, which no test reached, and every backup and
//! update fault the panel found depends on them. The host keeps the I/O
//! (running the operations, writing state, logging); what to do for one stack
//! on one night is decided here, from what the night already knows.

use crate::manifest::StackManifest;
use crate::native::NativeServiceManifest;
use crate::ops::backup::NightBackup;
use crate::sink::Level;
use crate::state::StackState;

/// What tonight's backup batch backs up for one due stack.
#[derive(Debug, Clone)]
pub enum BackupWork {
    Compose(Box<StackManifest>),
    /// T5: every service on the container.
    Native(Vec<NativeServiceManifest>),
}

/// The backup work for a due stack; `None` for a compose stack whose
/// manifest was never stored (there is nothing to back up from).
pub fn backup_work(st: &StackState) -> Option<BackupWork> {
    if st.is_native() {
        Some(BackupWork::Native(st.natives.clone()))
    } else {
        st.manifest
            .clone()
            .map(|m| BackupWork::Compose(Box::new(m)))
    }
}

/// The updates tonight runs for one stack, after the backup batch.
#[derive(Debug, Clone)]
pub enum UpdateWork {
    None,
    Compose(Box<StackManifest>),
    /// fix-58: per service, along its policy. Both lists may be empty.
    Native {
        /// B1: the orchestrator's own signature-checked release update.
        release: Vec<NativeServiceManifest>,
        /// The service's own `update_cmd` under the armed rollback.
        own_cmd: Vec<NativeServiceManifest>,
    },
}

/// One stack's night, decided.
#[derive(Debug, Clone)]
pub struct StackNight {
    /// Write `last_backup = now`: only for a backup that happened.
    pub record_last_backup: bool,
    pub updates: UpdateWork,
    /// Hand the updates' outcome to `enable::after_night` (H8, fix-59). Set
    /// whenever updates were due to run: a compose stack's update, or a
    /// native stack's night even when no service had an update path (as the
    /// scheduler always did).
    pub settle_park: bool,
    /// What the scheduler logs for this stack.
    pub lines: Vec<(Level, String)>,
}

impl StackNight {
    fn nothing() -> Self {
        StackNight {
            record_last_backup: false,
            updates: UpdateWork::None,
            settle_park: false,
            lines: Vec::new(),
        }
    }
}

fn parked_line(name: &str) -> String {
    format!(
        "scheduler: automatic updates of {} are parked — skipped; \
         `homelab enable {}` resumes them",
        name, name
    )
}

/// Decide one stack's night.
///
/// `due` is whether tonight's plan names the stack, `parked` whether a
/// failed night parked its updates (fix-59), `backup` what the batch did
/// for it (a stack the batch did not report on counts as failed).
pub fn stack_night(
    name: &str,
    st: &StackState,
    due: bool,
    parked: bool,
    backup: &NightBackup,
) -> StackNight {
    let mut night = StackNight::nothing();
    if !due {
        if !st.enabled {
            // H8: parked stack — no nightly backup, no auto-update.
            night.lines.push((
                Level::Info,
                format!("scheduler: stack {} is disabled — skipped", name),
            ));
        }
        return night;
    }
    if st.is_native() {
        // T5: several services share the container and a fate; the park
        // below is per stack.
        night.lines.push((
            Level::Info,
            format!(
                "scheduler: nightly run for {} ({} native service(s))",
                name,
                st.natives.len()
            ),
        ));
        let services: &[NativeServiceManifest] = if parked {
            night.lines.push((Level::Info, parked_line(name)));
            &[]
        } else if !backup.allows_update() {
            night
                .lines
                .push((Level::Info, backup.update_skip_line(name)));
            &[]
        } else {
            &st.natives
        };
        night.updates = UpdateWork::Native {
            release: services
                .iter()
                .filter(|n| n.nightly_updates().release)
                .cloned()
                .collect(),
            own_cmd: services
                .iter()
                .filter(|n| n.nightly_updates().own_cmd)
                .cloned()
                .collect(),
        };
        night.record_last_backup = backup.records_a_timestamp();
        night.settle_park = true;
        return night;
    }
    let Some(manifest) = st.manifest.clone() else {
        night.lines.push((
            Level::Warn,
            format!("scheduler: stack {} has no stored manifest — skipped", name),
        ));
        return night;
    };
    night
        .lines
        .push((Level::Info, format!("scheduler: nightly run for {}", name)));
    night.record_last_backup = backup.records_a_timestamp();
    if parked {
        night.lines.push((Level::Info, parked_line(name)));
    } else if !backup.allows_update() {
        night
            .lines
            .push((Level::Info, backup.update_skip_line(name)));
    } else {
        night.updates = UpdateWork::Compose(Box::new(manifest));
        night.settle_park = true;
    }
    night
}
