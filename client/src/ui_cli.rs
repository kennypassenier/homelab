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
pub const STEPS: &str = "goto <path> | open <action> [stack] | type <field> <text> | \
pick <field> <value> | check <field> on|off | press next|back|confirm | close | state | done";

fn usage() -> String {
    format!("usage: homelab ui {STEPS} [--json]")
}

/// `args` are the words after `ui`, `--json` already taken out.
pub fn parse(args: &[String]) -> Result<UiStep, String> {
    let word = |i: usize| args.get(i).cloned().ok_or_else(usage);
    let verb = args.first().map(String::as_str).unwrap_or("state");
    let step = match verb {
        "goto" => {
            let p = word(1)?;
            let path = if p.starts_with('/') {
                p
            } else {
                format!("/app/{}", p.trim_start_matches("app/"))
            };
            UiStep::Goto { path }
        }
        "open" => UiStep::Open {
            form: word(1)?,
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
        "press" => UiStep::Press { button: word(1)? },
        "close" => UiStep::Close,
        "state" => UiStep::State,
        "done" => UiStep::Done,
        other => return Err(format!("unknown ui step '{other}'; {}", usage())),
    };
    let extra = match &step {
        UiStep::Goto { .. } | UiStep::Press { .. } => args.len() > 2,
        UiStep::Open { .. } | UiStep::Pick { .. } | UiStep::Check { .. } => args.len() > 3,
        UiStep::Close | UiStep::State | UiStep::Done => args.len() > 1,
        UiStep::Type { .. } => false,
    };
    if extra {
        return Err(format!("too many words for ui {verb}; {}", usage()));
    }
    Ok(step)
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
        let f = &s["form"];
        if f.is_null() {
            out.push_str("form   none open\n");
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
                    line.push_str("  [hidden: shown only when the deploy guard refuses]");
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
    if refused.is_some() {
        Err(out)
    } else {
        Ok(out)
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
                path: "/app/stacks/media".into()
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
        assert!(parse(&words("fly away"))
            .unwrap_err()
            .contains("usage: homelab ui"));
        assert!(parse(&words("close now")).is_err());
    }

    #[test]
    fn follow_the_answer_renders_the_screen_and_a_refusal_is_an_error() {
        let ok = serde_json::json!({
            "ok": true,
            "state": {"active": true, "by": "wsl", "seq": 4, "page": "/app/stacks/media",
              "form": {"title": "Deploy · media", "step": "review", "step_index": 0,
                "steps": ["review"], "buttons": ["confirm"],
                "fields": [{"id": "act-force", "kind": "check", "value": false, "shown": false, "error": null}],
                "guard": null, "job": {"job": 812, "state": "done", "message": "complete", "progress": null}}}
        });
        let t = render(&ok.to_string()).unwrap();
        assert!(t.contains("page   /app/stacks/media"), "{t}");
        assert!(t.contains("act-force"), "{t}");
        assert!(t.contains("job    812 done · complete"), "{t}");
        let no = serde_json::json!({
            "ok": false,
            "refusal": {"what": "ui type", "why": "no field act-reason", "fix": "fields: act-force"},
            "state": {"active": true, "by": "wsl", "seq": 4, "page": "/app/", "form": null}
        });
        let e = render(&no.to_string()).unwrap_err();
        assert!(
            e.starts_with("refused: ui type — no field act-reason"),
            "{e}"
        );
        assert!(e.contains("form   none open"), "{e}");
    }
}
