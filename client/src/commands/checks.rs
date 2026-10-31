//! `homelab check` and `homelab today`: the fleet check, the edge and pin
//! checks run from this side, and the morning verdict.

use super::*;
use homelab_core::ops::fleetcheck::{Severity, check_passes};

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "check" => cmd_check(c).await,
        "today" => cmd_today(c).await,
        _ => return false,
    }
    true
}

/// The stacks directory a check reads: the argument, else the repository's.
fn base_arg(args: &[String]) -> String {
    args.get(2)
        .cloned()
        .unwrap_or_else(|| stacks_base().display().to_string())
}

/// Kenny ran this from inside `stacks/` on 2026-09-02 and it said
/// "0 stack file(s)" in passing, then reported the host's half as
/// if it were the whole answer. Half a check that looks like a
/// whole one is this project's most-repeated fault; say so.
fn print_check_banner(base: &str, stack_files: &[(String, u16)]) {
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
}

fn severity_word(s: &Severity) -> &'static str {
    match s {
        Severity::Broken => "broken",
        Severity::Drift => "drift",
        Severity::Noted => "noted",
    }
}

/// fix-143 (expert panel 2026-09-27, edge-changes-unnoticed): the
/// Cloudflare edge against captured/gateway/, from here because
/// the read-only token and the capture both live on this side.
fn check_edge(base: &str) -> bool {
    let captured = Path::new(base).join("../captured/gateway");
    match homelab_client::edge::check_edge(&captured) {
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
                    severity_word(&f.severity),
                    f.subject,
                    f.what,
                    f.remedy
                );
            }
            check_passes(&findings)
        }
    }
}

/// gap-37 (2026-09-28, Kenny: "eerst checken of de image bestaat"):
/// every pinned digest in the stack files, asked of its registry.
fn check_pins(base: &str) -> bool {
    let keep = std::fs::read_to_string(in_repo("config/host.toml"))
        .ok()
        .and_then(|t| homelab_client::pinexists::declared_keep(&t));
    let pin_findings = homelab_client::pinexists::check_pins(
        Path::new(base),
        keep.as_ref().map(|(h, p)| (h.as_str(), *p)),
    );
    let broken_pins: Vec<_> = pin_findings
        .iter()
        .filter(|f| f.severity == Severity::Broken)
        .collect();
    // pinned-digest-pruned-1: deleted upstream, a kept copy is used.
    for f in pin_findings
        .iter()
        .filter(|f| f.severity == Severity::Drift)
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
    check_passes(&pin_findings)
}

pub(crate) async fn cmd_check(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let base = base_arg(&c.args);
    let stack_files = spec::stack_files_with_vmids(&base);
    let digests = stack_digests(&stack_files);
    print_check_banner(&base, &stack_files);
    let command = Command::FleetCheck {
        json: false,
        stack_files,
        digests,
        host_config: declared_host_config(),
    };
    let (fleet_ok, _fleet, fleet_resp) = rpc_exchange(host, token, command, true).await;
    // fix-219 (drift-finding-names-only-filenames): for every stack
    // the fleet check just named as drifted against the host, show
    // the actual old/new file content — fetched only for those
    // stacks, so a clean fleet (the normal case) pays nothing extra.
    if let Some(resp) = &fleet_resp {
        print_repo_drift_diffs(host, token, &base, &resp.message).await;
    }
    let edge_ok = check_edge(&base);
    let pins_ok = check_pins(&base);
    std::process::exit(if fleet_ok && edge_ok && pins_ok { 0 } else { 1 });
}

fn print_today_banner(base: &str, stack_files: &[(String, u16)]) {
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
}

/// The verdict is the last line, in the colour of whether it needs you.
fn print_today(today: &homelab_core::ops::today::Today, text: &str) {
    let color = if today.needs_you() { C_YELLOW } else { C_GREEN };
    let (body, verdict) = text.rsplit_once('\n').unwrap_or(("", text));
    if !body.is_empty() {
        println!("{}", body);
    }
    println!("{}{}{}", color, verdict, C_RESET);
}

/// fix-68 (four-answers-to-is-anything-wrong, 2026-09-27): the morning
/// question in one verb. `check`, `doctor`, `incidents` and `checks`
/// each gave part of the answer, and the operator merged them in his
/// head; this asks the host for all of it and prints one verdict.
pub(crate) async fn cmd_today(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let base = base_arg(&c.args);
    let stack_files = spec::stack_files_with_vmids(&base);
    let digests = stack_digests(&stack_files);
    print_today_banner(&base, &stack_files);
    let command = Command::Today {
        stack_files,
        digests,
        host_config: declared_host_config(),
    };
    let reply = rpc_reply(host, token, command)
        .await
        .unwrap_or_else(|| die("the host did not answer"));
    let today: homelab_core::ops::today::Today = serde_json::from_str(&reply.message)
        .unwrap_or_else(|_| die(&format!("the host answered: {}", reply.message)));
    let text = homelab_core::ops::today::render(&today);
    print_today(&today, &text);
    // fix-219: same drift diffs as `homelab check`, read from the
    // text just printed — `today` folds the fleet check's findings
    // in with doctor and the incidents, so the stack names are
    // found here instead of asking the host separately again.
    print_repo_drift_diffs(host, token, &base, &text).await;
    std::process::exit(if today.needs_you() { 1 } else { 0 });
}
