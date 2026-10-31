//! The nightly round's planning: when the night window is, which backups are due and what the next task is.

/// H12: pure scheduler decisions, extracted so the clock logic is testable.
/// `local_hour: None` (a failed/weird `date`) skips LOUDLY via the caller —
/// the old code folded it into 255 and silently never fired.
pub(crate) fn parse_local_hour(date_stdout: &str) -> Option<u8> {
    date_stdout.trim().parse::<u8>().ok().filter(|h| *h < 24)
}

/// fix-129 (expert panel, restart-skips-night, 2026-09-27): seconds from
/// start to the scheduler's first look. It waited a whole tick (20 min), so
/// a restart late in the backup hour missed the hour altogether. A minute
/// leaves the boot notification and the network their head start.
pub(crate) const SCHEDULER_FIRST_CHECK_S: u64 = 60;

/// fix-129: how many hours the nightly window lasts, starting at
/// `backup_hour`. The second hour is a catch-up: a daemon restarted (a
/// self-update, a crash, a power cut) in the first hour picks up there what
/// is still due, instead of skipping the night. With `backup_hour = 4` the
/// window ends at 06:00, before the house wakes up.
pub(crate) const NIGHT_WINDOW_HOURS: u8 = 2;

/// fix-129: is `local_hour` inside the nightly window that opens at
/// `cfg_hour`? Wraps midnight.
pub(crate) fn in_night_window(cfg_hour: u8, local_hour: u8) -> bool {
    (local_hour + 24 - cfg_hour) % 24 < NIGHT_WINDOW_HOURS
}

pub(crate) fn backup_due(cfg_hour: u8, local_hour: u8, last_backup: u64, now: u64) -> bool {
    // fix-129: due and not yet run since the last window, anywhere in the
    // window; it was only in the configured hour itself.
    in_night_window(cfg_hour, local_hour) && now.saturating_sub(last_backup) >= 20 * 3600
}

/// fix-nightly-1 (Kenny, 2026-10-09): the nightly's per-stack input is the
/// stack's last NIGHTLY backup, never `StackState::last_backup`. That one
/// also moves for a backup by hand, so a backup in the evening put the stack
/// under the 20 h rule and it missed the night (measured: five stacks on
/// 10-08 to 10-09, every stack on 10-10).
pub(crate) fn nightly_stack_inputs(
    snapshot: &homelab_core::state::HostState,
) -> Vec<(String, bool, u64)> {
    snapshot
        .stacks
        .iter()
        .map(|(n, st)| {
            (
                n.clone(),
                st.enabled,
                snapshot.last_nightly_backup.get(n).copied().unwrap_or(0),
            )
        })
        .collect()
}

/// One unit of work in a nightly run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NightlyTask {
    /// Backup + auto-update this stack.
    Stack(String),
    /// H10: snapshot the host's own crown jewels (vault, state, TLS, intent
    /// repo). ALWAYS part of a nightly run, even when no stack is due —
    /// secrets change on deploys, not on backups.
    HostMeta,
    /// E8: ZFS snapshots + replication of the declared jobs.
    Zfs,
    /// G14: rehearse a restore from one repository, in turn.
    ///
    /// B3 asked for a quarterly trial restore and it was never built. Kenny
    /// declined "write it down as a known limitation" at the Phase-7 gate, so
    /// it rides the round that already runs — a backup nobody has restored is
    /// a hypothesis, and one done by hand on a day somebody thought of it is
    /// a hypothesis with a date on it.
    RestoreDrill,
    /// Route A: ask a device that is on the no-touch list for its own
    /// configuration and store the answer. Rides with the host-meta slot
    /// rather than a slot of its own — it is one small GET, and a device
    /// whose config changed today is exactly a night the vault changed too.
    DeviceConfig,
    /// fix-96: `restic copy` of every repository into the second repository
    /// set, after the backups and before the ZFS replication carries the
    /// pool to HDD18TB.
    SecondCopy,
    /// fix-96: `restic check` of one repository, both copies, in turn.
    IntegrityCheck,
}

/// fix-94, fix-156: after a gateway deploy, whether to read the house's
/// address again. Only a deploy that finished: a failed one may have left
/// CrowdSec half-started, and reloading it then helps nobody. The address
/// comes from a public service (fix-156), not the router.
pub(crate) fn home_address_after_deploy(vmid: u16, ok: bool, gateway_vmid: u16) -> bool {
    ok && vmid == gateway_vmid
}

/// H12 pattern: the whole nightly decision as a pure function, so "does the
/// host-meta backup actually run?" is a test instead of an assumption.
/// `stacks` is (name, enabled, last NIGHTLY backup — fix-nightly-1).
/// What the night needs to know beyond the clock and the stacks: when the
/// host-wide jobs last ran, and which of them exist at all. Grouped because
/// the argument list had grown past the point where a caller could get the
/// order right by reading it.
pub(crate) struct NightlyState {
    pub(crate) last_host_meta: u64,
    pub(crate) last_zfs: u64,
    pub(crate) zfs_configured: bool,
    /// G14: when the last restore drill proved something, and how long a
    /// passed drill counts for.
    pub(crate) last_restore_drill: u64,
    pub(crate) restore_drill_interval_s: u64,
    pub(crate) devices_configured: bool,
    /// fix-96: whether a second copy is configured, when it last ran, and
    /// when the rotating check last ran.
    pub(crate) second_copy_configured: bool,
    pub(crate) last_second_copy: u64,
    pub(crate) last_integrity_check: u64,
}

pub(crate) fn nightly_plan(
    cfg_hour: u8,
    local_hour: u8,
    now: u64,
    stacks: &[(String, bool, u64)],
    st: &NightlyState,
) -> Vec<NightlyTask> {
    let mut plan = Vec::new();
    for (name, enabled, last_backup) in stacks {
        // H8: parked stacks sit out the nightly rotation entirely.
        if *enabled && backup_due(cfg_hour, local_hour, *last_backup, now) {
            plan.push(NightlyTask::Stack(name.clone()));
        }
    }
    if backup_due(cfg_hour, local_hour, st.last_host_meta, now) {
        plan.push(NightlyTask::HostMeta);
    }
    if st.zfs_configured && backup_due(cfg_hour, local_hour, st.last_zfs, now) {
        plan.push(NightlyTask::Zfs);
    }
    if st.devices_configured && backup_due(cfg_hour, local_hour, st.last_host_meta, now) {
        plan.push(NightlyTask::DeviceConfig);
    }
    // G14: at most one per round, and only in the nightly window (fix-129)
    // — a restore pulls a whole snapshot back over the same link the
    // backups just used.
    if in_night_window(cfg_hour, local_hour)
        && homelab_core::ops::restoredrill::due(
            st.last_restore_drill,
            now,
            st.restore_drill_interval_s,
        )
    {
        plan.push(NightlyTask::RestoreDrill);
    }
    // fix-96: once a night each, in the backup hour.
    if st.second_copy_configured && backup_due(cfg_hour, local_hour, st.last_second_copy, now) {
        plan.push(NightlyTask::SecondCopy);
    }
    if local_hour == cfg_hour
        && homelab_core::ops::restoredrill::due(
            st.last_integrity_check,
            now,
            homelab_core::ops::secondcopy::DEFAULT_CHECK_INTERVAL_S,
        )
    {
        plan.push(NightlyTask::IntegrityCheck);
    }
    plan
}
