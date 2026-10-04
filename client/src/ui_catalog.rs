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

use homelab_core::drivecatalog::{
    Catalog, Click, DialogControl, FieldName, click_line, dialog_click, dialog_field, reach_line,
};
use homelab_proto::UiStep;
use serde_json::Value;

/// What to send for one line, after the check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The steps to send, in order (an old name that became a menu item is
    /// a click and a press).
    pub steps: Vec<UiStep>,
    /// What the driver is told first: a new name, an address's new home.
    pub notes: Vec<String>,
}

/// What is on Kenny's screen, as the `state` answer says: a server-modelled
/// form open, or a page-level dialog and what it offers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Screen {
    pub form_open: bool,
    pub dialog: Option<Dialog>,
}

/// The open page-level dialog (`state.page_dialog`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dialog {
    pub title: String,
    pub controls: Option<Vec<DialogControl>>,
    pub fields: Option<Vec<String>>,
}

impl Screen {
    /// The screen from a `state` answer's `state` object (review H3: the
    /// client reads the open dialog from the state it already fetched).
    pub fn of(state: &Value) -> Screen {
        let d = &state["page_dialog"];
        Screen {
            form_open: !state["form"].is_null(),
            dialog: d.is_object().then(|| Dialog {
                title: d["title"].as_str().unwrap_or("the dialog").to_string(),
                controls: serde_json::from_value(d["controls"].clone()).ok(),
                fields: serde_json::from_value(d["fields"].clone()).ok(),
            }),
        }
    }
}

/// Check `step` against `catalog` and what `screen` shows. `Err`: refused
/// here, nothing is sent.
pub fn check(step: UiStep, catalog: &Catalog, screen: &Screen) -> Result<Checked, String> {
    let mut notes = Vec::new();
    let steps = check_one(step, catalog, screen, &mut notes)?;
    Ok(Checked { steps, notes })
}

fn refusal(verb: &str, why: &str, fix: &str) -> String {
    format!("refused here, nothing was sent to the dashboard: ui {verb} — {why}\n  fix: {fix}")
}

fn check_one(
    step: UiStep,
    catalog: &Catalog,
    screen: &Screen,
    notes: &mut Vec<String>,
) -> Result<Vec<UiStep>, String> {
    // A server-modelled form: the dashboard checks every step against the
    // form's own description.
    if screen.form_open && !matches!(step, UiStep::Plan { .. }) {
        return Ok(vec![step]);
    }
    // review H3: inside a page-level dialog, what that dialog offers
    // decides (the tab finds its buttons by name or by label).
    if let Some(d) = &screen.dialog {
        let verb = step.verb();
        let bad = |(why, fix): (String, String)| refusal(verb, &why, &fix);
        match &step {
            UiStep::Click { control: n, .. } | UiStep::Press { button: n } if n != "close" => {
                if let Some(offer) = &d.controls {
                    dialog_click(offer, &d.title, n, catalog).map_err(bad)?;
                }
                return Ok(vec![step]);
            }
            UiStep::Type { field, .. }
            | UiStep::Edit { field, .. }
            | UiStep::Pick { field, .. }
            | UiStep::Check { field, .. } => {
                if let Some(fields) = &d.fields {
                    dialog_field(
                        fields,
                        d.controls.as_deref().unwrap_or(&[]),
                        &d.title,
                        field,
                        catalog,
                    )
                    .map_err(bad)?;
                }
                return Ok(vec![step]);
            }
            _ => {}
        }
    }
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
            Click::Unknown { why, fix } => Err(refusal("click", &why, &fix)),
        },
        UiStep::Goto { path } => {
            let bare = path.split(['?', '#']).next().unwrap_or("");
            // review L1: a stack's own page, against the router's shape
            // (whether the stack exists is the dashboard's to say: the
            // fleet is not in the catalog).
            if bare.starts_with("/stacks/") {
                return match catalog.stack_path(bare) {
                    Ok(note) => {
                        notes.extend(note);
                        Ok(vec![UiStep::Goto { path }])
                    }
                    Err(why) => Err(refusal(
                        "goto",
                        &why,
                        "`homelab ui list` ends with every address the router knows",
                    )),
                };
            }
            match catalog.land(&path, &[]) {
                Ok(l) => {
                    notes.extend(l.note);
                    Ok(vec![UiStep::Goto { path }])
                }
                Err(why) => Err(refusal(
                    "goto",
                    &why,
                    "`homelab ui list` ends with every address the router knows",
                )),
            }
        }
        UiStep::Plan { steps } => {
            // Later steps run on a screen this check cannot see: after a
            // click that opens a dialog (or a form), its own buttons and
            // fields are the dialog's to check, so they go unchecked here.
            let mut out = Vec::new();
            let mut inside = screen.clone();
            for (i, s) in steps.into_iter().enumerate() {
                let opens = match &s {
                    UiStep::Click { control, row } => match catalog.click(control) {
                        // redesign-final-42: a row may say it opens no dialog
                        // (the stack hub's Update goes to the Update flow).
                        Click::Known(c) => c.opens_for(row.as_deref()) == "dialog",
                        Click::Renamed { control, press, .. } => {
                            control.opens_for(row.as_deref()) == "dialog" || press.is_some()
                        }
                        Click::Unknown { .. } => false,
                    },
                    UiStep::Open { .. } => true,
                    _ => false,
                };
                let closes = matches!(
                    s,
                    UiStep::Close | UiStep::Done | UiStep::Goto { .. } | UiStep::Reload
                );
                let checked = if inside.form_open || inside.dialog.is_some() {
                    match &s {
                        UiStep::Goto { .. } | UiStep::Plan { .. } => {
                            check_one(s, catalog, &Screen::default(), notes)
                        }
                        _ => Ok(vec![s]),
                    }
                } else {
                    check_one(s, catalog, &Screen::default(), notes)
                }
                .map_err(|e| format!("plan step {}: {e}", i + 1))?;
                out.extend(checked);
                if closes {
                    inside = Screen::default();
                } else if opens {
                    inside = Screen {
                        form_open: true,
                        dialog: None,
                    };
                }
            }
            Ok(vec![UiStep::Plan { steps: out }])
        }
        // review M5: a page field is declared like a control; an old id is
        // rewritten and the driver told.
        // redesign-final (coordinator, 2026-10-04): an old field that became
        // a control per row (a select turned into one press per choice)
        // presses that row.
        UiStep::Type { field, text } => match field_as_click(catalog, &field, &text, notes) {
            Some(click) => Ok(vec![click]),
            None => Ok(vec![UiStep::Type {
                field: field_now(catalog, "type", field, notes)?,
                text,
            }]),
        },
        UiStep::Edit { field, text } => Ok(vec![UiStep::Edit {
            field: field_now(catalog, "edit", field, notes)?,
            text,
        }]),
        UiStep::Pick { field, value } => match field_as_click(catalog, &field, &value, notes) {
            Some(click) => Ok(vec![click]),
            None => Ok(vec![UiStep::Pick {
                field: field_now(catalog, "pick", field, notes)?,
                value,
            }]),
        },
        UiStep::Check { field, on } => Ok(vec![UiStep::Check {
            field: field_now(catalog, "check", field, notes)?,
            on,
        }]),
        other => Ok(vec![other]),
    }
}

/// An old page field id that a control repeating per row keeps in its
/// `was` (and no field has): the click on that control's row `value`, the
/// driver told. `None` for every other name.
fn field_as_click(
    catalog: &Catalog,
    field: &str,
    value: &str,
    notes: &mut Vec<String>,
) -> Option<UiStep> {
    if !matches!(catalog.field(field), FieldName::Unknown { .. }) {
        return None;
    }
    match catalog.click(field) {
        Click::Renamed {
            control,
            press: None,
            ..
        } if control.row.is_some() => {
            notes.push(format!(
                "field {field} is the control {} now: {}",
                control.id,
                click_line(control, Some(value))
            ));
            Some(UiStep::Click {
                control: control.id.clone(),
                row: Some(value.to_string()),
            })
        }
        _ => None,
    }
}

/// The id a page field has now: a declared one as it is, an old one
/// rewritten (the driver told), an unknown one refused with the closest.
fn field_now(
    catalog: &Catalog,
    verb: &str,
    field: String,
    notes: &mut Vec<String>,
) -> Result<String, String> {
    match catalog.field(&field) {
        FieldName::Known(_) => Ok(field),
        FieldName::Renamed { field: f, now } => {
            notes.push(format!("field {field} is called {now} now (on {})", f.page));
            Ok(now)
        }
        FieldName::Unknown { why, fix } => Err(refusal(verb, &why, &fix)),
    }
}

/// `homelab ui list [page|words]`: the controls the filter picks, each with
/// where it lives, what it does and the line that clicks it; with no
/// filter, the router's addresses and old addresses after them.
pub fn render_list(catalog: &Catalog, filter: &[String]) -> String {
    // review M5: `ui list fields` — every page field `ui type/pick/check/edit`
    // may name, with its page and what it holds.
    if let [one] = filter
        && one == "fields"
    {
        let mut out = String::new();
        for f in &catalog.fields {
            let id = match &f.row {
                Some(r) => format!("{}-{r}", f.id),
                None => f.id.clone(),
            };
            out.push_str(&format!("{id}  · {} · {}\n", f.page, f.what));
            if !f.was.is_empty() {
                out.push_str(&format!("    was: {}\n", f.was.join(", ")));
            }
        }
        out.push_str(&format!(
            "\n{} page fields; a dialog's own fields and buttons are in `homelab ui state --json` (page_dialog) while it is open\n",
            catalog.fields.len()
        ));
        return out;
    }
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
        let name = [s("control"), s("field"), s("path")]
            .into_iter()
            .find(|n| !n.is_empty())
            .unwrap_or_default();
        let mut what = s("verb");
        for part in [name, s("row")] {
            if !part.is_empty() {
                what.push(' ');
                what.push_str(&part);
            }
        }
        if let Some(n) = x["text_len"].as_u64() {
            what.push_str(&format!(" ({n} characters typed)"));
        }
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
                     "reach": [{"do": "click", "control": "toggle-schedule", "row": "*"}]},
                    {"id": "stack-head", "page": "stack", "what": "Back up, Update or Deploy",
                     "opens": "dialog", "rowOpens": {"update": "view"},
                     "row": "<stack>/<backup|update|deploy>", "href": "/stacks/<stack>", "was": []},
                    {"id": "snooze-for", "page": "notifications", "what": "snooze every push",
                     "opens": "run", "row": "60|240", "href": "/system/notifications",
                     "was": [{"id": "notify-snooze-minutes"}]}
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

    fn cat() -> Catalog {
        let v: Value = serde_json::from_str(&reply()).unwrap();
        Catalog::parse(&v["catalog"].to_string()).unwrap()
    }

    fn click(c: &str, row: Option<&str>) -> UiStep {
        UiStep::Click {
            control: c.into(),
            row: row.map(str::to_string),
        }
    }

    #[test]
    fn drive_reach_an_unknown_control_is_refused_locally_with_the_closest() {
        let cat = cat();
        let e = check(click("new-schedul", None), &cat, &Screen::default()).unwrap_err();
        assert!(e.contains("nothing was sent"), "{e}");
        assert!(e.contains("new-schedule (on schedules"), "{e}");
        assert!(e.contains("homelab ui click new-schedule"), "{e}");
        let ok = check(click("new-schedule", None), &cat, &Screen::default()).unwrap();
        assert_eq!(ok.steps, [click("new-schedule", None)]);
        assert!(ok.notes.is_empty());
    }

    // redesign-final (coordinator, 2026-10-04): a removed select that became
    // one press per choice still answers `ui pick` / `ui type` by its old id.
    #[test]
    fn redesign_final_an_old_field_that_became_a_control_per_row_is_pressed() {
        let cat = cat();
        for step in [
            UiStep::Pick {
                field: "notify-snooze-minutes".into(),
                value: "240".into(),
            },
            UiStep::Type {
                field: "notify-snooze-minutes".into(),
                text: "240".into(),
            },
        ] {
            let c = check(step, &cat, &Screen::default()).unwrap();
            assert_eq!(c.steps, [click("snooze-for", Some("240"))]);
            assert!(
                c.notes[0].contains("notify-snooze-minutes is the control snooze-for now"),
                "{:?}",
                c.notes
            );
        }
        // A real field keeps its own verb.
        let c = check(
            UiStep::Pick {
                field: "shell-target".into(),
                value: "104".into(),
            },
            &cat,
            &Screen::default(),
        )
        .unwrap();
        assert!(matches!(&c.steps[0], UiStep::Pick { .. }));
    }

    #[test]
    fn drive_reach_an_old_field_name_is_rewritten_to_the_field_it_became() {
        let cat = cat();
        let c = check(
            UiStep::Pick {
                field: "shell-vmid".into(),
                value: "104".into(),
            },
            &cat,
            &Screen::default(),
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
        // review M5: a page field no page declares is refused like a click.
        let e = check(
            UiStep::Type {
                field: "notify-digest".into(),
                text: "07:30".into(),
            },
            &cat,
            &Screen::default(),
        )
        .unwrap_err();
        assert!(e.contains("no page declares a field notify-digest"), "{e}");
    }

    #[test]
    fn drive_reach_an_old_control_name_becomes_the_click_and_the_press() {
        let cat = cat();
        let c = check(click("edit-schedule", Some("s1")), &cat, &Screen::default()).unwrap();
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
            &Screen::default(),
        )
        .unwrap();
        assert!(matches!(&p.steps[0], UiStep::Plan { steps } if steps.len() == 2));
        assert!(
            check(
                UiStep::Plan {
                    steps: vec![click("zzz", None)]
                },
                &cat,
                &Screen::default()
            )
            .unwrap_err()
            .starts_with("plan step 1:")
        );
    }

    #[test]
    fn redesign_final_42_a_plan_after_the_hub_update_row_checks_the_page_not_a_dialog() {
        let cat = cat();
        let plan = |row: &str| UiStep::Plan {
            steps: vec![click("stack-head", Some(row)), click("zzz", None)],
        };
        // Back up opens a dialog: the next step is the dialog's to check.
        assert!(check(plan("kp-soft/backup"), &cat, &Screen::default()).is_ok());
        // Update goes to the Update flow page: the next step is checked.
        let e = check(plan("kp-soft/update"), &cat, &Screen::default()).unwrap_err();
        assert!(e.starts_with("plan step 2:"), "{e}");
    }

    #[test]
    fn drive_reach_goto_is_checked_and_an_old_address_says_its_new_home() {
        let cat = cat();
        let g = |p: &str| UiStep::Goto { path: p.into() };
        let c = check(g("/status"), &cat, &Screen::default()).unwrap();
        assert_eq!(c.steps, [g("/status")]);
        assert!(c.notes[0].contains("it is /inbox now"), "{:?}", c.notes);
        assert!(check(g("/stacks/anything/logs"), &cat, &Screen::default()).is_ok());
        let e = check(g("/inbx"), &cat, &Screen::default()).unwrap_err();
        assert!(e.contains("the closest: /inbox"), "{e}");
    }

    #[test]
    fn drive_reach_list_names_where_each_control_lives_and_its_line() {
        let cat = cat();
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
        let fields = render_list(&cat, &["fields".into()]);
        assert!(fields.starts_with("shell-target  · shell"), "{fields}");
        assert!(fields.contains("was: shell-vmid"), "{fields}");
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
