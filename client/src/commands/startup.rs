//! Start-up of one invocation: colour, help and unknown-verb handling,
//! repository and host resolution, and the token check.

use super::*;

/// Colour only when stdout is a terminal and NO_COLOR is unset.
pub(crate) fn init_colour() {
    use std::io::IsTerminal as _;
    homelab_client::output::set_colour(homelab_client::output::should_colour(
        std::io::stdout().is_terminal(),
        std::env::var("NO_COLOR").ok().as_deref(),
    ));
}

/// F309 (see REGISTER.md): `--help` anywhere means SHOW the help, never
/// do the thing. Checked over every argument, not just the one that bit:
/// an unparseable input is a question, not a licence to proceed.
pub(crate) fn show_help_if_asked(args: &[String]) {
    if !homelab_client::version::wants_help(args) {
        return;
    }
    // fix-108 (help-flat-ids-typo-exit0, 2026-09-27): `homelab <verb>
    // --help` is that verb's help with an example, not the whole list.
    match args
        .get(1)
        .and_then(|v| homelab_client::cli_help::verb_help(v))
    {
        Some(help) => print!("{}", help),
        None => print!("{}", homelab_client::cli_help::usage()),
    }
    std::process::exit(0);
}

/// fix-108: a mistyped verb printed the help and exited 0, so a typo in a
/// script looked like success. It says what was probably meant and exits 2.
pub(crate) fn refuse_unknown_verb(cmd: &str) {
    if homelab_client::cli_help::is_verb(cmd) {
        return;
    }
    let hint = homelab_client::cli_help::suggest(cmd)
        .map(|v| format!("; did you mean '{}'?", v))
        .unwrap_or_default();
    eprintln!(
        "{}error:{} unknown command '{}'{} — `homelab help` lists them",
        C_RED, C_RESET, cmd, hint
    );
    std::process::exit(2);
}

/// Kenny, 2026-09-02: `homelab check` has to work from any directory
/// without sourcing anything first. Every command in every document here
/// was written as `homelab <verb>`, and every one of them needed
/// `set -a; . ./.env` in front of it and a repository to stand in — a
/// ritual nobody wrote down and nothing enforced.
///
/// Order: the environment wins (so a one-off override still works), then
/// the user's own config, then the repository's `.env` when standing in
/// it. Reading, never writing: this file is where the token lives, not a
/// cache of it.
/// feat-client-1 (Kenny, 2026-09-19): the address is a fact about the
/// fleet and lives in the repository, `config/client.toml`; a value typed
/// before the command still wins, the machine's env file comes after the
/// repository, and the compiled-in default is the last resort.
/// (The env file was loaded in `main`, before the runtime started.)
pub(crate) fn resolve_host(explicit_host: Option<String>) -> String {
    let root = homelab_client::repo_config::repo_root(
        &cwd(),
        std::env::var("HOMELAB_REPO").ok().as_deref(),
    );
    let repo_cfg = homelab_client::repo_config::load(root.as_deref().unwrap_or(&cwd()))
        .unwrap_or_else(|e| die(&e));
    let _ = REPO_ROOT.set(root);
    let (host, host_source) = homelab_client::repo_config::resolve_host(
        explicit_host,
        repo_cfg.as_ref().map(|(p, c)| (p.as_path(), c)),
        std::env::var("HOMELAB_HOST").ok(),
    );
    let _ = HOST_SOURCE.set(host_source);
    let _ = REPO_PIN.set(repo_cfg.as_ref().and_then(|(_, c)| c.pin.clone()));
    host
}

/// fix-66: `--answer allow|stop` pre-answers every host question this
/// command's operation raises, without waiting for a terminal.
pub(crate) fn read_pre_answer(args: &[String]) {
    let pre_answer = homelab_client::answer::parse_pre_answer_flag(
        args.iter()
            .position(|a| a == "--answer")
            .and_then(|i| args.get(i + 1))
            .map(String::as_str),
    )
    .unwrap_or_else(|e| die(&e));
    let _ = PRE_ANSWER.set(pre_answer);
}

/// Commands that never touch the network need no token: help, offline TUI,
/// and `plan` (local validation only, D10).
fn needs_token(cmd: &str, offline: bool) -> bool {
    !matches!(
        cmd,
        "help"
            | "plan"
            | "runbook"
            | "update-policy"
            | "presets"
            | "export"
            | "import"
            | "self-install"
            // fix-110: local, they never reach the host.
            | "new"
            | "testplan"
    ) && !(cmd == "tui" && offline)
}

/// Reads the token and refuses a networked verb that has none.
pub(crate) fn require_token(cmd: &str, offline: bool) -> String {
    let token = std::env::var("HOMELAB_TOKEN").unwrap_or_default();
    if token.is_empty() && needs_token(cmd, offline) {
        // fix-110: the refusal names where the token goes.
        die(
            "HOMELAB_TOKEN is not set — put HOMELAB_TOKEN=<token> in ~/.config/homelab/env \
             (or export it); it is the token in the daemon's host.toml",
        );
    }
    token
}
