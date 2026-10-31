//! Helpers shared by the check verbs.

use super::*;

/// fix-103 (check-output-buries-problem, 2026-09-27): the fleet check with
/// the summary first and each severity group in its own colour.
pub(crate) fn print_check(msg: &str, ok: bool) {
    use homelab_client::output::{Tone, check_tones};
    for (i, (tone, line)) in check_tones(msg).into_iter().enumerate() {
        let color = match tone {
            Tone::Broken => C_RED,
            Tone::Drift => C_YELLOW,
            Tone::Noted => C_DIM,
            Tone::Plain => C_GREEN,
        };
        let mark = match (i, ok) {
            (0, true) => "✓ ",
            (0, false) => "✗ ",
            _ => "",
        };
        println!("{}{}{}{}", color, mark, line, C_RESET);
    }
}

/// fix-219 (drift-finding-names-only-filenames, 2026-10-02): for every stack
/// a rendered fleet check/`today` text names as drifted against the host
/// (`homelab_core::ops::fleetcheck::repo_drift_stacks_in_rendered_text`),
/// print the actual per-file change: this working copy's own pinned files
/// against the host's intent repository copy (`GetApplied`, secret-free by
/// construction), the same pairing `deploy_diff` already uses for the
/// dashboard's Apply page. Only ever runs for stacks already reported
/// drifted — a clean fleet costs nothing beyond what it already cost.
pub(crate) async fn print_repo_drift_diffs(host: &str, token: &str, base: &str, rendered: &str) {
    let stacks = homelab_core::ops::fleetcheck::repo_drift_stacks_in_rendered_text(rendered);
    for stack in stacks {
        let dir = Path::new(base).join(&stack);
        let local = match homelab_client::spec::stack_files(&dir) {
            Ok(f) => f,
            Err(e) => {
                println!(
                    "{}  {}: could not read its local files for the diff — {}{}",
                    C_DIM, stack, e, C_RESET
                );
                continue;
            }
        };
        let Some(reply) = rpc_reply(
            host,
            token,
            Command::GetApplied {
                stack: stack.clone(),
            },
        )
        .await
        else {
            println!(
                "{}  {}: the host did not answer GetApplied for the diff{}",
                C_DIM, stack, C_RESET
            );
            continue;
        };
        if !reply.ok {
            println!("{}  {}: {}{}", C_DIM, stack, reply.message, C_RESET);
            continue;
        }
        let applied: Vec<homelab_proto::FileBlob> = match serde_json::from_str(&reply.message) {
            Ok(v) => v,
            Err(_) => {
                println!(
                    "{}  {}: the host's applied files did not read — update the host and the \
                     client to the same release{}",
                    C_DIM, stack, C_RESET
                );
                continue;
            }
        };
        let diffs = homelab_client::apply::file_diffs(&local, &applied);
        if diffs.is_empty() {
            continue;
        }
        println!("{}  {} — what changed:{}", C_YELLOW, stack, C_RESET);
        for d in &diffs {
            println!("    {} {}", d.sign, d.path);
            match &d.diff {
                Some(text) => {
                    for line in text.lines() {
                        println!("      {}", line);
                    }
                }
                None => println!("      (secret — content not shown)"),
            }
        }
    }
}

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "check" => cmd_check(c).await,
        "today" => cmd_today(c).await,
        "checks" => cmd_checks(c).await,
        "status" => cmd_status(c).await,
        "testplan" => cmd_testplan(c).await,
        "runbook" => cmd_runbook(c).await,
        _ => return false,
    }
    true
}

pub(crate) async fn cmd_check(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let base = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| stacks_base().display().to_string());
    let stack_files = spec::stack_files_with_vmids(&base);
    let digests = stack_digests(&stack_files);
    // Kenny ran this from inside `stacks/` on 2026-09-02 and it said
    // "0 stack file(s)" in passing, then reported the host's half as
    // if it were the whole answer. Half a check that looks like a
    // whole one is this project's most-repeated fault; say so.
    if stack_files.is_empty() {
        println!(
            "{}▶ fleet check :: no stack files under '{}' — checking only what the \
             HOST can see{}",
            C_YELLOW, base, C_RESET
        );
        println!(
            "  the half that compares your stack files against the fleet is SKIPPED. \
             Run this inside the repository, set HOMELAB_REPO in \
             ~/.config/homelab/env, or pass the path: homelab check \
             ~/Projects/homelab/stacks"
        );
    } else {
        println!(
            "{}▶ fleet check :: {} stack file(s) from {}{}",
            C_CYAN,
            stack_files.len(),
            base,
            C_RESET
        );
    }
    let (fleet_ok, _fleet, fleet_resp) = rpc_exchange(
        host,
        token,
        Command::FleetCheck {
            json: false,
            stack_files,
            digests,
            host_config: declared_host_config(),
        },
        true,
    )
    .await;
    // fix-219 (drift-finding-names-only-filenames): for every stack
    // the fleet check just named as drifted against the host, show
    // the actual old/new file content — fetched only for those
    // stacks, so a clean fleet (the normal case) pays nothing extra.
    if let Some(resp) = &fleet_resp {
        print_repo_drift_diffs(host, token, &base, &resp.message).await;
    }
    // fix-143 (expert panel 2026-09-27, edge-changes-unnoticed): the
    // Cloudflare edge against captured/gateway/, from here because
    // the read-only token and the capture both live on this side.
    let captured = Path::new(&base).join("../captured/gateway");
    let edge_ok = match homelab_client::edge::check_edge(&captured) {
        homelab_client::edge::EdgeOutcome::NotCompared(why) => {
            println!("{}edge: not compared — {}{}", C_DIM, why, C_RESET);
            true
        }
        homelab_client::edge::EdgeOutcome::Compared(findings) if findings.is_empty() => {
            println!("edge: Cloudflare agrees with captured/gateway/");
            true
        }
        homelab_client::edge::EdgeOutcome::Compared(findings) => {
            println!("edge: {} finding(s)", findings.len());
            for f in &findings {
                println!(
                    "  [{}] {} — {}\n      remedy: {}",
                    match f.severity {
                        homelab_core::ops::fleetcheck::Severity::Broken => "broken",
                        homelab_core::ops::fleetcheck::Severity::Drift => "drift",
                        homelab_core::ops::fleetcheck::Severity::Noted => "noted",
                    },
                    f.subject,
                    f.what,
                    f.remedy
                );
            }
            homelab_core::ops::fleetcheck::check_passes(&findings)
        }
    };
    // gap-37 (2026-09-28, Kenny: "eerst checken of de image bestaat"):
    // every pinned digest in the stack files, asked of its registry.
    let keep = std::fs::read_to_string(in_repo("config/host.toml"))
        .ok()
        .and_then(|t| homelab_client::pinexists::declared_keep(&t));
    let pin_findings = homelab_client::pinexists::check_pins(
        Path::new(&base),
        keep.as_ref().map(|(h, p)| (h.as_str(), *p)),
    );
    let broken_pins: Vec<_> = pin_findings
        .iter()
        .filter(|f| f.severity == homelab_core::ops::fleetcheck::Severity::Broken)
        .collect();
    // pinned-digest-pruned-1: deleted upstream, a kept copy is used.
    for f in pin_findings
        .iter()
        .filter(|f| f.severity == homelab_core::ops::fleetcheck::Severity::Drift)
    {
        println!(
            "  [attention] {} — {}\n      remedy: {}",
            f.subject, f.what, f.remedy
        );
    }
    if broken_pins.is_empty() {
        println!(
            "pins: every pinned image digest still exists ({} not asked)",
            pin_findings.len()
        );
    } else {
        println!("pins: {} missing upstream", broken_pins.len());
        for f in &broken_pins {
            println!(
                "  [broken] {} — {}\n      remedy: {}",
                f.subject, f.what, f.remedy
            );
        }
    }
    let pins_ok = homelab_core::ops::fleetcheck::check_passes(&pin_findings);
    std::process::exit(if fleet_ok && edge_ok && pins_ok { 0 } else { 1 });
}

/// fix-68 (four-answers-to-is-anything-wrong, 2026-09-27): the morning
/// question in one verb. `check`, `doctor`, `incidents` and `checks`
/// each gave part of the answer, and the operator merged them in his
/// head; this asks the host for all of it and prints one verdict.
pub(crate) async fn cmd_today(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let base = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| stacks_base().display().to_string());
    let stack_files = spec::stack_files_with_vmids(&base);
    let digests = stack_digests(&stack_files);
    if stack_files.is_empty() {
        println!(
            "{}▶ today :: no stack files under '{}' — the check reads only what the \
             HOST can see; run this inside the repository, set HOMELAB_REPO in \
             ~/.config/homelab/env, or pass the path{}",
            C_YELLOW, base, C_RESET
        );
    } else {
        println!(
            "{}▶ today :: doctor, fleet check ({} stack file(s)), incidents, manual \
             checks — about a minute{}",
            C_CYAN,
            stack_files.len(),
            C_RESET
        );
    }
    let reply = rpc_reply(
        host,
        token,
        Command::Today {
            stack_files,
            digests,
            host_config: declared_host_config(),
        },
    )
    .await
    .unwrap_or_else(|| die("the host did not answer"));
    let today: homelab_core::ops::today::Today = serde_json::from_str(&reply.message)
        .unwrap_or_else(|_| die(&format!("the host answered: {}", reply.message)));
    let color = if today.needs_you() { C_YELLOW } else { C_GREEN };
    let text = homelab_core::ops::today::render(&today);
    let (body, verdict) = text.rsplit_once('\n').unwrap_or(("", text.as_str()));
    if !body.is_empty() {
        println!("{}", body);
    }
    println!("{}{}{}", color, verdict, C_RESET);
    // fix-219: same drift diffs as `homelab check`, read from the
    // text just printed — `today` folds the fleet check's findings
    // in with doctor and the incidents, so the stack names are
    // found here instead of asking the host separately again.
    print_repo_drift_diffs(host, token, &base, &text).await;
    std::process::exit(if today.needs_you() { 1 } else { 0 });
}

/// G17: the questions only a person can answer. `homelab checks` lists
/// them with their ids; `homelab checks answer <id> ok|nok [note]`
/// records one. They used to be printed at the end of a deploy and
/// stored nowhere, which is not asking anybody anything.
pub(crate) async fn cmd_checks(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    match args.get(2).map(|s| s.as_str()) {
        None | Some("list") => rpc(host, token, Command::ListManualChecks { json: false }).await,
        Some("answer") => {
            // fix-65 (see REGISTER.md): a comma-separated list answers
            // several ids at once with the same verdict, note and (for
            // `accept`) days.
            let usage = "usage: homelab checks answer <id>[,<id>,...] ok|nok [note]";
            let ids = homelab_client::checks::split_ids(args.get(3).unwrap_or_else(|| die(usage)));
            if ids.is_empty() {
                die(usage);
            }
            let verdict = args.get(4).unwrap_or_else(|| die(usage));
            // fix-65: `accept <days> <reason>` records a deliberate nok
            // that is noted, not broken, until that many days from now.
            let (ok, note, accept_days) = if verdict == "accept" {
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
                (false, reason, Some(days))
            } else {
                let yes = ["ok", "yes", "ja"];
                let no = ["nok", "no", "nee"];
                let ok = if yes.contains(&verdict.as_str()) {
                    true
                } else if no.contains(&verdict.as_str()) {
                    false
                } else {
                    die(&format!(
                        "answer must be ok, nok or accept, not {}",
                        verdict
                    ))
                };
                (ok, args[5..].join(" "), None)
            };
            let mut all_ok = true;
            for id in &ids {
                if ids.len() > 1 {
                    println!("{}▶ {}{}", C_CYAN, id, C_RESET);
                }
                let one_ok = rpc_with(
                    host,
                    token,
                    Command::AnswerManualCheck {
                        check_id: id.clone(),
                        ok,
                        note: note.clone(),
                        accept_days,
                    },
                )
                .await;
                all_ok &= one_ok;
            }
            std::process::exit(if all_ok { 0 } else { 1 });
        }
        Some(other) => die(&format!(
            "unknown: homelab checks {} — try `list` or `answer`",
            other
        )),
    }
}

/// fix-68: a human table by default; `--json` still gets the
/// `FleetState` as-is, for a script (previously raw `pct list` +
/// the whole of state.json — 1,473 lines, measured live).
pub(crate) async fn cmd_status(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let json = args.iter().any(|a| a == "--json");
    let reply = rpc_reply(host, token, Command::Status)
        .await
        .unwrap_or_else(|| die("the host did not answer"));
    if !reply.ok {
        die(&reply.message);
    }
    if json {
        println!("{}", reply.message);
    } else {
        match serde_json::from_str::<homelab_proto::FleetState>(&reply.message) {
            Ok(fleet) => print!("{}", homelab_client::status::render_table(&fleet)),
            Err(_) => println!("{}", reply.message),
        }
    }
}

/// Phase 7's output document, derived from the tests rather than kept
/// beside them — the same reasoning as `runbook`.
pub(crate) async fn cmd_testplan(c: &Ctx) {
    let args = &c.args;
    let out = args.get(2).cloned().unwrap_or_else(|| {
        in_repo("docs/deployment/TEST_PLAN.md")
            .display()
            .to_string()
    });
    match homelab_client::testplan::generate_test_plan(
        &[&in_repo("core/tests"), &in_repo("client/tests")],
        &in_repo("docs/deployment/REALIZATION_PLAN.md"),
        &stacks_base(),
        Path::new(&out),
    ) {
        Ok(n) => println!(
            "{}✓ test plan written{} — {} ({} suite(s))",
            C_GREEN, C_RESET, out, n
        ),
        Err(e) => die(&e),
    }
}

pub(crate) async fn cmd_runbook(c: &Ctx) {
    let args = &c.args;
    // E7: generate the disaster-recovery runbook from the local stacks
    // directory — a document that works when everything else is down.
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| in_repo("docs/DR_RUNBOOK.md").display().to_string());
    match spec::generate_runbook(&stacks_base(), &out) {
        Ok(n) => println!(
            "{}✓ runbook written{} — {} ({} stack(s))",
            C_GREEN, C_RESET, out, n
        ),
        Err(e) => die(&format!("runbook: {}", e)),
    }
}
