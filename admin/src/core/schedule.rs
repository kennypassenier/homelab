//! feat-stacks-8 (arch-schedule): actions planned in the dashboard. Any
//! action can be scheduled; the dashboard runs it while CT 120 is up; a slot
//! that passed while it could not run is skipped and notified, never caught
//! up.
//!
//! Times are wall-clock times in Europe/Brussels, the house's zone, so "every
//! day at 03:30" stays 03:30 across the clock changes. The zone's rules are
//! the EU's since 1996: summer time from the last Sunday of March 01:00 UTC
//! to the last Sunday of October 01:00 UTC, UTC+1 otherwise. On the spring
//! day a time inside the skipped hour (02:00-02:59) runs at 03:00, the first
//! minute that exists; on the autumn day a time inside the repeated hour runs
//! once, at its first occurrence. Pure: no clock, no I/O.

use serde::{Deserialize, Serialize};

use super::actions::{ActionArgs, ActionRequest, Refusal};

/// The zone every schedule is in.
pub const ZONE: &str = "Europe/Brussels";

/// Days since 1970-01-01 of a proleptic Gregorian date (H. Hinnant).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day number.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// 0 = Monday … 6 = Sunday. 1970-01-01 was a Thursday.
pub fn weekday(days: i64) -> u32 {
    (days + 3).rem_euclid(7) as u32
}

fn days_in_month(y: i64, m: u32) -> u32 {
    days_from_civil(
        if m == 12 { y + 1 } else { y },
        if m == 12 { 1 } else { m + 1 },
        1,
    ) as u32
        - days_from_civil(y, m, 1) as u32
}

/// The last Sunday of a month, as a day number.
fn last_sunday(y: i64, m: u32) -> i64 {
    let last = days_from_civil(y, m, days_in_month(y, m));
    last - ((weekday(last) + 1) % 7) as i64
}

/// Summer time of year `y`: [start, end) in unix seconds.
pub fn summer_time(y: i64) -> (i64, i64) {
    (
        last_sunday(y, 3) * 86_400 + 3_600,
        last_sunday(y, 10) * 86_400 + 3_600,
    )
}

/// Brussels' offset from UTC at a unix second: 3600 or 7200.
pub fn offset_at(unix: i64) -> i64 {
    let (y, _, _) = civil_from_days(unix.div_euclid(86_400));
    let (start, end) = summer_time(y);
    if unix >= start && unix < end {
        7_200
    } else {
        3_600
    }
}

/// A local wall-clock minute resolved to a unix second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolved {
    Exact(i64),
    /// The autumn hour: both instants carry this local time; the first is
    /// the one that counts.
    Twice(i64, i64),
    /// The spring hour: the time does not exist; this is the first instant
    /// after it (03:00 summer time).
    Skipped(i64),
}

impl Resolved {
    /// The instant a schedule runs at.
    pub fn instant(self) -> i64 {
        match self {
            Resolved::Exact(t) | Resolved::Twice(t, _) | Resolved::Skipped(t) => t,
        }
    }
}

pub fn resolve_local(y: i64, m: u32, d: u32, hh: u32, mm: u32) -> Resolved {
    let local = days_from_civil(y, m, d) * 86_400 + (hh as i64) * 3_600 + (mm as i64) * 60;
    let summer = local - 7_200;
    let winter = local - 3_600;
    let summer_ok = offset_at(summer) == 7_200;
    let winter_ok = offset_at(winter) == 3_600;
    match (summer_ok, winter_ok) {
        (true, true) => Resolved::Twice(summer, winter),
        (true, false) => Resolved::Exact(summer),
        (false, true) => Resolved::Exact(winter),
        (false, false) => Resolved::Skipped(summer_time(y).0),
    }
}

/// The local date, hour and minute of a unix second.
pub fn to_local(unix: i64) -> (i64, u32, u32, u32, u32) {
    let local = unix + offset_at(unix);
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    let s = local.rem_euclid(86_400);
    (y, m, d, (s / 3_600) as u32, ((s % 3_600) / 60) as u32)
}

/// `HH:MM`, 00:00 to 23:59.
pub fn parse_hhmm(s: &str) -> Option<(u32, u32)> {
    let (h, m) = s.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// `YYYY-MM-DD`.
pub fn parse_date(s: &str) -> Option<(i64, u32, u32)> {
    let mut it = s.split('-');
    let (y, m, d) = (it.next()?, it.next()?, it.next()?);
    if it.next().is_some() || y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    let (y, m, d): (i64, u32, u32) = (y.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    ((1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)).then_some((y, m, d))
}

/// When a schedule runs, in Brussels wall-clock time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "every", rename_all = "snake_case", deny_unknown_fields)]
pub enum When {
    /// Every day at `at` (`HH:MM`).
    Day { at: String },
    /// On the listed weekdays (0 = Monday … 6 = Sunday) at `at`.
    Week { days: Vec<u32>, at: String },
    /// Once, on `date` (`YYYY-MM-DD`) at `at`.
    Once { date: String, at: String },
}

impl When {
    pub fn validate(&self) -> Result<(), String> {
        let at = match self {
            When::Day { at } | When::Week { at, .. } | When::Once { at, .. } => at,
        };
        if parse_hhmm(at).is_none() {
            return Err(format!("{at:?} is not a time; write HH:MM, 00:00 to 23:59"));
        }
        if let When::Week { days, .. } = self
            && (days.is_empty() || days.iter().any(|d| *d > 6))
        {
            return Err("days are 0 (Monday) to 6 (Sunday), at least one".into());
        }
        if let When::Once { date, .. } = self
            && parse_date(date).is_none()
        {
            return Err(format!("{date:?} is not a date; write YYYY-MM-DD"));
        }
        Ok(())
    }

    /// The first slot strictly after `after` (unix seconds), None when
    /// there is none (a one-off in the past, or a malformed schedule).
    pub fn next_after(&self, after: i64) -> Option<i64> {
        let (hh, mm) = match self {
            When::Day { at } | When::Week { at, .. } | When::Once { at, .. } => parse_hhmm(at)?,
        };
        if let When::Once { date, .. } = self {
            let (y, m, d) = parse_date(date)?;
            let t = resolve_local(y, m, d, hh, mm).instant();
            return (t > after).then_some(t);
        }
        let (y, m, d, _, _) = to_local(after);
        let today = days_from_civil(y, m, d);
        // Start a day early: the resolved instant of "yesterday's" local date
        // can still lie after `after` near a clock change. Eight days cover
        // any weekly pattern.
        for day in today - 1..=today + 8 {
            if let When::Week { days, .. } = self
                && !days.contains(&weekday(day))
            {
                continue;
            }
            let (y, m, d) = civil_from_days(day);
            let t = resolve_local(y, m, d, hh, mm).instant();
            if t > after {
                return Some(t);
            }
        }
        None
    }

    /// Every slot in (from, to], oldest first, at most `cap` of them.
    pub fn slots_between(&self, from: i64, to: i64, cap: usize) -> Vec<i64> {
        let mut out = Vec::new();
        let mut t = from;
        while out.len() < cap {
            match self.next_after(t) {
                Some(s) if s <= to => {
                    out.push(s);
                    t = s;
                }
                _ => break,
            }
        }
        out
    }
}

/// One planned action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schedule {
    pub id: String,
    pub stack: String,
    pub action: String,
    #[serde(default)]
    pub args: ActionArgs,
    pub when: When,
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Free text the page shows.
    #[serde(default)]
    pub note: String,
    pub created_at: i64,
    /// Every slot up to here is handled: run, or skipped and notified.
    pub handled_until: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run: Option<LastRun>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastRun {
    pub slot: i64,
    /// The dashboard's job id.
    pub job: u64,
}

/// What the browser posts to create or change a schedule.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScheduleInput {
    pub stack: String,
    pub action: String,
    #[serde(default)]
    pub args: ActionArgs,
    pub when: When,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub note: String,
}

/// A schedule's input checked the way a press of the same button would be,
/// plus the time. Destructive actions cannot be scheduled: their typed name
/// is a person's confirmation at the moment of the press.
pub fn check_input(input: &ScheduleInput) -> Result<ActionRequest, Refusal> {
    let req = super::actions::validate(&input.stack, &input.action, input.args.clone())?;
    let what = format!("schedule {} {}", input.action, input.stack);
    if req.action.confirm() || req.action == super::actions::ActionKind::Wipe {
        return Err(Refusal::new(
            what,
            "an action that asks for the typed stack name cannot be scheduled",
            "run it by hand when it is needed",
        ));
    }
    input
        .when
        .validate()
        .map_err(|why| Refusal::new(what, why, "fix the time and save again"))?;
    Ok(req)
}

/// The on-disk file (arch-state).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduleFile {
    pub schema_version: u32,
    pub zone: String,
    #[serde(default)]
    pub schedules: Vec<Schedule>,
}

pub const SCHEDULE_SCHEMA: u32 = 1;

impl Default for ScheduleFile {
    fn default() -> Self {
        ScheduleFile {
            schema_version: SCHEDULE_SCHEMA,
            zone: ZONE.into(),
            schedules: Vec::new(),
        }
    }
}

impl ScheduleFile {
    /// Refuse a file this build does not understand rather than guess.
    pub fn check(&self) -> Result<(), String> {
        if self.schema_version != SCHEDULE_SCHEMA {
            return Err(format!(
                "schedules.json has schema_version {}, this dashboard reads {}",
                self.schema_version, SCHEDULE_SCHEMA
            ));
        }
        if self.zone != ZONE {
            return Err(format!(
                "schedules.json is in zone {:?}; only {ZONE} is supported",
                self.zone
            ));
        }
        Ok(())
    }
}

/// What a tick decides for one schedule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickPlan {
    /// The slot to run now, if one is due.
    pub run: Option<i64>,
    /// Slots that passed without a run: skipped, one notice each.
    pub missed: Vec<i64>,
    /// The schedule's new `handled_until`.
    pub handled_until: i64,
}

/// At `now`, what to do about `s`. A slot is run when it is at most `grace`
/// seconds old; older ones (the dashboard was down, or a run of the same
/// schedule was still going) are skipped. Of several due slots only the
/// newest runs. `busy`: this schedule's previous run has not finished, so
/// its due slot is skipped too.
pub fn tick(s: &Schedule, now: i64, grace: i64, busy: bool) -> TickPlan {
    if !s.enabled {
        return TickPlan {
            run: None,
            missed: Vec::new(),
            handled_until: now.max(s.handled_until),
        };
    }
    let slots = s.when.slots_between(s.handled_until, now, 1_000);
    let mut missed = Vec::new();
    let mut run = None;
    for (i, slot) in slots.iter().enumerate() {
        let newest = i + 1 == slots.len();
        if newest && now - slot <= grace && !busy {
            run = Some(*slot);
        } else {
            missed.push(*slot);
        }
    }
    TickPlan {
        run,
        missed,
        handled_until: now.max(s.handled_until),
    }
}

/// The next slot a schedule will run at, for the page.
pub fn next_run(s: &Schedule, now: i64) -> Option<i64> {
    if !s.enabled {
        return None;
    }
    s.when.next_after(now.max(s.handled_until))
}

/// A local time for messages: `2026-10-25 02:30`.
pub fn local_label(unix: i64) -> String {
    let (y, m, d, hh, mm) = to_local(unix);
    format!("{y:04}-{m:02}-{d:02} {hh:02}:{mm:02}")
}
