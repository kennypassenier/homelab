//! feat-overview-5 (notification center), feat-ops-8 (a push via kyu when a
//! long action finishes) and feat-ops-9 (switches per stack and for all, and
//! a snooze of every notification for a chosen time).
//!
//! The center is a list the dashboard keeps, read and unread, fed by action
//! results, missed schedules and the host's incident bundles. Every notice
//! lands in the list, so nothing is lost while pushes are off or snoozed;
//! the switches and the snooze decide only whether a notice also goes to the
//! phone (through kyu) and whether the open pages pop it up. Pure: the shell
//! stores the file and sends the push.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const NOTIFY_SCHEMA: u32 = 1;
/// The newest notices kept; older ones fall off.
pub const KEEP: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// An action the dashboard ran finished well.
    ActionDone,
    /// … or failed.
    ActionFailed,
    /// … or deliberately did not run (the host stood aside).
    ActionDeferred,
    /// A schedule's slot passed without a run.
    ScheduleMissed,
    /// The host left a new incident bundle.
    Incident,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub id: u64,
    /// Unix seconds.
    pub at: i64,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    pub title: String,
    pub body: String,
    /// The dashboard job it is about, for a link to its progress.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<u64>,
    pub read: bool,
    /// Whether it went to the phone, and if not, why not.
    pub push: PushOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PushOutcome {
    Sent,
    Failed { why: String },
    Skipped { why: String },
}

/// feat-ops-9's switches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    /// Pushes at all.
    #[serde(default = "yes")]
    pub push: bool,
    /// Stacks whose notices never push (they still land in the list).
    #[serde(default)]
    pub muted_stacks: Vec<String>,
    /// Until then (unix seconds) nothing pushes or pops up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snooze_until: Option<i64>,
    /// feat-ops-8: an action that ran at least this long pushes when it
    /// finishes; a shorter one only when it failed. Default 120 s.
    #[serde(default = "d_long")]
    pub long_action_s: u64,
}

fn yes() -> bool {
    true
}
fn d_long() -> u64 {
    120
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            push: true,
            muted_stacks: Vec::new(),
            snooze_until: None,
            long_action_s: d_long(),
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.long_action_s == 0 || self.long_action_s > 86_400 {
            return Err("long_action_s is 1 to 86400 seconds".into());
        }
        if self
            .muted_stacks
            .iter()
            .any(|s| !super::actions::valid_stack_name(s))
        {
            return Err("muted_stacks holds stack names only".into());
        }
        Ok(())
    }

    pub fn snoozed(&self, now: i64) -> bool {
        self.snooze_until.is_some_and(|t| now < t)
    }
}

/// The longest snooze: a week.
pub const SNOOZE_MAX_S: i64 = 7 * 86_400;

/// The on-disk file (arch-state).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotifyFile {
    pub schema_version: u32,
    pub next_id: u64,
    #[serde(default)]
    pub settings: Settings,
    /// Newest last.
    #[serde(default)]
    pub notices: Vec<Notice>,
    /// Incident bundles already turned into a notice.
    #[serde(default)]
    pub seen_incidents: Vec<String>,
    /// False until the first incident list was read: the bundles that exist
    /// then are history, not news.
    #[serde(default)]
    pub incidents_seeded: bool,
}

impl Default for NotifyFile {
    fn default() -> Self {
        NotifyFile {
            schema_version: NOTIFY_SCHEMA,
            next_id: 1,
            settings: Settings::default(),
            notices: Vec::new(),
            seen_incidents: Vec::new(),
            incidents_seeded: false,
        }
    }
}

/// A notice before it is stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub kind: Kind,
    /// What the push names as its operation (`deploy-media`,
    /// `schedule-backup-media`, the incident's op).
    pub op: String,
    pub stack: Option<String>,
    pub title: String,
    pub body: String,
    pub job: Option<u64>,
    /// How long the action ran, for the long-action rule.
    pub ran_s: Option<u64>,
}

/// Whether a notice goes to the phone, and whether pages pop it up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub push: Result<(), String>,
    pub pop_up: bool,
}

/// The one decision about a notice. Order: snooze (silences everything),
/// the global switch, the stack's switch, then what the notice is.
pub fn route(s: &Settings, d: &Draft, now: i64) -> Route {
    if s.snoozed(now) {
        return Route {
            push: Err("snoozed".into()),
            pop_up: false,
        };
    }
    let push = if !s.push {
        Err("pushes are off".into())
    } else if d
        .stack
        .as_ref()
        .is_some_and(|st| s.muted_stacks.contains(st))
    {
        Err("this stack's pushes are off".into())
    } else {
        match d.kind {
            Kind::ActionDone | Kind::ActionDeferred => match d.ran_s {
                Some(r) if r >= s.long_action_s => Ok(()),
                _ => Err(format!("shorter than {} s", s.long_action_s)),
            },
            Kind::ActionFailed | Kind::ScheduleMissed | Kind::Incident => Ok(()),
        }
    };
    Route { push, pop_up: true }
}

impl NotifyFile {
    pub fn check(&self) -> Result<(), String> {
        if self.schema_version != NOTIFY_SCHEMA {
            return Err(format!(
                "notifications.json has schema_version {}, this dashboard reads {}",
                self.schema_version, NOTIFY_SCHEMA
            ));
        }
        Ok(())
    }

    /// Store a notice; the oldest fall off past `KEEP`.
    pub fn add(&mut self, d: Draft, at: i64, push: PushOutcome) -> Notice {
        let n = Notice {
            id: self.next_id,
            at,
            kind: d.kind,
            stack: d.stack,
            title: d.title,
            body: d.body,
            job: d.job,
            read: false,
            push,
        };
        self.next_id += 1;
        self.notices.push(n.clone());
        if self.notices.len() > KEEP {
            let cut = self.notices.len() - KEEP;
            self.notices.drain(..cut);
        }
        n
    }

    /// Mark some notices (or all, with `ids` None) read or unread; how many
    /// changed.
    pub fn mark(&mut self, ids: Option<&[u64]>, read: bool) -> usize {
        let mut n = 0;
        for x in self.notices.iter_mut() {
            if ids.is_none_or(|ids| ids.contains(&x.id)) && x.read != read {
                x.read = read;
                n += 1;
            }
        }
        n
    }

    pub fn unread(&self) -> usize {
        self.notices.iter().filter(|n| !n.read).count()
    }

    /// The incident bundles in `listed` not seen before, oldest first. The
    /// first list ever read only seeds the memory.
    pub fn new_incidents(&mut self, listed: &[String]) -> Vec<String> {
        let mut fresh: Vec<String> = listed
            .iter()
            .filter(|n| !self.seen_incidents.contains(n))
            .cloned()
            .collect();
        fresh.sort();
        // Remember only what the host still lists, so the memory cannot grow
        // past the host's own retention.
        self.seen_incidents = listed.to_vec();
        if !self.incidents_seeded {
            self.incidents_seeded = true;
            return Vec::new();
        }
        fresh
    }

    /// Snooze everything for `seconds` from `now` (at most a week); 0 ends
    /// the snooze.
    pub fn snooze(&mut self, now: i64, seconds: i64) -> Result<Option<i64>, String> {
        if !(0..=SNOOZE_MAX_S).contains(&seconds) {
            return Err(format!("a snooze lasts 0 to {SNOOZE_MAX_S} seconds"));
        }
        self.settings.snooze_until = (seconds > 0).then_some(now + seconds);
        Ok(self.settings.snooze_until)
    }
}

/// An incident bundle's name is `<unix>-<op>`; the notice says which op.
pub fn incident_draft(name: &str) -> Draft {
    let op = name.split_once('-').map(|(_, op)| op).unwrap_or(name);
    Draft {
        kind: Kind::Incident,
        op: op.to_string(),
        stack: None,
        title: format!("Incident: {op}"),
        body: format!(
            "The host kept an incident bundle ({name}); `homelab incidents show {name}` prints it."
        ),
        job: None,
        ran_s: None,
    }
}

/// A per-stack switch as the page toggles it.
pub fn set_stack_muted(s: &mut Settings, stack: &str, muted: bool) {
    s.muted_stacks.retain(|x| x != stack);
    if muted {
        s.muted_stacks.push(stack.to_string());
        s.muted_stacks.sort();
    }
}

/// The unread count per stack, for badges.
pub fn unread_by_stack(f: &NotifyFile) -> BTreeMap<String, usize> {
    let mut m = BTreeMap::new();
    for n in f.notices.iter().filter(|n| !n.read) {
        if let Some(s) = &n.stack {
            *m.entry(s.clone()).or_insert(0) += 1;
        }
    }
    m
}

/// The kyu payload of a notice: the host's shape (core `op_payload`), sent
/// as `homelab-admin`, with the notice's kind as the label.
pub fn push_payload(d: &Draft, version: &str) -> String {
    let label = match d.kind {
        Kind::ActionDone => "action-done",
        Kind::ActionFailed => "action-failed",
        Kind::ActionDeferred => "action-deferred",
        Kind::ScheduleMissed => "schedule-missed",
        Kind::Incident => "incident",
    };
    let ok = matches!(d.kind, Kind::ActionDone | Kind::ActionDeferred);
    let error = (!ok).then_some(d.body.as_str());
    homelab_core::notify::op_payload_from("homelab-admin", &d.op, label, ok, error, version)
}
