//! Exchange output: the question prompt and the lines printed for host frames.

use super::*;

/// A question the host raised, as the `Ask` frame carries it.
pub(crate) struct Question {
    pub(crate) id: u64,
    pub(crate) op: String,
    pub(crate) step: String,
    pub(crate) what: String,
    pub(crate) if_allowed: String,
    pub(crate) if_stopped: String,
    pub(crate) boot: Option<String>,
    pub(crate) wait_s: Option<u64>,
}

/// The answer to give: `--answer`, a typed choice on a terminal, or None.
pub(crate) fn choose_answer(q: &Question) -> Option<bool> {
    use std::io::IsTerminal as _;
    let pre_answer = PRE_ANSWER.get().copied().flatten();
    match homelab_client::answer::choose(pre_answer, std::io::stdin().is_terminal()) {
        homelab_client::answer::AnswerChoice::PreAnswered(allow) => {
            eprintln!(
                "  --answer {} :: {}",
                if allow { "allow" } else { "stop" },
                if allow { &q.if_allowed } else { &q.if_stopped }
            );
            Some(allow)
        }
        homelab_client::answer::AnswerChoice::Prompt => {
            eprintln!("    [a] allow — {}", q.if_allowed);
            eprintln!("    [s] stop  — {}", q.if_stopped);
            loop {
                match homelab_client::answer::parse_prompt_answer(&read_typed("  ")) {
                    Some(allow) => break Some(allow),
                    None => eprintln!("  type 'a' or 's'"),
                }
            }
        }
        homelab_client::answer::AnswerChoice::TimesOut => None,
    }
}

pub(crate) fn print_log(level: LogLevel, source: &str, msg: &str) {
    let color = match level {
        LogLevel::Debug => C_DIM,
        LogLevel::Info => C_CYAN,
        LogLevel::Warn => C_YELLOW,
        LogLevel::Error => C_RED,
    };
    println!("{}{:<5}{} {}", color, source, C_RESET, msg);
}

pub(crate) fn print_transfer(label: &str, done: u64, total: Option<u64>) {
    let total_str = total.map(|t| format!("/{}", t)).unwrap_or_default();
    println!(
        "{}⇅ {} {}{} bytes{}",
        C_DIM, label, done, total_str, C_RESET
    );
}

/// G8: plain-text dump for the CLI (`homelab config`).
pub(crate) fn print_config(view: &homelab_proto::HostConfigView) {
    let hour = view
        .backup_hour
        .map(|h| format!("{:02}:00", h))
        .unwrap_or_else(|| "off".into());
    println!("nightly run : {}", hour);
    println!(
        "webhook     : {}",
        view.notify_webhook.as_deref().unwrap_or("off")
    );
    for (i, t) in view.retention.iter().enumerate() {
        let span = t
            .span_days
            .map(|d| format!("for {} days", d))
            .unwrap_or_else(|| "forever".into());
        println!("retention {} : every {} days {}", i + 1, t.every_days, span);
    }
}

/// fix-149: nothing trusted on first use; say where the pin came from.
pub(crate) fn announce_pin(pinned: &Option<homelab_client::link::Pinned>, quiet: bool) {
    use homelab_client::link::Pinned;
    match pinned {
        Some(Pinned::BuiltIn(_) | Pinned::FromRepo(_)) if quiet => {}
        Some(Pinned::BuiltIn(fp)) => eprintln!(
            "{}● pinned host certificate SHA256:{}, the one this client was built with{}",
            C_YELLOW, fp, C_RESET
        ),
        Some(Pinned::FromRepo(fp)) => eprintln!(
            "{}● pinned host certificate SHA256:{} from {}{}",
            C_YELLOW,
            fp,
            homelab_client::repo_config::REPO_FILE,
            C_RESET
        ),
        Some(Pinned::FirstUse(fp)) => {
            eprintln!(
                "{}● pinned host certificate SHA256:{}{}",
                C_YELLOW, fp, C_RESET
            );
            eprintln!(
                "{}  verify this matches the fingerprint the host printed at boot{}",
                C_DIM, C_RESET
            );
        }
        None => {}
    }
}
