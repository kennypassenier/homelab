//! drive-reach (Kenny, 2026-10-03): every Live view step the dashboard
//! refused, kept so it can be counted and read back.
//!
//! Before this, a refusal went to the driver and to the tabs' toast and
//! nowhere else: the dashboard's journal named only the verb ("a driven step
//! was refused", without the control or the page), a refusal a TAB answered
//! (no such control on screen) was even logged as "applied", and the host's
//! relay logs no step outcome at all. So nobody could say how often a click
//! was refused, or which control. The log is bounded ([`KEEP`] entries,
//! oldest dropped first: Kenny's rule 20, nothing may balloon) and lives in
//! memory; each refusal also goes to the journal with its control and why.

use std::collections::VecDeque;

use homelab_proto::UiStep;
use serde::Serialize;

use super::actions::Refusal;

/// How many refusals are kept, newest last.
pub const KEEP: usize = 200;

/// One refused step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refused {
    /// Unix seconds.
    pub at: i64,
    /// The token that sent it.
    pub by: String,
    /// The step as `homelab ui` sends it.
    pub step: serde_json::Value,
    /// The verb (`click`, `goto`, …).
    pub verb: String,
    /// The control a click named, and its row.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<String>,
    /// The page the drive was on.
    pub page: String,
    pub why: String,
    pub fix: String,
}

/// The bounded log.
#[derive(Debug, Default)]
pub struct RefusalLog {
    items: VecDeque<Refused>,
    /// Every refusal since the dashboard started, kept or dropped.
    total: u64,
}

impl RefusalLog {
    pub fn record(&mut self, at: i64, by: &str, step: &UiStep, page: &str, r: &Refusal) {
        let (control, row) = match step {
            UiStep::Click { control, row } => (Some(control.clone()), row.clone()),
            _ => (None, None),
        };
        self.items.push_back(Refused {
            at,
            by: by.to_string(),
            step: serde_json::to_value(step).unwrap_or_default(),
            verb: step.verb().to_string(),
            control,
            row,
            page: page.to_string(),
            why: r.why.clone(),
            fix: r.fix.clone(),
        });
        self.total += 1;
        while self.items.len() > KEEP {
            self.items.pop_front();
        }
    }

    /// The kept refusals, oldest first, and the count since start.
    pub fn view(&self) -> serde_json::Value {
        serde_json::json!({
            "total": self.total,
            "kept": self.items.len(),
            "keep": KEEP,
            "refusals": self.items,
        })
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drive_reach_a_refused_click_is_kept_with_its_control_and_the_log_stays_bounded() {
        let mut log = RefusalLog::default();
        let click = UiStep::Click {
            control: "edit-schedul".into(),
            row: Some("s1".into()),
        };
        let r = Refusal::new(
            "ui click",
            "no page declares a control edit-schedul",
            "the closest: …",
        );
        log.record(10, "claude", &click, "/activity", &r);
        let v = log.view();
        assert_eq!(v["total"], 1);
        assert_eq!(v["refusals"][0]["control"], "edit-schedul");
        assert_eq!(v["refusals"][0]["row"], "s1");
        assert_eq!(v["refusals"][0]["verb"], "click");
        assert_eq!(v["refusals"][0]["page"], "/activity");
        let goto = UiStep::Goto {
            path: "/nope".into(),
        };
        for i in 0..(KEEP as i64 + 50) {
            log.record(i, "claude", &goto, "/", &r);
        }
        assert_eq!(log.len(), KEEP);
        assert_eq!(log.view()["total"], KEEP as u64 + 51);
        assert!(log.view()["refusals"][0].get("control").is_none());
    }
}
