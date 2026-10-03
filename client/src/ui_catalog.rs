//! drive-reach (Kenny, 2026-10-03: "Claude must always be able to reach every
//! control"; Claude made every button, so guessing a name must stop): the
//! client's side of the Live view control catalog.
//!
//! A `homelab ui` line asks the dashboard for its `state` first; the answer
//! carries the catalog of the dashboard version that is running
//! (`homelab_core::drivecatalog`, generated from the page modules' own
//! declarations). Here, before anything is sent to Kenny's tab:
//!
//! * `ui click <name>` is checked: an unknown name is refused locally with
//!   the closest real controls and never reaches the screen; an old name is
//!   rewritten to the control it became (and the menu press that picks it);
//! * `ui goto <path>` is checked against every address the router knows,
//!   and an old address says where it lands now;
//! * `ui list [page|words]` prints the catalog: each control, where it
//!   lives, what it does and the exact line that clicks it;
//! * `ui refusals` prints the dashboard's recent refused steps.
//!
//! A dashboard from before the catalog answers `state` without one: every
//! step is then sent as it is, as before (a version check informs, it never
//! blocks).

use homelab_core::drivecatalog::{Catalog, Click, click_line, reach_line};
use homelab_proto::UiStep;
use serde_json::Value;

/// The catalog a `state` answer carries; `None` from an older dashboard.
pub fn catalog_of(state_reply: &str) -> Option<Catalog> {
    let v: Value = serde_json::from_str(state_reply).ok()?;
    let c = v.get("catalog").filter(|c| !c.is_null())?;
    serde_json::from_value(c.clone()).ok()
}

/// What to send for one line, after the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The steps to send, in order (an old name that became a menu item is
    /// a click and a press).
    pub steps: Vec<UiStep>,
    /// What the driver is told first: a new name, an address's new home.
    pub notes: Vec<String>,
}

/// Check `step` against `catalog`. `Err`: refused here, nothing is sent.
pub fn check(step: UiStep, catalog: &Catalog) -> Result<Checked, String> {
    let mut notes = Vec::new();
    let steps = check_one(step, catalog, &mut notes)?;
    Ok(Checked { steps, notes })
}

fn check_one(
    step: UiStep,
    catalog: &Catalog,
    notes: &mut Vec<String>,
) -> Result<Vec<UiStep>, String> {
    match step {
        UiStep::Click { control, row } => match catalog.click(&control) {
            Click::Known(_) => Ok(vec![UiStep::Click { control, row }]),
            Click::Renamed {
                control: now,
                was,
                press,
            } => {
                notes.push(match &press {
                    Some(p) => format!(
                        "{was} is an item of {} now: clicking {} then pressing {p} ({})",
                        now.id,
                        now.id,
                        click_line(now, row.as_deref())
                    ),
                    None => format!(
                        "{was} is called {} now ({})",
                        now.id,
                        click_line(now, row.as_deref())
                    ),
                });
                let mut v = vec![UiStep::Click {
                    control: now.id.clone(),
                    row,
                }];
                if let Some(button) = press {
                    v.push(UiStep::Press { button });
                }
                Ok(v)
            }
            Click::Unknown { why, fix } => Err(format!(
                "refused here, nothing was sent to the dashboard: ui click — {why}\n  fix: {fix}"
            )),
        },
        UiStep::Goto { path } => {
            // A stack's own page: whether the stack exists is the
            // dashboard's to say (the fleet is not in the catalog).
            let bare = path.split(['?', '#']).next().unwrap_or("");
            if bare.starts_with("/stacks/") {
                return Ok(vec![UiStep::Goto { path }]);
            }
            match catalog.land(&path, &[]) {
                Ok(l) => {
                    if let Some(n) = l.note {
                        notes.push(n);
                    }
                    Ok(vec![UiStep::Goto { path }])
                }
                Err(why) => Err(format!(
                    "refused here, nothing was sent to the dashboard: ui goto — {why}\n  \
                     fix: `homelab ui list` ends with every address the router knows"
                )),
            }
        }
        UiStep::Plan { steps } => {
            let mut out = Vec::new();
            for (i, s) in steps.into_iter().enumerate() {
                let checked = check_one(s, catalog, notes)
                    .map_err(|e| format!("plan step {}: {e}", i + 1))?;
                out.extend(checked);
            }
            Ok(vec![UiStep::Plan { steps: out }])
        }
        // drive-reach: a page field a redesign renamed, by its old id.
        UiStep::Type { field, text } => Ok(vec![UiStep::Type {
            field: field_now(catalog, field, notes),
            text,
        }]),
        UiStep::Edit { field, text } => Ok(vec![UiStep::Edit {
            field: field_now(catalog, field, notes),
            text,
        }]),
        UiStep::Pick { field, value } => Ok(vec![UiStep::Pick {
            field: field_now(catalog, field, notes),
            value,
        }]),
        UiStep::Check { field, on } => Ok(vec![UiStep::Check {
            field: field_now(catalog, field, notes),
            on,
        }]),
        other => Ok(vec![other]),
    }
}

/// The id a field has now; an old one is rewritten and the driver told.
fn field_now(catalog: &Catalog, field: String, notes: &mut Vec<String>) -> String {
    match catalog.renamed_field(&field) {
        Some(f) => {
            notes.push(format!(
                "field {field} is called {} now (on {})",
                f.id, f.page
            ));
            f.id.clone()
        }
        None => field,
    }
}

/// `homelab ui list [page|words]`: the controls the filter picks, each with
/// where it lives, what it does and the line that clicks it; with no
/// filter, the router's addresses and old addresses after them.
pub fn render_list(catalog: &Catalog, filter: &[String]) -> String {
    let picked = catalog.list(filter);
    let mut out = String::new();
    if picked.is_empty() {
        let near = catalog.closest(&filter.join("-"), 5);
        out.push_str(&format!(
            "no control matches {}{}\n",
            filter.join(" "),
            if near.is_empty() {
                String::new()
            } else {
                format!(
                    "; the closest: {}",
                    near.iter()
                        .map(|c| c.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        ));
    }
    for c in &picked {
        let opens = match c.opens.as_str() {
            "dialog" => "opens a dialog",
            "view" => "changes the view",
            _ => "runs",
        };
        out.push_str(&format!(
            "{}  · {} · {}\n    {}\n    {}\n",
            c.id,
            c.href.as_deref().unwrap_or(&c.page),
            opens,
            c.what,
            click_line(c, None)
        ));
        if !c.was.is_empty() {
            let was: Vec<String> = c
                .was
                .iter()
                .map(|w| match &w.press {
                    Some(p) => format!("{} (now: press {p} in it)", w.id),
                    None => w.id.clone(),
                })
                .collect();
            out.push_str(&format!("    was: {}\n", was.join(", ")));
        }
        if let Some(s) = &c.shows {
            out.push_str(&format!("    on screen only {s}\n"));
        }
        if !c.reach.is_empty() {
            let lines: Vec<String> = c.reach.iter().map(reach_line).collect();
            out.push_str(&format!("    reach it first: {}\n", lines.join("; then ")));
        }
    }
    if filter.is_empty() {
        let current: Vec<String> = catalog
            .addresses
            .iter()
            .filter(|a| !catalog.redirects.contains_key(*a))
            .map(|a| format!("/{a}"))
            .collect();
        out.push_str(&format!(
            "\n{} controls\npages  {} · a stack: /stacks/<name>[/<tab>]\n",
            picked.len(),
            current.join(" ")
        ));
        let old: Vec<String> = catalog
            .redirects
            .iter()
            .map(|(from, to)| format!("/{from} → {to}"))
            .collect();
        out.push_str(&format!("old    {}\n", old.join(" · ")));
    }
    out
}

/// `homelab ui refusals`: the dashboard's recent refused steps, oldest
/// first. `now`: unix seconds, for "N min ago".
pub fn render_refusals(state_reply: &str, now: i64) -> Result<String, String> {
    let v: Value = serde_json::from_str(state_reply)
        .map_err(|_| format!("the host answered: {state_reply}"))?;
    let r = v
        .get("refusals")
        .filter(|r| !r.is_null())
        .ok_or("this dashboard keeps no refusals yet (it predates drive-reach); update it")?;
    let items = r["refusals"].as_array().cloned().unwrap_or_default();
    let mut out = format!(
        "{} step(s) refused since the dashboard started; the newest {} shown (it keeps {})\n",
        r["total"].as_u64().unwrap_or(0),
        items.len(),
        r["keep"].as_u64().unwrap_or(0)
    );
    for x in &items {
        let s = |k: &str| x[k].as_str().unwrap_or("").to_string();
        let what = match (s("control"), s("row")) {
            (c, r) if !c.is_empty() && !r.is_empty() => format!("{} {c} {r}", s("verb")),
            (c, _) if !c.is_empty() => format!("{} {c}", s("verb")),
            _ => s("verb"),
        };
        out.push_str(&format!(
            "{:>12}  {what} on {} by {}\n    why: {}\n    fix: {}\n",
            ago(now - x["at"].as_i64().unwrap_or(now)),
            s("page"),
            s("by"),
            s("why"),
            s("fix")
        ));
    }
    Ok(out)
}

/// A past moment as "N s/min/h ago" (Kenny: minutes and seconds, hours past
/// 60 min).
fn ago(secs: i64) -> String {
    let s = secs.max(0);
    if s < 60 {
        format!("{s} s ago")
    } else if s < 3600 {
        format!("{} min ago", s / 60)
    } else {
        format!("{} h {} min ago", s / 3600, (s % 3600) / 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply() -> String {
        serde_json::json!({
            "ok": true,
            "state": {},
            "catalog": {
                "controls": [
                    {"id": "schedule-menu", "page": "schedules", "what": "open one schedule's menu",
                     "opens": "dialog", "row": "<schedule id>", "href": "/activity?view=planned",
                     "was": [{"id": "edit-schedule", "press": "edit"}]},
                    {"id": "new-schedule", "page": "schedules", "what": "open the New schedule drawer",
                     "opens": "dialog", "href": "/activity?view=planned", "was": [],
                     "shows": null, "reach": []},
                    {"id": "undo-schedule-change", "page": "schedules", "what": "undo the last switch",
                     "opens": "run", "href": "/activity?view=planned", "was": [],
                     "shows": "while its Undo toast shows",
                     "reach": [{"do": "click", "control": "toggle-schedule", "row": "*"}]}
                ],
                "fields": [
                    {"id": "shell-target", "page": "shell", "what": "the container a command runs in",
                     "was": ["shell-vmid"]}
                ],
                "addresses": ["", "apps", "inbox", "status"],
                "redirects": {"status": "/inbox"},
                "stack_slot": "{stack}"
            },
            "refusals": {"total": 3, "kept": 1, "keep": 200, "refusals": [
                {"at": 1000, "by": "wsl", "step": {}, "verb": "click", "control": "nope",
                 "row": "s1", "page": "/activity", "why": "no page declares a control nope",
                 "fix": "the closest: …"}
            ]}
        })
        .to_string()
    }

    fn click(c: &str, row: Option<&str>) -> UiStep {
        UiStep::Click {
            control: c.into(),
            row: row.map(str::to_string),
        }
    }

    #[test]
    fn drive_reach_an_unknown_control_is_refused_locally_with_the_closest() {
        let cat = catalog_of(&reply()).unwrap();
        let e = check(click("new-schedul", None), &cat).unwrap_err();
        assert!(e.contains("nothing was sent"), "{e}");
        assert!(e.contains("new-schedule (on schedules"), "{e}");
        assert!(e.contains("homelab ui click new-schedule"), "{e}");
        let ok = check(click("new-schedule", None), &cat).unwrap();
        assert_eq!(ok.steps, [click("new-schedule", None)]);
        assert!(ok.notes.is_empty());
    }

    #[test]
    fn drive_reach_an_old_field_name_is_rewritten_to_the_field_it_became() {
        let cat = catalog_of(&reply()).unwrap();
        let c = check(
            UiStep::Pick {
                field: "shell-vmid".into(),
                value: "104".into(),
            },
            &cat,
        )
        .unwrap();
        assert_eq!(
            c.steps,
            [UiStep::Pick {
                field: "shell-target".into(),
                value: "104".into()
            }]
        );
        assert!(
            c.notes[0].contains("shell-vmid is called shell-target now"),
            "{:?}",
            c.notes
        );
        // A field no declaration renamed goes as it is (most page fields
        // are found by their element id and are not in the catalog).
        let same = check(
            UiStep::Type {
                field: "notify-digest".into(),
                text: "07:30".into(),
            },
            &cat,
        )
        .unwrap();
        assert!(same.notes.is_empty());
        assert!(matches!(&same.steps[0], UiStep::Type { field, .. } if field == "notify-digest"));
    }

    #[test]
    fn drive_reach_an_old_control_name_becomes_the_click_and_the_press() {
        let cat = catalog_of(&reply()).unwrap();
        let c = check(click("edit-schedule", Some("s1")), &cat).unwrap();
        assert_eq!(
            c.steps,
            [
                click("schedule-menu", Some("s1")),
                UiStep::Press {
                    button: "edit".into()
                }
            ]
        );
        assert!(c.notes[0].contains("edit-schedule is an item of schedule-menu now"));
        // Inside a plan too.
        let p = check(
            UiStep::Plan {
                steps: vec![click("edit-schedule", Some("s1"))],
            },
            &cat,
        )
        .unwrap();
        assert!(matches!(&p.steps[0], UiStep::Plan { steps } if steps.len() == 2));
        assert!(
            check(
                UiStep::Plan {
                    steps: vec![click("zzz", None)]
                },
                &cat
            )
            .unwrap_err()
            .starts_with("plan step 1:")
        );
    }

    #[test]
    fn drive_reach_goto_is_checked_and_an_old_address_says_its_new_home() {
        let cat = catalog_of(&reply()).unwrap();
        let g = |p: &str| UiStep::Goto { path: p.into() };
        let c = check(g("/status"), &cat).unwrap();
        assert_eq!(c.steps, [g("/status")]);
        assert!(c.notes[0].contains("it is /inbox now"), "{:?}", c.notes);
        assert!(check(g("/stacks/anything/logs"), &cat).is_ok());
        let e = check(g("/inbx"), &cat).unwrap_err();
        assert!(e.contains("the closest: /inbox"), "{e}");
    }

    #[test]
    fn drive_reach_an_older_dashboard_without_a_catalog_is_not_checked() {
        assert!(catalog_of(r#"{"ok":true,"state":{}}"#).is_none());
    }

    #[test]
    fn drive_reach_list_names_where_each_control_lives_and_its_line() {
        let cat = catalog_of(&reply()).unwrap();
        let all = render_list(&cat, &[]);
        assert!(
            all.contains("schedule-menu  · /activity?view=planned · opens a dialog"),
            "{all}"
        );
        assert!(
            all.contains("homelab ui click schedule-menu <schedule id>"),
            "{all}"
        );
        assert!(
            all.contains("was: edit-schedule (now: press edit in it)"),
            "{all}"
        );
        assert!(
            all.contains("on screen only while its Undo toast shows"),
            "{all}"
        );
        assert!(
            all.contains("reach it first: homelab ui click toggle-schedule <row>"),
            "{all}"
        );
        assert!(all.contains("/status → /inbox"), "{all}");
        let undo = render_list(&cat, &["undo".into()]);
        assert!(undo.starts_with("undo-schedule-change"), "{undo}");
        assert!(!undo.contains("new-schedule"), "{undo}");
        // A half-remembered old name still finds what it became.
        let half = render_list(&cat, &["edit-schedul".into()]);
        assert!(half.starts_with("schedule-menu"), "{half}");
        let none = render_list(&cat, &["menu-schedulez".into()]);
        assert!(none.starts_with("no control matches"), "{none}");
        assert!(none.contains("the closest: schedule-menu"), "{none}");
    }

    #[test]
    fn drive_reach_refusals_print_what_why_and_fix() {
        let out = render_refusals(&reply(), 1000 + 125).unwrap();
        assert!(out.starts_with("3 step(s) refused"), "{out}");
        assert!(
            out.contains("2 min ago  click nope s1 on /activity by wsl"),
            "{out}"
        );
        assert!(
            out.contains("why: no page declares a control nope"),
            "{out}"
        );
        assert!(render_refusals(r#"{"ok":true}"#, 0).is_err());
    }
}
