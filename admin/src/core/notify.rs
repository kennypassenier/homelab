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
//!
//! Decision "Notifications and Grafana" (Kenny, 2026-09-30): everything lands
//! here, the host's operations and nightly checks (`import_host`) and
//! Alertmanager's alerts (`alert_drafts`) beside the dashboard's own; only
//! the urgent is pushed at once, at its source (`route`); every notice says
//! what, since when, the consequence and what to do, with its page; a remedy
//! the dashboard can run carries a Fix (`fix_for`); and a digest goes out at
//! 09:00 when something waits (`digest_due`, `digest`).

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
    /// One of the host's own notices: an operation's outcome, the boot
    /// notice, a parked stack (`Command::Notices`).
    HostEvent,
    /// The host's nightly fleet check.
    FleetCheck,
    /// An Alertmanager alert that fires …
    Alert,
    /// … and its end.
    AlertResolved,
    /// replace-kuma: the dashboard's minute watch saw a service, a
    /// container or the host stop answering for five minutes …
    Down,
    /// … and answer again.
    Up,
}

/// How bad a notice is, worst first (the digest's order).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Urgent: it went to the phone at once.
    Critical,
    /// Something failed or needs a look, not urgent.
    Warning,
    #[default]
    Info,
    /// Done, nothing to do.
    Ok,
}

/// Kenny, 2026-09-30 09:16: a remedy the dashboard can run, as the action
/// dialog it opens (prefilled; the review and Confirm still decide).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fix {
    /// An action slug (`core::actions::ActionKind::slug`).
    pub action: String,
    /// The stack, or `_host` for a host-wide action.
    pub stack: String,
    /// The dialog's fields, prefilled.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
    /// The action's label, for the button ("Fix: Deploy").
    pub label: String,
}

/// An old `notifications.json` holds no `push_short`; read as not urgent
/// rather than claim a push that was never recorded either way.
fn push_short_default() -> String {
    "No push · not urgent".into()
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
    /// Owner decision 2026-09-30 (item 3): the table's own short push
    /// status ("Pushed", "No push · succeeded", "Push failed", …), derived
    /// from `push`. `push`'s own `why` (on `Skipped`/`Failed`) stays the
    /// full reason for the row a reader expands.
    #[serde(default = "push_short_default")]
    pub push_short: String,
    #[serde(default)]
    pub level: Level,
    /// Since when it is so (unix seconds): an operation's start, an alert's
    /// `startsAt`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consequence: Option<String>,
    /// What to do: the exact command or the dashboard button.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remedy: Option<String>,
    /// The dashboard page that acts on it (`/app/...`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// Who said so: "host", "alertmanager"; None for the dashboard's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// What makes two notices the same thing (an alert's fingerprint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixes: Vec<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PushOutcome {
    Sent,
    Failed {
        why: String,
    },
    Skipped {
        why: String,
    },
    /// Pushed (or not) where it came from: the host, Alertmanager.
    BySender {
        who: String,
    },
}

impl PushOutcome {
    /// Owner decision 2026-09-30 (item 3): the notifications table's own
    /// short push column ("Pushed", "No push · succeeded", "Push failed",
    /// …); the full reason stays on the notice itself (`why`, `Skipped`'s
    /// own field) for the row a reader expands.
    pub fn short(&self) -> &'static str {
        match self {
            PushOutcome::Sent | PushOutcome::BySender { .. } => {
                homelab_core::notify::push_status_short(true, false, "")
            }
            PushOutcome::Failed { .. } => homelab_core::notify::push_status_short(false, true, ""),
            PushOutcome::Skipped { why } => {
                homelab_core::notify::push_status_short(false, false, why)
            }
        }
    }
}

/// feat-ops-9's switches, and the digest's time.
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
    /// Decision daily-digest: the local time (`HH:MM`, Europe/Brussels) of
    /// the daily digest; None: no digest.
    #[serde(default = "d_digest")]
    pub digest_at: Option<String>,
    /// feat-ops-8's "push a long action" threshold, retired by decision
    /// notify-routing (2026-09-30: only urgent is pushed). Read so a file
    /// from before still loads; never written.
    #[serde(default, rename = "long_action_s", skip_serializing)]
    pub retired_long_action_s: Option<u64>,
}

fn yes() -> bool {
    true
}
fn d_digest() -> Option<String> {
    Some(DIGEST_AT.into())
}

/// The digest's default time (Kenny, 2026-09-30).
pub const DIGEST_AT: &str = "09:00";

impl Default for Settings {
    fn default() -> Self {
        Settings {
            push: true,
            muted_stacks: Vec::new(),
            snooze_until: None,
            digest_at: d_digest(),
            retired_long_action_s: None,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(t) = &self.digest_at {
            if super::schedule::parse_hhmm(t).is_none() {
                return Err("digest_at is a time HH:MM, 00:00 to 23:59, or null for none".into());
            }
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
    /// The newest host notice read (`HostNotice::seq`).
    #[serde(default)]
    pub host_cursor: u64,
    /// False until the host's notices were first read: what the host kept
    /// before is history, not news.
    #[serde(default)]
    pub host_seeded: bool,
    /// The last daily digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_digest: Option<DigestRecord>,
}

/// What became of one day's digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestRecord {
    /// The local day, `YYYY-MM-DD`.
    pub day: String,
    pub at: i64,
    /// How many things waited.
    pub count: usize,
    pub push: PushOutcome,
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
            host_cursor: 0,
            host_seeded: false,
            last_digest: None,
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
    /// How long the action ran.
    pub ran_s: Option<u64>,
    pub detail: Detail,
}

/// Decision notify-detail: the rest of what a notice says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Detail {
    pub level: Level,
    pub since: Option<i64>,
    pub consequence: Option<String>,
    pub remedy: Option<String>,
    /// A dashboard path.
    pub link: Option<String>,
    pub source: Option<String>,
    pub key: Option<String>,
    /// The operation's kind for the routing decision ("backup", "deploy"),
    /// when `op` alone does not say it (a missed schedule).
    pub label: Option<String>,
    pub fixes: Vec<Fix>,
}

impl Draft {
    pub fn new(kind: Kind, op: &str, title: &str, body: &str) -> Draft {
        Draft {
            kind,
            op: op.to_string(),
            stack: None,
            title: title.to_string(),
            body: body.to_string(),
            job: None,
            ran_s: None,
            detail: Detail::default(),
        }
    }
}

/// Whether a notice goes to the phone, and whether pages pop it up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub push: Result<(), String>,
    pub pop_up: bool,
}

/// Decision notify-routing for the dashboard's own notices: a failed
/// operation is pushed by the host at its source, an alert by Alertmanager;
/// the dashboard itself pushes only a missed schedule of urgent work.
pub fn draft_urgency(d: &Draft) -> homelab_core::notify::Urgency {
    use homelab_core::notify::{urgency, Event, Urgency};
    match d.kind {
        Kind::ScheduleMissed => urgency(&Event::Op {
            label: d.detail.label.as_deref().unwrap_or(""),
            ok: false,
            deferred: false,
        }),
        Kind::ActionFailed | Kind::Incident => Urgency {
            urgent: false,
            why: "not pushed here: the host pushes a failed operation itself",
        },
        Kind::ActionDone => Urgency {
            urgent: false,
            why: "not urgent: it succeeded",
        },
        Kind::ActionDeferred => Urgency {
            urgent: false,
            why: "not urgent: it stood aside, nothing broke",
        },
        // replace-kuma: "a service that has not answered for more than 5
        // minutes" is urgent (notify-routing); its return goes where its
        // loss went, as a resolved alert does.
        Kind::Down | Kind::Up => urgency(&Event::Alert {
            alertname: homelab_core::notify::SERVICE_DOWN_ALERTS[0],
        }),
        Kind::HostEvent | Kind::FleetCheck | Kind::Alert | Kind::AlertResolved => Urgency {
            urgent: false,
            why: "pushed at its source",
        },
    }
}

/// The one decision about a notice. Order: snooze (silences everything),
/// what the notice is (only urgent pushes), the global switch, the stack's.
/// Pages pop up what needs a look and the dashboard's own action results.
pub fn route(s: &Settings, d: &Draft, now: i64) -> Route {
    if s.snoozed(now) {
        return Route {
            push: Err("snoozed".into()),
            pop_up: false,
        };
    }
    let u = draft_urgency(d);
    let push = if !u.urgent {
        Err(u.why.to_string())
    } else if !s.push {
        Err("pushes are off".into())
    } else if d
        .stack
        .as_ref()
        .is_some_and(|st| s.muted_stacks.contains(st))
    {
        Err("this stack's pushes are off".into())
    } else {
        Ok(())
    };
    let pop_up = matches!(d.detail.level, Level::Critical | Level::Warning)
        || matches!(
            d.kind,
            Kind::ActionDone
                | Kind::ActionFailed
                | Kind::ActionDeferred
                | Kind::ScheduleMissed
                | Kind::Incident
        );
    Route { push, pop_up }
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

    /// Store a notice; the oldest fall off past `keep` (`KEEP` is the
    /// default, overridden by `HOMELAB_ADMIN_NOTIFY_KEEP`).
    pub fn add(&mut self, d: Draft, at: i64, push: PushOutcome, keep: usize) -> Notice {
        let n = Notice {
            id: self.next_id,
            at,
            kind: d.kind,
            stack: d.stack,
            title: d.title,
            body: d.body,
            job: d.job,
            read: false,
            push_short: push.short().to_string(),
            push,
            level: d.detail.level,
            since: d.detail.since,
            consequence: d.detail.consequence,
            remedy: d.detail.remedy,
            link: d.detail.link,
            source: d.detail.source,
            key: d.detail.key,
            fixes: d.detail.fixes,
        };
        self.next_id += 1;
        self.notices.push(n.clone());
        if self.notices.len() > keep {
            let cut = self.notices.len() - keep;
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

    /// Snooze everything for `seconds` from `now` (at most `snooze_max_s`,
    /// `SNOOZE_MAX_S` by default, a week); 0 ends the snooze.
    pub fn snooze(
        &mut self,
        now: i64,
        seconds: i64,
        snooze_max_s: i64,
    ) -> Result<Option<i64>, String> {
        if !(0..=snooze_max_s).contains(&seconds) {
            return Err(format!("a snooze lasts 0 to {snooze_max_s} seconds"));
        }
        self.settings.snooze_until = (seconds > 0).then_some(now + seconds);
        Ok(self.settings.snooze_until)
    }
}

/// An incident bundle's name is `<unix>-<op>`; the notice says which op.
pub fn incident_draft(name: &str) -> Draft {
    let op = name.split_once('-').map(|(_, op)| op).unwrap_or(name);
    let mut d = Draft::new(
        Kind::Incident,
        op,
        &format!("Incident: {op}"),
        &format!(
            "The host kept an incident bundle ({name}); `homelab incidents show {name}` prints it."
        ),
    );
    d.detail.level = Level::Warning;
    d.detail.remedy = Some(format!(
        "`homelab incidents show {name}` prints what happened."
    ));
    d.detail.link = Some(homelab_core::notify::page::JOBS.into());
    d
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

/// The kyu payload of a notice: the host's shape (core `push_payload`),
/// sent as `homelab-admin`, with the notice's kind as the label, the short
/// text (title and what to do) and the link to its page.
pub fn push_payload(d: &Draft, version: &str, base_url: &str) -> String {
    let label = match d.kind {
        Kind::ActionDone => "action-done",
        Kind::ActionFailed => "action-failed",
        Kind::ActionDeferred => "action-deferred",
        Kind::ScheduleMissed => "schedule-missed",
        Kind::Incident => "incident",
        Kind::HostEvent => "host-event",
        Kind::FleetCheck => "fleet-check",
        Kind::Alert => "alert",
        Kind::AlertResolved => "alert-resolved",
        Kind::Down => "down",
        Kind::Up => "up",
    };
    let ok = matches!(d.kind, Kind::ActionDone | Kind::ActionDeferred | Kind::Up);
    let short =
        homelab_core::notify::push_short(&d.title, d.detail.remedy.as_deref().unwrap_or(&d.body));
    let page = d
        .detail
        .link
        .as_deref()
        .unwrap_or(homelab_core::notify::page::NOTIFICATIONS);
    homelab_core::notify::push_payload(
        "homelab-admin",
        &d.op,
        label,
        ok,
        (!ok).then_some(short.as_str()),
        version,
        Some(&homelab_core::notify::click_url(base_url, page)),
    )
}

// ── Fix: a remedy the dashboard can run (Kenny, 2026-09-30 09:16) ───────

/// What a Fix is looked for in.
#[derive(Debug, Clone, Copy)]
pub enum FixSource<'a> {
    /// A host operation's outcome; `label` is its kind, `stack` its stack.
    Op {
        op: &'a str,
        label: &'a str,
        ok: bool,
        deferred: bool,
        stack: Option<&'a str>,
    },
    /// A remedy in words (a fleet-check finding, a Today item): the first
    /// `homelab <verb> …` in it that the dashboard runs.
    Text(&'a str),
    /// An alert, with the container it names (`host` label) and its remedy.
    Alert {
        alertname: &'a str,
        host: Option<&'a str>,
        remedy: &'a str,
    },
    /// A dashboard job that failed: the same action again, when it is one
    /// that fixes (not a destroy, a restore or an exec).
    Retry { action: &'a str, stack: &'a str },
}

/// The one table: a `homelab` verb and the dashboard action that does the
/// same, and whether it takes a stack. `release-update` is the host's own.
const VERBS: &[(&str, &str, bool)] = &[
    ("deploy", "deploy", true),
    ("backup", "backup", true),
    ("update", "update", true),
    ("enable", "enable", true),
    ("backup-native", "backup-native", true),
    ("update-native", "update-native", true),
    ("release-update-native", "release-update-native", true),
    ("rollback-native", "rollback-native", true),
    ("patch", "patch", false),
    ("backup-host-meta", "backup-host-meta", false),
    ("backup-devices", "backup-devices", false),
    ("zfs-replicate", "zfs-replicate", false),
    ("release-update", "update-host", false),
];

/// A failed operation's retry, as the verb it would be typed with.
fn op_verb(label: &str) -> Option<&'static str> {
    Some(match label {
        "deploy" => "deploy",
        "backup" | "scheduled-backup" => "backup",
        "backup-native" | "scheduled-backup-native" => "backup-native",
        "update" | "scheduled-update" => "update",
        "update-native" | "scheduled-update-native" => "update-native",
        "self-update" => "release-update",
        "patch" => "patch",
        "host-meta-backup" => "backup-host-meta",
        "device-backup" => "backup-devices",
        "zfs-replicate" => "zfs-replicate",
        _ => return None,
    })
}

fn fix_of(verb: &str, stack: Option<&str>) -> Option<Fix> {
    let (_, action, takes_stack) = VERBS.iter().find(|(v, _, _)| *v == verb)?;
    let kind = super::actions::ActionKind::from_slug(action)?;
    let stack = if *takes_stack {
        let s = stack?;
        if !super::actions::valid_stack_name(s) {
            return None;
        }
        s.to_string()
    } else {
        super::actions::HOST_TARGET.to_string()
    };
    Some(Fix {
        action: action.to_string(),
        stack,
        args: BTreeMap::new(),
        label: kind.label().to_string(),
    })
}

/// The first `homelab …` command in a text that the dashboard runs.
fn fix_in_text(text: &str) -> Option<Fix> {
    let words: Vec<&str> = text
        .split(|c: char| c.is_whitespace() || c == '`')
        .filter(|w| !w.is_empty())
        .collect();
    let clean = |w: &str| w.trim_end_matches(['.', ',', ';', ':', ')']).to_string();
    for (i, w) in words.iter().enumerate() {
        if *w != "homelab" {
            continue;
        }
        let Some(verb) = words.get(i + 1).map(|v| clean(v)) else {
            continue;
        };
        let arg = words.get(i + 2).map(|a| clean(a));
        if verb == "checks" && arg.as_deref() == Some("answer") {
            let id = words.get(i + 3).map(|a| clean(a));
            let verdict = words.get(i + 4).map(|a| clean(a));
            if let (Some(id), Some(v)) = (id, verdict) {
                let id_ok = !id.is_empty()
                    && id.len() <= 64
                    && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
                if id_ok && (v == "ok" || v == "nok") {
                    let kind = super::actions::ActionKind::AnswerCheck;
                    return Some(Fix {
                        action: kind.slug().into(),
                        stack: super::actions::HOST_TARGET.into(),
                        args: BTreeMap::from([("check".into(), id), ("verdict".into(), v)]),
                        label: kind.label().into(),
                    });
                }
            }
            continue;
        }
        if let Some(f) = fix_of(&verb, arg.as_deref()) {
            return Some(f);
        }
    }
    None
}

/// The one mapping from a finding, an alert or an operation to the
/// dashboard action that fixes it. None when no action does: the remedy
/// then stays words and a command.
pub fn fix_for(s: &FixSource) -> Option<Fix> {
    match *s {
        FixSource::Op {
            op,
            label,
            ok,
            deferred,
            stack,
        } => {
            if ok || deferred {
                return None;
            }
            // H8: a parked stack is enabled again.
            if let Some(parked) = op.strip_prefix("stack-disabled-") {
                return fix_of("enable", Some(parked));
            }
            fix_of(op_verb(label)?, stack)
        }
        FixSource::Text(t) => fix_in_text(t),
        FixSource::Retry { action, stack } => {
            let (verb, _, takes_stack) = VERBS.iter().find(|(_, a, _)| *a == action)?;
            fix_of(verb, takes_stack.then_some(stack))
        }
        FixSource::Alert {
            alertname,
            host,
            remedy,
        } => {
            // A container that stopped answering: deploy reconciles it.
            // Its hostname is `<vmid>-app-<stack>`.
            let stack = host.and_then(|h| h.split_once("-app-").map(|(_, s)| s));
            match alertname {
                "HostDown" => fix_of("deploy", stack),
                _ => fix_in_text(remedy),
            }
        }
    }
}

// ── The host's notices (`Command::Notices`) ─────────────────────────────

/// A host notice as a draft for the centre, and what became of its push
/// (the host decided it at the source).
pub fn host_draft(n: &homelab_core::notify::HostNotice) -> (Draft, PushOutcome) {
    let kind = if n.op == "fleet-check" {
        Kind::FleetCheck
    } else {
        Kind::HostEvent
    };
    let mut d = Draft::new(kind, &n.op, &n.title, &n.what);
    d.stack = n.stack.clone();
    d.detail = Detail {
        level: if n.urgent {
            Level::Critical
        } else if n.ok {
            Level::Ok
        } else if n.deferred {
            Level::Info
        } else {
            Level::Warning
        },
        since: Some(n.since as i64),
        consequence: Some(n.consequence.clone()),
        remedy: Some(n.remedy.clone()),
        link: Some(n.page.clone()),
        source: Some("host".into()),
        key: Some(format!("host-{}", n.seq)),
        label: Some(n.label.clone()),
        fixes: Vec::new(),
    };
    let mut fixes: Vec<Fix> = if n.findings.is_empty() {
        fix_for(&FixSource::Op {
            op: &n.op,
            label: &n.label,
            ok: n.ok,
            deferred: n.deferred,
            stack: n.stack.as_deref(),
        })
        .into_iter()
        .collect()
    } else {
        n.findings
            .iter()
            .filter_map(|f| fix_for(&FixSource::Text(&f.remedy)))
            .collect()
    };
    fixes.dedup();
    d.detail.fixes = fixes;
    let push = match n.push.as_str() {
        "sent" => PushOutcome::BySender {
            who: "the host".into(),
        },
        p if p.starts_with("failed") => PushOutcome::Failed {
            why: format!("the host's push {p}"),
        },
        p => PushOutcome::Skipped {
            why: format!("{} ({})", p, n.routed),
        },
    };
    (d, push)
}

impl NotifyFile {
    /// One host notice into the list. The notice of the dashboard job that
    /// asked for it (`job`) takes the host's words instead of a second
    /// notice being added. Returns the notice as stored.
    pub fn import_host(
        &mut self,
        n: &homelab_core::notify::HostNotice,
        job: Option<u64>,
        at: i64,
        keep: usize,
    ) -> Option<Notice> {
        self.host_cursor = self.host_cursor.max(n.seq);
        let (d, push) = host_draft(n);
        if let Some(j) = job {
            if let Some(x) = self.notices.iter_mut().rev().find(|x| x.job == Some(j)) {
                // The dashboard's title and push stay; the host adds the rest.
                x.level = d.detail.level.min(x.level_hint());
                x.since = d.detail.since;
                x.consequence = d.detail.consequence;
                x.remedy = d.detail.remedy;
                x.link = d.detail.link;
                x.source = d.detail.source;
                x.fixes = d.detail.fixes;
                if !d.body.is_empty() {
                    x.body = d.body;
                }
                return Some(x.clone());
            }
        }
        Some(self.add(d, at, push, keep))
    }
}

impl Notice {
    /// A dashboard notice stored before levels existed reads as Info.
    fn level_hint(&self) -> Level {
        match self.kind {
            Kind::ActionFailed | Kind::Incident | Kind::ScheduleMissed => Level::Warning,
            _ => self.level,
        }
    }
}

// ── Alertmanager (`POST /hooks/alertmanager`) ───────────────────────────

/// One alert of a webhook, ready for the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlertDraft {
    pub draft: Draft,
    pub firing: bool,
    pub push: PushOutcome,
}

/// `2026-09-30T07:00:00Z` (fractions and an offset allowed) as unix seconds.
pub fn rfc3339(s: &str) -> Option<i64> {
    let (date, time) = s.split_once('T')?;
    let mut d = date.split('-');
    let (y, m, dd): (i64, u32, u32) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let (clock, offset) = if let Some(c) = time.strip_suffix('Z') {
        (c, 0)
    } else {
        let at = time.rfind(['+', '-'])?;
        let (c, o) = time.split_at(at);
        let sign = if o.starts_with('-') { -1 } else { 1 };
        let (oh, om) = o[1..].split_once(':')?;
        (
            c,
            sign * (oh.parse::<i64>().ok()? * 3600 + om.parse::<i64>().ok()? * 60),
        )
    };
    let clock = clock.split('.').next()?;
    let mut t = clock.split(':');
    let (hh, mm, ss): (i64, i64, i64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    );
    if y < 1970 {
        return None;
    }
    Some(super::schedule::days_from_civil(y, m, dd) * 86_400 + hh * 3600 + mm * 60 + ss - offset)
}

/// Alertmanager's webhook body as notices: one per alert, firing or
/// resolved, with the rule's summary, description, consequence, remedy
/// and page. A body of another shape gives none.
pub fn alert_drafts(body: &serde_json::Value) -> Vec<AlertDraft> {
    let Some(alerts) = body.get("alerts").and_then(|a| a.as_array()) else {
        return Vec::new();
    };
    let text = |v: &serde_json::Value, k: &str| {
        v.get(k)
            .and_then(|x| x.as_str())
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_string)
    };
    alerts
        .iter()
        .filter_map(|a| {
            let labels = a.get("labels")?;
            let ann = a.get("annotations").cloned().unwrap_or_default();
            let name = text(labels, "alertname")?;
            let firing = text(a, "status").as_deref() != Some("resolved");
            let severity = text(labels, "severity").unwrap_or_else(|| "warning".into());
            let u = homelab_core::notify::urgency(&homelab_core::notify::Event::Alert {
                alertname: &name,
            });
            let summary = text(&ann, "summary").unwrap_or_else(|| name.clone());
            let title = if firing {
                summary
            } else {
                format!("Resolved: {summary}")
            };
            let mut d = Draft::new(
                if firing {
                    Kind::Alert
                } else {
                    Kind::AlertResolved
                },
                &format!("alert-{name}"),
                &title,
                &text(&ann, "description").unwrap_or_default(),
            );
            let remedy = text(&ann, "remedy");
            // nav-decisions (chassis-rs 3.1.0): every dashboard page now
            // lives at the root, so there is no `/app/` marker left to
            // find; take the path after the scheme and host instead (or
            // the whole string when it is already a bare path).
            let link = text(&ann, "click_url").and_then(|u| match u.split_once("://") {
                Some((_, rest)) => rest.find('/').map(|i| rest[i..].to_string()),
                None => u.starts_with('/').then_some(u.clone()),
            });
            d.detail = Detail {
                level: if !firing {
                    Level::Ok
                } else if u.urgent {
                    Level::Critical
                } else if severity == "critical" || severity == "warning" {
                    Level::Warning
                } else {
                    Level::Info
                },
                since: text(a, "startsAt").and_then(|s| rfc3339(&s)),
                consequence: text(&ann, "consequence"),
                remedy: remedy.clone(),
                link,
                source: Some("alertmanager".into()),
                key: text(a, "fingerprint").or_else(|| Some(format!("alert-{name}"))),
                label: Some(name.clone()),
                fixes: if firing {
                    fix_for(&FixSource::Alert {
                        alertname: &name,
                        host: labels.get("host").and_then(|h| h.as_str()),
                        remedy: remedy.as_deref().unwrap_or(""),
                    })
                    .into_iter()
                    .collect()
                } else {
                    Vec::new()
                },
            };
            let push = if u.urgent {
                PushOutcome::BySender {
                    who: "Alertmanager".into(),
                }
            } else {
                PushOutcome::Skipped {
                    why: format!("centre only ({})", u.why),
                }
            };
            Some(AlertDraft {
                draft: d,
                firing,
                push,
            })
        })
        .collect()
}

impl NotifyFile {
    /// Is an alert with this key firing now (its newest notice is a firing
    /// one)?
    fn alert_open(&self, key: &str) -> Option<usize> {
        let i = self
            .notices
            .iter()
            .rposition(|n| n.key.as_deref() == Some(key))?;
        (self.notices[i].kind == Kind::Alert).then_some(i)
    }

    /// One alert into the list. A repeat of an alert that still fires adds
    /// nothing; a resolved one is stored read, and the firing notice it ends
    /// no longer waits.
    pub fn add_alert(&mut self, a: AlertDraft, at: i64, keep: usize) -> Option<Notice> {
        let key = a.draft.detail.key.clone().unwrap_or_default();
        let open = self.alert_open(&key);
        if a.firing {
            if open.is_some() {
                return None;
            }
            return Some(self.add(a.draft, at, a.push, keep));
        }
        if let Some(i) = open {
            self.notices[i].read = true;
        }
        let mut n = self.add(a.draft, at, a.push, keep);
        if let Some(x) = self.notices.iter_mut().rev().find(|x| x.id == n.id) {
            x.read = true;
        }
        n.read = true;
        Some(n)
    }
}

// ── The daily digest (decision daily-digest) ────────────────────────────

/// A digest is not sent more than this long after its time (a dashboard
/// that starts at 22:00 sends no morning digest).
pub const DIGEST_LATE_S: i64 = 3 * 3600;

/// The local day the digest is due for now, if it is: at or after the
/// digest's time, within `digest_late_s` (`DIGEST_LATE_S` by default), and
/// not sent that day yet.
pub fn digest_due(
    s: &Settings,
    last: Option<&DigestRecord>,
    now: i64,
    digest_late_s: i64,
) -> Option<String> {
    let (hh, mm) = super::schedule::parse_hhmm(s.digest_at.as_deref()?)?;
    let (y, m, d, h, min) = super::schedule::to_local(now);
    let day = format!("{y:04}-{m:02}-{d:02}");
    if last.is_some_and(|l| l.day == day) {
        return None;
    }
    let late = (h as i64 * 60 + min as i64 - (hh as i64 * 60 + mm as i64)) * 60;
    (0..=digest_late_s).contains(&late).then_some(day)
}

/// One line the digest can carry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestLine {
    pub level: Level,
    pub text: String,
}

/// The open Today items as digest lines.
pub fn today_lines(t: &homelab_core::ops::today::Today) -> Vec<DigestLine> {
    use homelab_core::ops::today::Level as T;
    t.items
        .iter()
        .map(|i| DigestLine {
            level: match i.level {
                T::Broken => Level::Critical,
                T::Attention => Level::Warning,
            },
            text: i.what.clone(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Digest {
    pub title: String,
    /// Worst first.
    pub lines: Vec<String>,
    pub count: usize,
}

/// What waits: the unread notices and the open Today items, worst first
/// (newest first within a level). None when nothing does.
pub fn digest(notices: &[Notice], today: &[DigestLine]) -> Option<Digest> {
    let mut lines: Vec<(Level, i64, String)> = notices
        .iter()
        .filter(|n| !n.read)
        .map(|n| (n.level_hint().min(n.level), -n.at, n.title.clone()))
        .chain(today.iter().map(|t| (t.level, 0, t.text.clone())))
        .collect();
    if lines.is_empty() {
        return None;
    }
    lines.sort_by_key(|l| (l.0, l.1));
    let count = lines.len();
    let urgent = lines.iter().filter(|l| l.0 == Level::Critical).count();
    Some(Digest {
        title: format!(
            "Homelab: {} thing{} wait{}{}",
            count,
            if count == 1 { "" } else { "s" },
            if count == 1 { "s" } else { "" },
            if urgent > 0 {
                format!(" ({urgent} urgent)")
            } else {
                String::new()
            }
        ),
        lines: lines.into_iter().map(|l| l.2).collect(),
        count,
    })
}

/// The digest's push: one message, the worst first, a link to the list.
pub fn digest_payload(d: &Digest, version: &str, base_url: &str) -> String {
    let body = d.lines.join("; ");
    let short = homelab_core::notify::push_short(&d.title, &body);
    homelab_core::notify::push_payload(
        "homelab-admin",
        "daily-digest",
        "digest",
        false,
        Some(&short),
        version,
        Some(&homelab_core::notify::click_url(
            base_url,
            homelab_core::notify::page::NOTIFICATIONS,
        )),
    )
}
