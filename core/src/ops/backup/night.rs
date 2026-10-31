//! What one stack's nightly backup did, and the scheduler's phase line.

/// What one stack's nightly backup did.
///
/// The third state is the point. A backup that stood aside because somebody
/// was watching television did not run — so no timestamp may be recorded, or
/// the staleness check goes quiet about a backup that never happened — and
/// did not fail — so H8 must not park the stack, or the house gets punished
/// for using its own services. A `bool` can only say one of those two wrong
/// things, which is why the deferral needed a state of its own (F280).
///
/// The verdicts live here rather than in the scheduler because they are the
/// decision, and the scheduler is the I/O around it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NightBackup {
    Done,
    Deferred(String),
    Failed,
}

impl NightBackup {
    /// Read an operation's outcome. `deferred` is only ever set together with
    /// a false `ok`, but this does not depend on that: a report claiming both
    /// counts as done, because something did run.
    pub fn of(ok: bool, deferred: Option<&str>) -> Self {
        if ok {
            NightBackup::Done
        } else if let Some(why) = deferred {
            NightBackup::Deferred(why.to_string())
        } else {
            NightBackup::Failed
        }
    }

    /// May tonight's automatic updates of this stack run? Only after a backup
    /// that happened.
    ///
    /// fix-60: the update waits for a night with a backup; a stack that
    /// keeps standing aside is escalated by the backup-age check, not
    /// updated blind. Story: `docs/deployment/REGISTER.md`.
    pub fn allows_update(&self) -> bool {
        matches!(self, NightBackup::Done)
    }

    /// The log line for a stack whose updates this backup outcome held back.
    pub fn update_skip_line(&self, stack: &str) -> String {
        let why = match self {
            NightBackup::Done => "it ran".to_string(),
            NightBackup::Deferred(why) => format!("it stood aside ({})", why),
            NightBackup::Failed => "it failed".to_string(),
        };
        format!(
            "scheduler: automatic updates of {} skipped tonight — no backup of tonight to go \
             back to: {}",
            stack, why
        )
    }

    /// May a `last_backup` timestamp be written? Only for work that happened.
    pub fn records_a_timestamp(&self) -> bool {
        matches!(self, NightBackup::Done)
    }

    /// T5: services sharing one container share a fate — the stack's night is
    /// as bad as its worst service. A failure outranks a deferral outranks a
    /// completed backup.
    pub fn worse_of(self, other: NightBackup) -> NightBackup {
        match (self, other) {
            (NightBackup::Failed, _) | (_, NightBackup::Failed) => NightBackup::Failed,
            (d @ NightBackup::Deferred(_), _) => d,
            (_, other) => other,
        }
    }
}

/// M-T75: the nightly backup phase says how long it took, in minutes and
/// seconds (hours past sixty minutes), so the concurrency setting can be
/// judged against a measurement instead of a prediction. One at a time
/// measured 38 min on 2026-09-02; three at a time was predicted at about
/// 13 min and never read.
pub fn phase_duration_line(secs: u64, stacks: usize, at_a_time: usize) -> String {
    let human = if secs >= 3600 {
        format!(
            "{} h {} min {} s",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    } else if secs >= 60 {
        format!("{} min {} s", secs / 60, secs % 60)
    } else {
        format!("{} s", secs)
    };
    format!(
        "scheduler: backup phase took {} for {} stack(s), {} at a time",
        human, stacks, at_a_time
    )
}
