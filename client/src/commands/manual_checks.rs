//! `homelab checks`: the questions only a person can answer.

use super::*;

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "checks" => cmd_checks(c).await,
        _ => return false,
    }
    true
}

/// The verdict of `checks answer`: pass/fail, the note, and the days an
/// accepted failure stays noted.
struct Verdict {
    ok: bool,
    note: String,
    accept_days: Option<u32>,
}

/// fix-65: `accept <days> <reason>` records a deliberate nok
/// that is noted, not broken, until that many days from now.
fn parse_accept(args: &[String]) -> Verdict {
    let usage = "usage: homelab checks answer <id>[,<id>,...] accept <days> \
                  <reason>";
    let days: u32 = args
        .get(5)
        .and_then(|d| d.parse().ok())
        .filter(|d| *d > 0)
        .unwrap_or_else(|| die(usage));
    let reason = args.get(6..).map(|r| r.join(" ")).unwrap_or_default();
    if reason.trim().is_empty() {
        die(usage);
    }
    Verdict {
        ok: false,
        note: reason,
        accept_days: Some(days),
    }
}

fn parse_verdict(args: &[String], verdict: &str) -> Verdict {
    if verdict == "accept" {
        return parse_accept(args);
    }
    let yes = ["ok", "yes", "ja"];
    let no = ["nok", "no", "nee"];
    let ok = if yes.contains(&verdict) {
        true
    } else if no.contains(&verdict) {
        false
    } else {
        die(&format!(
            "answer must be ok, nok or accept, not {}",
            verdict
        ))
    };
    Verdict {
        ok,
        note: args[5..].join(" "),
        accept_days: None,
    }
}

/// fix-65 (see REGISTER.md): a comma-separated list answers
/// several ids at once with the same verdict, note and (for
/// `accept`) days.
async fn checks_answer(host: &str, token: &str, args: &[String]) -> ! {
    let usage = "usage: homelab checks answer <id>[,<id>,...] ok|nok [note]";
    let ids = homelab_client::checks::split_ids(args.get(3).unwrap_or_else(|| die(usage)));
    if ids.is_empty() {
        die(usage);
    }
    let verdict = args.get(4).unwrap_or_else(|| die(usage));
    let v = parse_verdict(args, verdict);
    let mut all_ok = true;
    for id in &ids {
        if ids.len() > 1 {
            println!("{}▶ {}{}", C_CYAN, id, C_RESET);
        }
        let answer = Command::AnswerManualCheck {
            check_id: id.clone(),
            ok: v.ok,
            note: v.note.clone(),
            accept_days: v.accept_days,
        };
        all_ok &= rpc_with(host, token, answer).await;
    }
    std::process::exit(if all_ok { 0 } else { 1 });
}

/// G17: the questions only a person can answer. `homelab checks` lists
/// them with their ids; `homelab checks answer <id> ok|nok [note]`
/// records one. They used to be printed at the end of a deploy and
/// stored nowhere, which is not asking anybody anything.
pub(crate) async fn cmd_checks(c: &Ctx) {
    let (host, token, args) = (&c.host, &c.token, &c.args);
    match args.get(2).map(|s| s.as_str()) {
        None | Some("list") => rpc(host, token, Command::ListManualChecks { json: false }).await,
        Some("answer") => checks_answer(host, token, args).await,
        Some(other) => die(&format!(
            "unknown: homelab checks {} — try `list` or `answer`",
            other
        )),
    }
}
