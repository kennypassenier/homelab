//! feat-platform-10 (milestone follow): `homelab ui <step>`, Claude driving
//! Kenny's open dashboard one step at a time without a browser.
//!
//! Each step goes to the host on this machine's own scoped token; the host
//! hands it to the dashboard, which validates it against the same form
//! description its dialogs are drawn from, applies it to its one shared
//! "Claude is driving" state, and every tab that follows plays it. The
//! answer is what is on screen afterwards, rendered here as text so the
//! driver needs no browser to know where it is.

use homelab_proto::UiStep;
use serde_json::Value;

/// The steps, for the usage line and the verb's help.
pub const STEPS: &str = "goto <path> | open <form> [stack] | open batch <action> [<stack>,<stack>] | \
select <stack>,<stack>|none | click <control> [row] | type <field> <text> | \
pick <field> <value> | check <field> on|off | edit <field> <file|-> | \
row add|edit|up|down|delete [n|key] | press next|back|save|cancel|default | \
press confirm [--wait] | finish | answer [operation] allow|stop | close | reload | state | done | plan \"<step>\" \"<step>\" … | plan --file <file|->";

/// What the line may print for a command besides its own answer. The host
/// broadcasts every log line, transfer, fleet snapshot and question to every
/// client on the line; a `homelab ui` step printed "fleet: 15 stack(s)
/// managed" and "CHECK asking each container…" between its answers (Kenny,
/// 2026-09-29), so a UI step prints only its answer and the dashboard's
/// notes to it (paused, stopped), plus a first-use certificate pin.
pub fn quiet_line(command: &homelab_proto::Command) -> bool {
    // fix-240: an answer sent from a second terminal prints only whether
    // it was taken, not the running operation's stream.
    matches!(
        command,
        homelab_proto::Command::Ui { .. } | homelab_proto::Command::AnswerOpen { .. }
    )
}

/// What one `homelab ui …` line asks for: one step, or a step and then
/// waiting for the open dialog's job to end and letting go (Kenny,
/// 2026-09-29: control back as soon as the job ends, not 30 s later).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiCall {
    Step(UiStep),
    /// `ui finish`: wait for the open dialog's job to end, print its
    /// outcome, then `done` (which closes the dialog and releases the tabs).
    Finish,
    /// `ui press confirm --wait`: the press, then as `finish`.
    PressWait(UiStep),
}

/// `args` are the words after `ui`, `--json` already taken out; `--wait`
/// is read here.
pub fn parse_call(args: &[String]) -> Result<UiCall, String> {
    let wait = args.iter().any(|a| a == "--wait");
    let words: Vec<String> = args.iter().filter(|a| *a != "--wait").cloned().collect();
    if words.first().map(String::as_str) == Some("finish") {
        if words.len() > 1 || wait {
            return Err(format!("too many words for ui finish; {}", usage()));
        }
        return Ok(UiCall::Finish);
    }
    let step = parse(&words)?;
    if !wait {
        return Ok(UiCall::Step(step));
    }
    match &step {
        UiStep::Press { button } if button == "confirm" => Ok(UiCall::PressWait(step)),
        _ => Err(format!(
            "--wait goes with press confirm only (the press that starts a job); {}",
            usage()
        )),
    }
}

/// What `finish` does next, read from a `state` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinishNext {
    /// The job still runs: the line to show (printed when it changes).
    Wait(String),
    /// Let go now. `outcome`: the job's end, when a job ran; `failed` when
    /// that end was not a success (exit 1 after the release).
    Release {
        outcome: Option<String>,
        failed: bool,
    },
    /// A dialog is open and ran no job: nothing to wait for, and closing it
    /// would throw its input away.
    NoJob(String),
    /// A viewer pressed Stop (Kenny, 2026-09-30). `finish` must not send
    /// `done`, which would clear the Stop and let the caller's next step
    /// through; it exits 1 so a script stops too.
    Stopped(String),
}

/// The next move of `finish` from the dashboard's answer to `state`.
pub fn finish_next(message: &str) -> Result<FinishNext, String> {
    let v: Value =
        serde_json::from_str(message).map_err(|_| format!("the host answered: {message}"))?;
    let stopped = &v["state"]["stopped_by"];
    if !stopped.is_null() {
        return Ok(FinishNext::Stopped(format!(
            "stopped by {}: Live view is no longer driven and every next step is refused; \
             a job already running on the host runs to its end (`homelab ui state`, the jobs page)",
            text(stopped)
        )));
    }
    let f = &v["state"]["form"];
    if f.is_null() {
        return Ok(FinishNext::Release {
            outcome: None,
            failed: false,
        });
    }
    // A batch's final press queued several jobs: follow the batch
    // (Kenny, 2026-09-30: `--wait` gave up on a 10-stack batch).
    let b = &f["edit"]["result"]["progress"];
    if !b.is_null() {
        let jobs = b["jobs"].as_array().cloned().unwrap_or_default();
        let running = jobs
            .iter()
            .find(|j| j["state"] == "running")
            .map(|j| format!(" · now {}", text(&j["stack"])))
            .unwrap_or_default();
        let finished = jobs
            .iter()
            .filter(|j| !matches!(j["state"].as_str(), Some("queued" | "running")))
            .count();
        let failed = b["failed"].as_u64().unwrap_or(0);
        let line = format!(
            "batch  {} {}/{} finished · {} ok · {} failed{}",
            text(&b["batch"]),
            finished,
            jobs.len(),
            b["ok"].as_u64().unwrap_or(0),
            failed,
            running
        );
        return Ok(if b["done"].as_bool().unwrap_or(false) {
            FinishNext::Release {
                outcome: Some(line),
                failed: failed > 0,
            }
        } else {
            FinishNext::Wait(line)
        });
    }
    let j = &f["job"];
    if j.is_null() {
        return Ok(FinishNext::NoJob(format!(
            "the open dialog {} ran no job, so there is nothing to wait for; \
             press confirm first, or `homelab ui close` and `homelab ui done` to let go without running it",
            text(&f["title"])
        )));
    }
    let state = text(&j["state"]);
    let line = format!(
        "job    {} {}{}",
        text(&j["job"]),
        state,
        [&j["progress"], &j["message"]]
            .iter()
            .filter(|x| !x.is_null())
            .map(|x| format!(" · {}", text(x)))
            .collect::<String>()
    );
    Ok(match state.as_str() {
        "queued" | "running" => FinishNext::Wait(line),
        other => FinishNext::Release {
            outcome: Some(line),
            failed: !matches!(other, "done" | "deferred"),
        },
    })
}

fn usage() -> String {
    format!("usage: homelab ui {STEPS} [--json]")
}

/// `args` are the words after `ui`, `--json` already taken out. `edit`
/// reads its text from a file, or from stdin for `-`.
pub fn parse(args: &[String]) -> Result<UiStep, String> {
    parse_with(args, &|path: &str| {
        if path == "-" {
            let mut s = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut s)
                .map_err(|e| format!("stdin: {e}"))?;
            Ok(s)
        } else {
            std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
        }
    })
}

/// `parse`, with the reader `edit` takes its text from.
pub fn parse_with(
    args: &[String],
    read: &dyn Fn(&str) -> Result<String, String>,
) -> Result<UiStep, String> {
    let word = |i: usize| args.get(i).cloned().ok_or_else(usage);
    let verb = args.first().map(String::as_str).unwrap_or("state");
    if verb == "plan" {
        return parse_plan(&args[1..], read);
    }
    let step = match verb {
        "goto" => {
            let p = word(1)?;
            // nav-decisions (chassis-rs 3.1.0): every dashboard page lives
            // at the root now; `app/jobs` or `jobs` both mean `/jobs`.
            let path = if p.starts_with('/') {
                p
            } else {
                format!("/{}", p.trim_start_matches("app/"))
            };
            UiStep::Goto { path }
        }
        // `open batch <action> <stack>,<stack>`: the batch form of that
        // action on those stacks. The stacks may be left out (owner
        // decision 2026-09-30): the dashboard then opens it from the
        // Overview table's own ticked selection, set first with
        // `homelab ui select`.
        "open" if args.get(1).map(String::as_str) == Some("batch") => UiStep::Open {
            form: format!("batch:{}", word(2)?),
            target: args.get(3).cloned(),
        },
        "select" => {
            let list = word(1)?;
            let stacks = if list == "none" {
                Vec::new()
            } else {
                list.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            };
            UiStep::Select { stacks }
        }
        "open" => UiStep::Open {
            form: word(1)?,
            target: args.get(2).cloned(),
        },
        "edit" => {
            let field = word(1)?;
            let text = read(&word(2)?)?;
            UiStep::Edit { field, text }
        }
        "row" => UiStep::Row {
            op: word(1)?,
            target: args.get(2).cloned(),
        },
        "type" => {
            let field = word(1)?;
            if args.len() < 3 {
                return Err(usage());
            }
            UiStep::Type {
                field,
                text: args[2..].join(" "),
            }
        }
        "pick" => UiStep::Pick {
            field: word(1)?,
            value: word(2)?,
        },
        "check" => {
            let field = word(1)?;
            let on = match word(2)?.as_str() {
                "on" | "yes" | "true" => true,
                "off" | "no" | "false" => false,
                other => return Err(format!("check takes on or off, not {other}; {}", usage())),
            };
            UiStep::Check { field, on }
        }
        // fix-239: a page's own control (a row's Update, New schedule,
        // Issue token…), clicked by the tab that follows; inside the
        // page-level dialog it opens, type/pick/check/press reach its
        // fields and buttons the same way.
        "click" => UiStep::Click {
            control: word(1)?,
            row: args.get(2).cloned(),
        },
        "press" => UiStep::Press { button: word(1)? },
        "close" => UiStep::Close,
        // fix-185: tell the driven tab to take the dashboard's current
        // page, what its own "update available" banner's button does.
        "reload" => UiStep::Reload,
        "state" => UiStep::State,
        "done" => UiStep::Done,
        other => return Err(format!("unknown ui step '{other}'; {}", usage())),
    };
    let batch = args.get(1).map(String::as_str) == Some("batch");
    let extra = match &step {
        UiStep::Goto { .. } | UiStep::Press { .. } => args.len() > 2,
        UiStep::Open { .. } if batch => args.len() > 4,
        UiStep::Open { .. }
        | UiStep::Pick { .. }
        | UiStep::Check { .. }
        | UiStep::Edit { .. }
        | UiStep::Row { .. }
        | UiStep::Click { .. } => args.len() > 3,
        UiStep::Select { .. } => args.len() > 2,
        UiStep::Close | UiStep::Reload | UiStep::State | UiStep::Done => args.len() > 1,
        UiStep::Type { .. } | UiStep::Plan { .. } => false,
    };
    if extra {
        return Err(format!("too many words for ui {verb}; {}", usage()));
    }
    Ok(step)
}

/// Live view: `plan "<step>" "<step>" …`, each argument one step as
/// `homelab ui` spells it, or `plan --file <file>` (`-` for stdin) with one
/// step per line; blank lines and lines starting with `#` are skipped. A
/// step's words are split on white space, so a typed text keeps single
/// spaces only. `edit` reads its file now, when the plan is made.
fn parse_plan(
    args: &[String],
    read: &dyn Fn(&str) -> Result<String, String>,
) -> Result<UiStep, String> {
    let lines: Vec<String> = match args.first().map(String::as_str) {
        Some("--file" | "-f") => {
            if args.len() != 2 {
                return Err(format!("plan --file takes one file; {}", usage()));
            }
            read(&args[1])?
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .map(str::to_string)
                .collect()
        }
        _ => args.to_vec(),
    };
    if lines.is_empty() {
        return Err(format!(
            "a plan needs at least one step, e.g. homelab ui plan \"goto jobs\" \"open deploy mystack\"; {}",
            usage()
        ));
    }
    let mut steps = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let words: Vec<String> = line.split_whitespace().map(str::to_string).collect();
        match words.first().map(String::as_str) {
            Some("plan") | Some("state") | None => {
                return Err(format!(
                    "plan step {} ({line:?}) is not a step that changes the screen: \
                     a plan lists goto, open, type, pick, check, edit, row, press, close and done",
                    i + 1
                ));
            }
            _ => {}
        }
        let step = parse_with(&words, read).map_err(|e| format!("plan step {}: {e}", i + 1))?;
        steps.push(step);
    }
    Ok(UiStep::Plan { steps })
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// The dashboard's answer as text: the refusal first when there is one,
/// then what is on screen. `Err` when the step was refused (exit 1).
pub fn render(message: &str) -> Result<String, String> {
    let Ok(v) = serde_json::from_str::<Value>(message) else {
        return Err(format!("the host answered: {message}"));
    };
    let mut out = String::new();
    let refused = v.get("refusal").filter(|r| !r.is_null());
    if let Some(r) = refused {
        out.push_str(&format!(
            "refused: {} — {}\n  fix: {}\n",
            text(&r["what"]),
            text(&r["why"]),
            text(&r["fix"])
        ));
    }
    let s = &v["state"];
    if !s.is_null() {
        let driving = if s["active"] == Value::Bool(true) {
            format!("driving as {} · step {}", text(&s["by"]), text(&s["seq"]))
        } else {
            "not driving: every tab is its viewer's".into()
        };
        out.push_str(&format!("{driving}\npage   {}\n", text(&s["page"])));
        if let Some(sel) = s["selected"].as_array().filter(|a| !a.is_empty()) {
            let names: Vec<String> = sel.iter().map(text).collect();
            out.push_str(&format!("select {}\n", names.join(", ")));
        }
        if !s["stopped_by"].is_null() {
            out.push_str(&format!(
                "stopped by {}: every step is refused until `homelab ui done`\n",
                text(&s["stopped_by"])
            ));
        }
        if !s["paused_by"].is_null() {
            out.push_str(&format!(
                "paused by {}: the next step waits for Continue\n",
                text(&s["paused_by"])
            ));
        }
        let p = &s["plan"];
        if !p.is_null() {
            let steps = p["steps"].as_array().map(Vec::len).unwrap_or(0);
            let done = p["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|x| x["done"] == Value::Bool(true))
                .count();
            out.push_str(&format!(
                "plan   {done} of {steps} steps done{}\n",
                if p["changed"] == Value::Bool(true) {
                    " · changed: a step was not the plan's next"
                } else {
                    ""
                }
            ));
            if let Some(next) = p["steps"]
                .as_array()
                .and_then(|a| a.get(p["next"].as_u64().unwrap_or(0) as usize))
            {
                out.push_str(&format!("  next {}\n", text(&next["text"])));
            }
        }
        // fix-239: a page-level dialog a `ui click` opened.
        let pd = &s["page_dialog"];
        if !pd.is_null() {
            out.push_str(&format!(
                "dialog {} (a page's own dialog: click, type, pick, check and press act in it; close closes it)\n",
                text(&pd["title"])
            ));
        }
        let f = &s["form"];
        if f.is_null() {
            if pd.is_null() {
                out.push_str("form   none open\n");
            }
        } else {
            let steps: Vec<String> = f["steps"]
                .as_array()
                .map(|a| a.iter().map(text).collect())
                .unwrap_or_default();
            out.push_str(&format!(
                "form   {} · step {} of {} ({})\n",
                text(&f["title"]),
                f["step_index"].as_u64().map(|i| i + 1).unwrap_or(0),
                steps.len(),
                text(&f["step"]),
            ));
            for x in f["fields"].as_array().into_iter().flatten() {
                let shown = x["shown"] != Value::Bool(false);
                let mut line = format!(
                    "  {:<22} {:<7} {}",
                    text(&x["id"]),
                    text(&x["kind"]),
                    if x["value"] == Value::String(String::new()) {
                        "(empty)".into()
                    } else {
                        text(&x["value"])
                    }
                );
                if !shown {
                    let why = x["hidden_why"]
                        .as_str()
                        .unwrap_or("shown only when the deploy guard refuses");
                    line.push_str(&format!("  [hidden: {why}]"));
                }
                if let Some(c) = x["choices"].as_array().filter(|c| !c.is_empty()) {
                    let c: Vec<String> = c.iter().map(text).collect();
                    line.push_str(&format!("  choices: {}", c.join(", ")));
                }
                if !x["error"].is_null() {
                    line.push_str(&format!("\n    ✗ {}", text(&x["error"])));
                }
                out.push_str(&line);
                out.push('\n');
            }
            if let Some(g) = f.get("guard").filter(|g| !g.is_null()) {
                out.push_str(&format!(
                    "  the deploy guard refuses: {} ({})\n",
                    text(&g["why"]),
                    text(&g["fix"])
                ));
            }
            let buttons: Vec<String> = f["buttons"]
                .as_array()
                .map(|a| a.iter().map(text).collect())
                .unwrap_or_default();
            if !buttons.is_empty() {
                out.push_str(&format!("buttons {}\n", buttons.join(", ")));
            }
            let e = &f["edit"];
            if !e.is_null() {
                render_edit(e, &mut out);
            }
            if let Some(r) = f.get("run_error").filter(|r| !r.is_null()) {
                out.push_str(&format!(
                    "  not done: {} ({})\n",
                    text(&r["why"]),
                    text(&r["fix"])
                ));
            }
            let j = &f["job"];
            if !j.is_null() {
                out.push_str(&format!(
                    "job    {} {}{}\n",
                    text(&j["job"]),
                    text(&j["state"]),
                    [&j["progress"], &j["message"]]
                        .iter()
                        .filter(|x| !x.is_null())
                        .map(|x| format!(" · {}", text(x)))
                        .collect::<String>()
                ));
            }
        }
    }
    if refused.is_some() { Err(out) } else { Ok(out) }
}

/// An edit form's own lines: its table, a dialog on top, the plan and
/// what the final press answered.
fn render_edit(e: &Value, out: &mut String) {
    for r in e["rows"].as_array().into_iter().flatten() {
        out.push_str(&format!("  row    {}\n", text(r)));
    }
    if let Some(s) = e.get("sub").filter(|s| !s.is_null()) {
        out.push_str(&format!(
            "dialog {} (its fields are the ones on step {})\n",
            text(&s["title"]),
            text(&s["kind"])
        ));
    }
    let p = &e["plan"];
    if !p.is_null() {
        let files: Vec<String> = p["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| format!("{} {}", text(&f["status"]), text(&f["path"])))
            .collect();
        out.push_str(&format!(
            "plan   {} · {}\n",
            if p["valid"] == Value::Bool(true) {
                "can be committed"
            } else {
                "refused"
            },
            if files.is_empty() {
                "no file changes".to_string()
            } else {
                files.join(", ")
            }
        ));
        for x in p["problems"].as_array().into_iter().flatten() {
            out.push_str(&format!("    ✗ {}\n", text(x)));
        }
    }
    let r = &e["result"];
    if !r.is_null() {
        let line = if !r["committed"].is_null() {
            let c = text(&r["committed"]["commit"]);
            format!(
                "committed {} and pushed: {}",
                c.chars().take(10).collect::<String>(),
                text(&r["committed"]["subject"])
            )
        } else if !r["batch"].is_null() {
            format!("batch {} queued", text(&r["batch"]))
        } else if !r["saved"].is_null() {
            "host.toml written".to_string()
        } else {
            r.to_string()
        };
        out.push_str(&format!("done   {line}\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    #[test]
    fn follow_every_step_parses_and_a_wrong_one_names_the_usage() {
        assert_eq!(
            parse(&words("goto stacks/media")).unwrap(),
            UiStep::Goto {
                path: "/stacks/media".into()
            }
        );
        assert_eq!(
            parse(&words("open deploy media")).unwrap(),
            UiStep::Open {
                form: "deploy".into(),
                target: Some("media".into())
            }
        );
        assert_eq!(
            parse(&words("type act-confirm new version")).unwrap(),
            UiStep::Type {
                field: "act-confirm".into(),
                text: "new version".into()
            }
        );
        assert_eq!(
            parse(&words("check act-force on")).unwrap(),
            UiStep::Check {
                field: "act-force".into(),
                on: true
            }
        );
        assert_eq!(parse(&words("press confirm")).unwrap().verb(), "press");
        assert_eq!(parse(&[]).unwrap(), UiStep::State);
        assert!(parse(&words("check act-force maybe")).is_err());
        assert!(
            parse(&words("fly away"))
                .unwrap_err()
                .contains("usage: homelab ui")
        );
        assert!(parse(&words("close now")).is_err());
        // fix-185: tell the driven tab to take the dashboard's current
        // page, what its own "update available" banner's button does.
        assert_eq!(parse(&words("reload")).unwrap(), UiStep::Reload);
        assert!(parse(&words("reload now")).is_err());
        assert!(STEPS.contains("reload"));
        // fix-239: a page's own control, with or without its row.
        assert_eq!(
            parse(&words("click pin-update media/jellyfin/jellyfin")).unwrap(),
            UiStep::Click {
                control: "pin-update".into(),
                row: Some("media/jellyfin/jellyfin".into()),
            }
        );
        assert_eq!(
            parse(&words("click new-schedule")).unwrap(),
            UiStep::Click {
                control: "new-schedule".into(),
                row: None,
            }
        );
        assert!(parse(&words("click")).is_err());
        assert!(parse(&words("click a b c")).is_err());
        assert!(STEPS.contains("click <control> [row]"));
        // The edit forms' steps.
        assert_eq!(
            parse(&words("open batch update media,drill")).unwrap(),
            UiStep::Open {
                form: "batch:update".into(),
                target: Some("media,drill".into())
            }
        );
        // Owner decision 2026-09-30: a batch opened with no stacks named
        // reads the Overview table's own ticked selection.
        assert_eq!(
            parse(&words("open batch deploy")).unwrap(),
            UiStep::Open {
                form: "batch:deploy".into(),
                target: None
            }
        );
        assert_eq!(
            parse(&words("select media,drill")).unwrap(),
            UiStep::Select {
                stacks: vec!["media".into(), "drill".into()]
            }
        );
        assert_eq!(
            parse(&words("select none")).unwrap(),
            UiStep::Select { stacks: vec![] }
        );
        assert!(parse(&words("select")).is_err());
        assert!(parse(&words("select media,drill extra")).is_err());
        assert_eq!(
            parse(&words("row up 2")).unwrap(),
            UiStep::Row {
                op: "up".into(),
                target: Some("2".into())
            }
        );
        let read = |p: &str| {
            if p == "f.yml" {
                Ok("a: 1\nb: 2\n".to_string())
            } else {
                Err(format!("{p}: missing"))
            }
        };
        assert_eq!(
            parse_with(&words("edit raw-text f.yml"), &read).unwrap(),
            UiStep::Edit {
                field: "raw-text".into(),
                text: "a: 1\nb: 2\n".into()
            }
        );
        assert!(
            parse_with(&words("edit raw-text nope"), &read)
                .unwrap_err()
                .contains("missing")
        );
    }

    /// Kenny, 2026-09-29: control back as soon as the confirmed dialog's
    /// job ends: `finish`, and `press confirm --wait` in one call.
    #[test]
    fn finish_and_press_confirm_wait_parse_and_wait_goes_with_confirm_only() {
        assert_eq!(parse_call(&words("finish")).unwrap(), UiCall::Finish);
        assert_eq!(
            parse_call(&words("press confirm --wait")).unwrap(),
            UiCall::PressWait(UiStep::Press {
                button: "confirm".into()
            })
        );
        assert_eq!(
            parse_call(&words("press confirm")).unwrap(),
            UiCall::Step(UiStep::Press {
                button: "confirm".into()
            })
        );
        assert!(
            parse_call(&words("press next --wait"))
                .unwrap_err()
                .contains("--wait goes with press confirm only")
        );
        assert!(parse_call(&words("finish now")).is_err());
        assert!(STEPS.contains("finish"));
        assert!(STEPS.contains("press confirm [--wait]"));
    }

    #[test]
    fn finish_waits_while_the_job_runs_then_releases_with_its_outcome() {
        let with_job = |state: &str, progress: Value, message: Value| {
            serde_json::json!({"ok": true, "state": {"active": true, "by": "wsl", "seq": 7,
                "page": "/stacks/uptime", "form": {"title": "Deploy · uptime",
                "job": {"job": 458, "state": state, "progress": progress, "message": message}}}})
            .to_string()
        };
        assert_eq!(
            finish_next(&with_job("running", "step 3/31: pull".into(), Value::Null)).unwrap(),
            FinishNext::Wait("job    458 running · step 3/31: pull".into())
        );
        assert!(matches!(
            finish_next(&with_job("queued", Value::Null, Value::Null)).unwrap(),
            FinishNext::Wait(_)
        ));
        assert_eq!(
            finish_next(&with_job(
                "done",
                "step 31/31: service checks".into(),
                "complete".into()
            ))
            .unwrap(),
            FinishNext::Release {
                outcome: Some("job    458 done · step 31/31: service checks · complete".into()),
                failed: false
            }
        );
        assert!(matches!(
            finish_next(&with_job("failed", Value::Null, "pull failed".into())).unwrap(),
            FinishNext::Release { failed: true, .. }
        ));
        // No dialog (already released): let go all the same.
        let none = serde_json::json!({"ok": true, "state": {"active": false, "form": null}});
        assert_eq!(
            finish_next(&none.to_string()).unwrap(),
            FinishNext::Release {
                outcome: None,
                failed: false
            }
        );
        // A dialog that ran nothing is not closed behind the driver's back.
        let open = serde_json::json!({"ok": true, "state": {"active": true,
            "form": {"title": "Deploy · uptime", "job": null}}});
        let FinishNext::NoJob(why) = finish_next(&open.to_string()).unwrap() else {
            panic!("not NoJob")
        };
        assert!(why.contains("Deploy · uptime ran no job"), "{why}");
        assert!(finish_next("not json").is_err());
        // A Stop wins over a running job: `finish` lets go without `done`.
        let stopped = serde_json::json!({"state": {"stopped_by": "kenny", "form": null}});
        let FinishNext::Stopped(why) = finish_next(&stopped.to_string()).unwrap() else {
            panic!("a Stop is reported as Stopped");
        };
        assert!(why.contains("stopped by kenny"), "{why}");
        // A batch is followed until every job in it finished.
        let batch = |done: bool, state: &str| {
            serde_json::json!({"state": {"stopped_by": null, "form": {"title": "Deploy · 2 stacks",
                "job": null, "edit": {"result": {"batch": 9, "progress": {"batch": 9, "done": done,
                "ok": 1, "failed": 0, "jobs": [
                    {"job": 10, "stack": "media", "state": "done"},
                    {"job": 11, "stack": "uptime", "state": state}]}}}}}})
            .to_string()
        };
        assert_eq!(
            finish_next(&batch(false, "running")).unwrap(),
            FinishNext::Wait("batch  9 1/2 finished · 1 ok · 0 failed · now uptime".into())
        );
        assert!(matches!(
            finish_next(&batch(true, "done")).unwrap(),
            FinishNext::Release { failed: false, .. }
        ));
    }

    #[test]
    fn follow_a_plan_is_parsed_from_words_or_a_file_and_refuses_what_is_no_step() {
        let args: Vec<String> = ["plan", "goto jobs", "open deploy media", "press confirm"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let UiStep::Plan { steps } = parse(&args).unwrap() else {
            panic!("not a plan")
        };
        assert_eq!(steps.len(), 3);
        assert_eq!(
            steps[0],
            UiStep::Goto {
                path: "/jobs".into()
            }
        );
        let file = "# deploy media\n\ngoto stacks/media\ntype act-confirm media now\nedit raw-text f.yml\n";
        let read = |p: &str| match p {
            "plan.txt" => Ok(file.to_string()),
            "f.yml" => Ok("a: 1\n".to_string()),
            _ => Err(format!("{p}: missing")),
        };
        let UiStep::Plan { steps } = parse_with(&words("plan --file plan.txt"), &read).unwrap()
        else {
            panic!("not a plan")
        };
        assert_eq!(steps.len(), 3);
        assert_eq!(
            steps[1],
            UiStep::Type {
                field: "act-confirm".into(),
                text: "media now".into()
            }
        );
        assert_eq!(
            steps[2],
            UiStep::Edit {
                field: "raw-text".into(),
                text: "a: 1\n".into()
            }
        );
        assert!(parse(&words("plan")).is_err());
        let nested: Vec<String> = ["plan", "state"].iter().map(|s| s.to_string()).collect();
        assert!(parse(&nested).unwrap_err().contains("plan step 1"));
        let wrong: Vec<String> = ["plan", "goto jobs", "fly away"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(parse(&wrong).unwrap_err().starts_with("plan step 2"));
    }

    #[test]
    fn follow_the_answer_names_a_pause_a_stop_and_the_plan() {
        let v = serde_json::json!({
            "ok": false,
            "refusal": {"what": "ui press", "why": "stopped by the viewer kenny", "fix": "ask Kenny"},
            "state": {"active": false, "by": "wsl", "seq": 9, "page": "/", "form": null,
              "paused_by": null, "stopped_by": "the viewer kenny",
              "plan": {"next": 1, "changed": true, "steps": [
                {"text": "go to jobs", "done": true}, {"text": "press Confirm", "done": false}]}}
        });
        let e = render(&v.to_string()).unwrap_err();
        assert!(e.contains("stopped by the viewer kenny"), "{e}");
        assert!(e.contains("plan   1 of 2 steps done · changed"), "{e}");
        assert!(e.contains("next press Confirm"), "{e}");
    }

    #[test]
    fn follow_a_ui_step_keeps_the_line_quiet_and_other_verbs_do_not() {
        use homelab_proto::Command;
        let step = Command::Ui {
            step: UiStep::Goto { path: "/".into() },
            client_version: "3.70.0".into(),
        };
        assert!(quiet_line(&step));
        assert!(!quiet_line(&Command::Ping));
        assert!(!quiet_line(&Command::PatchFleet));
    }

    #[test]
    fn follow_the_answer_renders_the_screen_and_a_refusal_is_an_error() {
        let ok = serde_json::json!({
            "ok": true,
            "state": {"active": true, "by": "wsl", "seq": 4, "page": "/stacks/media",
              "selected": ["media", "drill"],
              "form": {"title": "Deploy · media", "step": "review", "step_index": 0,
                "steps": ["review"], "buttons": ["confirm"],
                "fields": [{"id": "act-force", "kind": "check", "value": false, "shown": false, "error": null}],
                "guard": null, "job": {"job": 812, "state": "done", "message": "complete", "progress": null}}}
        });
        let t = render(&ok.to_string()).unwrap();
        assert!(t.contains("page   /stacks/media"), "{t}");
        assert!(t.contains("select media, drill"), "{t}");
        assert!(t.contains("act-force"), "{t}");
        assert!(t.contains("job    812 done · complete"), "{t}");
        let no = serde_json::json!({
            "ok": false,
            "refusal": {"what": "ui type", "why": "no field act-reason", "fix": "fields: act-force"},
            "state": {"active": true, "by": "wsl", "seq": 4, "page": "/", "form": null}
        });
        let e = render(&no.to_string()).unwrap_err();
        assert!(
            e.starts_with("refused: ui type — no field act-reason"),
            "{e}"
        );
        assert!(e.contains("form   none open"), "{e}");
    }

    /// fix-answer-days: a field hidden until another field has a value says
    /// when it shows; the guard's force keeps its own words.
    #[test]
    fn fix_answer_days_a_hidden_field_says_when_it_shows() {
        let ok = serde_json::json!({
            "ok": true,
            "state": {"active": true, "by": "wsl", "seq": 4, "page": "/host",
              "form": {"title": "Answer check · the whole host", "step": "options", "step_index": 0,
                "steps": ["options", "review"], "buttons": ["next", "close"],
                "fields": [
                  {"id": "act-days", "kind": "text", "value": "", "shown": false,
                   "hidden_why": "shown only when the answer is \"not ok, accepted for some days\"", "error": null},
                  {"id": "act-force", "kind": "check", "value": false, "shown": false, "error": null}
                ],
                "guard": null, "job": null}}
        });
        let t = render(&ok.to_string()).unwrap();
        assert!(
            t.contains(
                "[hidden: shown only when the answer is \"not ok, accepted for some days\"]"
            ),
            "{t}"
        );
        assert!(
            t.contains("[hidden: shown only when the deploy guard refuses]"),
            "{t}"
        );
    }
}
