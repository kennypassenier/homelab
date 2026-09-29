//! Live view: announce, plan and pause (Kenny, 2026-09-29, form "Live view
//! aankondigen"). The pure half.
//!
//! Before a step changes the screen, every tab that follows shows "Next:
//! <step>" with a short countdown and marks the element the step will act
//! on; the dashboard's server holds the step for the countdown, so every tab
//! sees the same wait and the CLI's answer comes after the step ran. Typing
//! is not announced: the letters appearing one by one already say what
//! happens. A viewer may Pause (the step waits for Continue), Continue, or
//! Stop (the step fails with "stopped by the viewer", the drive ends and
//! every later step is refused until the driver says `homelab ui done`).
//!
//! The driver can send the whole sequence first (`homelab ui plan`); each
//! step taken marks the plan's next one done, and a step that is not the
//! plan's next is still taken but marks the plan "changed".
//!
//! Everything here is state and words; the waiting is the shell's
//! (`shell::drive`).

use homelab_proto::UiStep;
use serde::Serialize;

use crate::core::actions::{ActionKind, Refusal, HOST_TARGET};
use crate::core::drive::{action_form, DriveState, Family};
use crate::core::driveedit::EditKind;

/// How long a step is announced before it is taken, unless the dashboard's
/// settings say otherwise (`HOMELAB_ADMIN_LIVE_ANNOUNCE_MS`).
pub const ANNOUNCE_MS: u64 = 3_000;
/// The longest announcement the settings accept: the host's usual wait for
/// an answer is 20 s, and a step still has to run after its countdown.
pub const ANNOUNCE_MAX_MS: u64 = 10_000;
/// How long a paused step waits for Continue before it fails, unless the
/// settings say otherwise (`HOMELAB_ADMIN_LIVE_MAX_PAUSE_S`).
pub const MAX_PAUSE_S: u64 = 1_800;
/// The longest pause the settings accept; the host holds a step at most
/// `homelab_proto::UI_HOLD_MAX_S`, which leaves room for the step itself.
pub const MAX_PAUSE_MAX_S: u64 = 3_600;
/// The most steps one plan may list.
pub const PLAN_MAX: usize = 200;

/// The step announced now, before it is taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Announce {
    /// Counts announcements, so a tab tells a new one from a repaint.
    pub id: u64,
    pub step: UiStep,
    /// What the step does, in words: "open Deploy · media".
    pub text: String,
    /// False for typing: held while paused, but never counted down.
    pub countdown: bool,
    pub total_ms: u64,
    /// What is left of the countdown when this state was read; frozen while
    /// paused.
    pub left_ms: u64,
}

/// One step of the plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanStep {
    pub step: UiStep,
    pub text: String,
    pub done: bool,
}

/// The sequence the driver sent up front.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub by: String,
    pub steps: Vec<PlanStep>,
    /// The index of the plan's next step; `steps.len()` once all are done.
    pub next: usize,
    /// A step was taken that was not the plan's next.
    pub changed: bool,
}

/// Does this step wait while a viewer has paused? Everything that changes
/// the screen does; reading it, sending the plan and letting go do not.
pub fn holds(step: &UiStep) -> bool {
    !matches!(step, UiStep::State | UiStep::Plan { .. } | UiStep::Done)
}

/// Is this step announced with a countdown? Typing is not (Kenny: the
/// typing was good as it was).
pub fn counts_down(step: &UiStep) -> bool {
    holds(step) && !matches!(step, UiStep::Type { .. } | UiStep::Edit { .. })
}

fn page_name(path: &str) -> String {
    let rest = path
        .trim_start_matches("/app")
        .trim_start_matches('/')
        .trim_end_matches('/');
    let parts: Vec<&str> = rest.split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [] => "the fleet overview".into(),
        ["stacks", name] => format!("the stack {name}"),
        ["stacks", name, tab] => format!("the {tab} tab of {name}"),
        [page] => format!("the {page} page"),
        _ => path.to_string(),
    }
}

fn form_name(form: &str, target: Option<&str>) -> String {
    if let Some(kind) = EditKind::from_form(form) {
        let t = target.unwrap_or("");
        return match kind {
            EditKind::Settings => format!("Settings · {t}"),
            EditKind::Raw => format!("the raw editor of {t}"),
            EditKind::AddApp => format!("Add an app · {t}"),
            EditKind::Firewall => format!("Firewall · {t}"),
            EditKind::NewStack => "the new-stack wizard".into(),
            EditKind::HostSettings => "the host settings".into(),
            EditKind::Batch => format!(
                "the batch {} on {}",
                form.strip_prefix("batch:").unwrap_or("action"),
                t.replace(',', ", ")
            ),
            EditKind::Rollback => format!("Roll back · {t}"),
            EditKind::Import => "the Import dialog".into(),
        };
    }
    match ActionKind::from_slug(form) {
        Some(kind) => action_form(kind, target.unwrap_or(HOST_TARGET)).title,
        None => form.to_string(),
    }
}

fn capital(word: &str) -> String {
    let mut c = word.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// What `step` does, in words, read against what is on screen now: "open
/// Deploy · media", "press Deploy on Deploy · media: the final press".
pub fn describe(step: &UiStep, st: &DriveState) -> String {
    let label = |id: &str| {
        st.form
            .as_ref()
            .and_then(|f| f.fields.iter().find(|x| x.id == id))
            .map(|x| x.label.clone())
            .filter(|l| !l.is_empty())
            .unwrap_or_else(|| id.to_string())
    };
    let title = st.form.as_ref().map(|f| f.title.clone());
    match step {
        UiStep::Goto { path } => format!("go to {}", page_name(path)),
        UiStep::Open { form, target } => format!("open {}", form_name(form, target.as_deref())),
        UiStep::Type { field, .. } => format!("type into {}", label(field)),
        UiStep::Edit { field, .. } => format!("set the whole text of {}", label(field)),
        UiStep::Pick { field, value } => format!("choose {value} for {}", label(field)),
        UiStep::Check { field, on } => {
            format!("{} {}", if *on { "tick" } else { "untick" }, label(field))
        }
        UiStep::Press { button } => {
            let on = title.map(|t| format!(" on {t}")).unwrap_or_default();
            if button == "confirm" {
                let what = match st.form.as_ref().map(|f| f.desc.family) {
                    Some(Family::Action(kind)) => kind.label().to_string(),
                    _ => "Confirm".into(),
                };
                format!("press {what}{on}: the final press, it runs once")
            } else {
                format!("press {}{on}", capital(button))
            }
        }
        UiStep::Row { op, target } => {
            let t = target.as_deref().unwrap_or("");
            let rule = t.parse::<u64>().is_ok();
            match (op.as_str(), rule) {
                ("add", _) => "add a firewall rule".into(),
                ("edit", false) => format!("edit the host setting {t}"),
                ("edit", true) => format!("edit firewall rule {t}"),
                ("up", _) => format!("move firewall rule {t} up"),
                ("down", _) => format!("move firewall rule {t} down"),
                ("delete", _) => format!("delete firewall rule {t}"),
                _ => format!("{op} row {t}"),
            }
        }
        UiStep::Close => match title {
            Some(t) => format!("close {t}"),
            None => "close the dialog".into(),
        },
        UiStep::State => "read the screen".into(),
        UiStep::Done => "hand the dashboard back".into(),
        UiStep::Plan { steps } => format!("send a plan of {} steps", steps.len()),
    }
}

fn plan_refused(why: impl Into<String>) -> Refusal {
    Refusal::new(
        "ui plan",
        why,
        "list the steps that change the screen, e.g. homelab ui plan \"goto jobs\" \"open deploy media\" \"press confirm\"",
    )
}

/// The plan, checked: at least one step, at most [`PLAN_MAX`], and each one
/// a step that changes the screen.
pub fn new_plan(steps: &[UiStep], by: &str, st: &DriveState) -> Result<Plan, Refusal> {
    if steps.is_empty() {
        return Err(plan_refused("the plan lists no step"));
    }
    if steps.len() > PLAN_MAX {
        return Err(plan_refused(format!(
            "the plan lists {} steps; at most {PLAN_MAX}",
            steps.len()
        )));
    }
    if let Some((i, s)) = steps
        .iter()
        .enumerate()
        .find(|(_, s)| !holds(s) && !matches!(s, UiStep::Done))
    {
        return Err(plan_refused(format!(
            "step {} ({}) does not change the screen",
            i + 1,
            s.verb()
        )));
    }
    Ok(Plan {
        by: by.to_string(),
        steps: steps
            .iter()
            .map(|s| PlanStep {
                step: s.clone(),
                text: describe(s, st),
                done: false,
            })
            .collect(),
        next: 0,
        changed: false,
    })
}

impl Plan {
    /// `step` was taken: the plan's next step is done; a later one of the
    /// plan is done and the ones skipped mark the plan changed; any other
    /// step marks it changed and moves nothing.
    pub fn advance(&mut self, step: &UiStep) {
        let from = self.next.min(self.steps.len());
        match self.steps[from..].iter().position(|p| &p.step == step) {
            Some(i) => {
                if i > 0 {
                    self.changed = true;
                }
                self.steps[from + i].done = true;
                self.next = from + i + 1;
            }
            None => self.changed = true,
        }
    }
}

/// A viewer's button in the announcement bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Control {
    Pause,
    Continue,
    Stop,
}

impl Control {
    pub fn word(self) -> &'static str {
        match self {
            Control::Pause => "pause",
            Control::Continue => "continue",
            Control::Stop => "stop",
        }
    }
}

/// Why a stopped drive refuses a step, for the driver.
pub fn stopped_refusal(step: &UiStep, who: &str) -> Refusal {
    Refusal::new(
        format!("ui {}", step.verb()),
        format!("stopped by {who}: the sequence ended and the tabs are theirs again"),
        "ask Kenny before driving again; `homelab ui done` acknowledges the stop, after which steps are taken again",
    )
}

/// Why a step paused past the longest pause failed.
pub fn pause_expired(step: &UiStep, who: &str, max_s: u64) -> Refusal {
    Refusal::new(
        format!("ui {}", step.verb()),
        format!(
            "paused by {who} for more than {} min: the step was not taken",
            max_s / 60
        ),
        "ask the viewer what they want; send the step again once they are ready",
    )
}

impl DriveState {
    /// A viewer pressed Pause, Continue or Stop. Pause holds the next step
    /// (the one announced now, or the next to come) until Continue; Stop
    /// ends the drive at once: the dialog closes, the plan is gone, and
    /// every step is refused until the driver says `done`.
    pub fn control(&mut self, c: Control, who: &str, now: i64) -> Result<(), Refusal> {
        let driving = self.snapshot(now).active || self.announce.is_some();
        let refuse = |why: &str, fix: &str| Err(Refusal::new(c.word(), why, fix));
        match c {
            Control::Pause => {
                if !driving {
                    return refuse("Claude is not driving", "nothing to pause");
                }
                if self.paused_by.is_none() {
                    self.paused_by = Some(who.to_string());
                }
                Ok(())
            }
            Control::Continue => {
                if self.paused_by.take().is_none() {
                    return refuse("nothing is paused", "Pause holds Claude's next step");
                }
                Ok(())
            }
            Control::Stop => {
                if !driving {
                    return refuse("Claude is not driving", "nothing to stop");
                }
                self.stopped_by = Some(who.to_string());
                self.paused_by = None;
                self.announce = None;
                self.plan = None;
                self.form = None;
                self.active = false;
                self.seq += 1;
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goto(p: &str) -> UiStep {
        UiStep::Goto { path: p.into() }
    }

    #[test]
    fn follow_live_view_typing_is_held_but_not_announced() {
        assert!(counts_down(&goto("/app/jobs")));
        assert!(counts_down(&UiStep::Press {
            button: "next".into()
        }));
        let typing = UiStep::Type {
            field: "act-confirm".into(),
            text: "media".into(),
        };
        assert!(holds(&typing) && !counts_down(&typing));
        for s in [UiStep::State, UiStep::Done, UiStep::Plan { steps: vec![] }] {
            assert!(!holds(&s) && !counts_down(&s), "{s:?}");
        }
    }

    #[test]
    fn follow_live_view_steps_read_as_words() {
        let st = DriveState::default();
        assert_eq!(describe(&goto("/app/"), &st), "go to the fleet overview");
        assert_eq!(describe(&goto("/app/jobs"), &st), "go to the jobs page");
        assert_eq!(
            describe(&goto("/app/stacks/media/logs"), &st),
            "go to the logs tab of media"
        );
        assert_eq!(
            describe(
                &UiStep::Open {
                    form: "firewall".into(),
                    target: Some("admin".into())
                },
                &st
            ),
            "open Firewall · admin"
        );
        let open = describe(
            &UiStep::Open {
                form: "deploy".into(),
                target: Some("media".into()),
            },
            &st,
        );
        assert!(
            open.starts_with("open ") && open.ends_with("· media"),
            "{open}"
        );
        assert_eq!(
            describe(
                &UiStep::Row {
                    op: "edit".into(),
                    target: Some("backup_hour".into())
                },
                &st
            ),
            "edit the host setting backup_hour"
        );
        assert_eq!(describe(&UiStep::Close, &st), "close the dialog");
    }

    #[test]
    fn follow_live_view_the_plan_advances_and_a_deviation_marks_it_changed() {
        let st = DriveState::default();
        let a = goto("/app/jobs");
        let b = goto("/app/host");
        let c = UiStep::Close;
        let mut p = new_plan(&[a.clone(), b.clone(), c.clone()], "wsl", &st).unwrap();
        p.advance(&a);
        assert_eq!((p.next, p.changed), (1, false));
        assert!(p.steps[0].done && !p.steps[1].done);
        // Not on the plan: taken, the plan is changed, nothing moves.
        p.advance(&goto("/app/log"));
        assert_eq!((p.next, p.changed), (1, true));
        // A later step of the plan: it is done, the skipped one is not.
        let mut q = new_plan(&[a.clone(), b.clone(), c.clone()], "wsl", &st).unwrap();
        q.advance(&b);
        assert_eq!((q.next, q.changed), (2, true));
        assert!(!q.steps[0].done && q.steps[1].done);
        q.advance(&c);
        assert_eq!(q.next, 3);
        // Past its end every step is a deviation.
        q.advance(&a);
        assert!(q.changed);
        // What is not a plan.
        assert!(new_plan(&[], "wsl", &st).is_err());
        assert!(new_plan(&[UiStep::State], "wsl", &st).is_err());
        assert!(new_plan(&vec![a; PLAN_MAX + 1], "wsl", &st).is_err());
    }

    #[test]
    fn follow_live_view_pause_continue_and_stop_on_the_state() {
        let mut st = DriveState {
            active: true,
            last_at: 100,
            ..DriveState::default()
        };
        assert!(st.control(Control::Continue, "kenny", 100).is_err());
        st.control(Control::Pause, "kenny", 100).unwrap();
        st.control(Control::Pause, "other", 100).unwrap();
        assert_eq!(st.paused_by.as_deref(), Some("kenny"));
        st.control(Control::Continue, "other", 100).unwrap();
        assert_eq!(st.paused_by, None);
        let seq = st.seq;
        st.control(Control::Stop, "kenny", 100).unwrap();
        assert_eq!(st.stopped_by.as_deref(), Some("kenny"));
        assert!(!st.active && st.form.is_none() && st.plan.is_none());
        assert_eq!(st.seq, seq + 1);
        // Nothing to pause or stop once nobody drives.
        assert!(st.control(Control::Pause, "kenny", 100).is_err());
        assert!(st.control(Control::Stop, "kenny", 100).is_err());
    }
}
