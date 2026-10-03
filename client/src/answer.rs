//! fix-66 (host-questions-unanswerable, 2026-09-27): the command line could
//! not answer a question an operation it started raised — "no answer from
//! here: run this from the TUI to decide" was the whole story, even once
//! the host-side wiring that let an answer reach a waiting operation was
//! fixed. This is the pure half of the client-side decision: given
//! `--answer allow|stop` (parsed once in `main`) and whether stdin is a
//! terminal, what happens to a question that arrives. The I/O (reading a
//! keypress, sending the RPC) stays in `main.rs`, which is not unit-tested;
//! this is, so the three cases cannot silently swap.

/// What a `ServerMsg::Ask` does once it arrives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerChoice {
    /// `--answer allow|stop` already said; send it without waiting.
    PreAnswered(bool),
    /// No pre-answer, but a person is at a terminal: ask `[a]`/`[s]`.
    Prompt,
    /// Neither: print the question and let the host's own timeout turn it
    /// into Unattended, same as before fix-66's client side existed.
    TimesOut,
}

pub fn choose(pre_answer: Option<bool>, stdin_is_terminal: bool) -> AnswerChoice {
    match pre_answer {
        Some(allow) => AnswerChoice::PreAnswered(allow),
        None if stdin_is_terminal => AnswerChoice::Prompt,
        None => AnswerChoice::TimesOut,
    }
}

/// One typed line at the `[a] allow / [s] stop` prompt: `Some(bool)` on a
/// recognised answer, `None` to ask again (anything else, case-insensitive,
/// surrounding whitespace ignored).
pub fn parse_prompt_answer(typed: &str) -> Option<bool> {
    match typed.trim().to_ascii_lowercase().as_str() {
        "a" | "allow" => Some(true),
        "s" | "stop" => Some(false),
        _ => None,
    }
}

/// `--answer <value>`, read from the flag's value: `allow`/`stop`, or an
/// error naming what was typed instead. `None` when the flag is absent —
/// not an error, since the flag is optional.
pub fn parse_pre_answer_flag(value: Option<&str>) -> Result<Option<bool>, String> {
    match value {
        None => Ok(None),
        Some("allow") => Ok(Some(true)),
        Some("stop") => Ok(Some(false)),
        Some(other) => Err(format!("--answer takes allow or stop, got '{}'", other)),
    }
}

/// fix-240: the words after `homelab answer` (or `homelab ui answer`): one
/// `allow` or `stop`, and at most one other word naming the question (its
/// operation `deploy-media`, its subject `media`, or its id), in either
/// order. No name answers the one open question; the host refuses when
/// more than one is open.
pub fn parse_answer_words(words: &[String]) -> Result<(Option<String>, bool), String> {
    let usage = "usage: homelab answer [<operation>|<stack>|<question id>] allow|stop";
    let choices: Vec<bool> = words
        .iter()
        .filter_map(|w| match w.as_str() {
            "allow" => Some(true),
            "stop" => Some(false),
            _ => None,
        })
        .collect();
    let names: Vec<&String> = words
        .iter()
        .filter(|w| !matches!(w.as_str(), "allow" | "stop"))
        .collect();
    match (choices.as_slice(), names.as_slice()) {
        ([allow], []) => Ok((None, *allow)),
        ([allow], [name]) => Ok((Some((*name).clone()), *allow)),
        ([], _) => Err(format!("say allow or stop; {}", usage)),
        _ => Err(usage.to_string()),
    }
}

/// fix-240: what a run that cannot answer its own question prints, so the
/// person (or Claude) reading its output knows how to answer it from
/// elsewhere and how long there is.
pub fn answer_elsewhere_hint(op: &str, wait_s: Option<u64>) -> String {
    let within = wait_s
        .map(|s| format!(" within {}", homelab_core::ask::wait_words(s)))
        .unwrap_or_default();
    format!(
        "  no answer from here: answer it{} with `homelab answer {} allow|stop` \
         (another terminal), `homelab ui answer {} allow|stop` or the dashboard's \
         banner; `homelab <verb> --answer allow|stop` answers without waiting. \
         Unanswered, the operation fails",
        within, op, op
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> Vec<String> {
        s.split_whitespace().map(str::to_string).collect()
    }

    /// covers: fix-240
    #[test]
    fn fix_240_answer_words_take_one_choice_and_at_most_one_name() {
        assert_eq!(parse_answer_words(&w("allow")), Ok((None, true)));
        assert_eq!(
            parse_answer_words(&w("deploy-media stop")),
            Ok((Some("deploy-media".into()), false))
        );
        assert_eq!(
            parse_answer_words(&w("allow media")),
            Ok((Some("media".into()), true))
        );
        assert!(parse_answer_words(&w("media")).is_err());
        assert!(parse_answer_words(&w("allow stop")).is_err());
        assert!(parse_answer_words(&w("a b allow")).is_err());
        assert!(parse_answer_words(&[]).is_err());
    }

    /// covers: fix-240
    #[test]
    fn fix_240_a_headless_run_says_how_to_answer_from_elsewhere() {
        let hint = answer_elsewhere_hint("deploy-media", Some(600));
        assert!(
            hint.contains("homelab answer deploy-media allow|stop"),
            "{hint}"
        );
        assert!(hint.contains("homelab ui answer deploy-media"), "{hint}");
        assert!(hint.contains("within 10 min"), "{hint}");
        assert!(hint.contains("fails"), "{hint}");
    }

    #[test]
    fn fix_66_a_pre_answer_is_used_whatever_stdin_is() {
        assert_eq!(choose(Some(true), true), AnswerChoice::PreAnswered(true));
        assert_eq!(choose(Some(false), false), AnswerChoice::PreAnswered(false));
    }

    #[test]
    fn fix_66_no_pre_answer_prompts_on_a_terminal() {
        assert_eq!(choose(None, true), AnswerChoice::Prompt);
    }

    #[test]
    fn fix_66_no_pre_answer_no_terminal_times_out_as_before() {
        assert_eq!(choose(None, false), AnswerChoice::TimesOut);
    }

    #[test]
    fn fix_66_the_prompt_takes_the_short_or_the_long_word_case_insensitively() {
        assert_eq!(parse_prompt_answer("a"), Some(true));
        assert_eq!(parse_prompt_answer("A"), Some(true));
        assert_eq!(parse_prompt_answer("allow"), Some(true));
        assert_eq!(parse_prompt_answer(" s \n"), Some(false));
        assert_eq!(parse_prompt_answer("stop"), Some(false));
        assert_eq!(parse_prompt_answer("maybe"), None);
        assert_eq!(parse_prompt_answer(""), None);
    }

    #[test]
    fn fix_66_the_answer_flag_takes_allow_or_stop_and_nothing_else() {
        assert_eq!(parse_pre_answer_flag(None), Ok(None));
        assert_eq!(parse_pre_answer_flag(Some("allow")), Ok(Some(true)));
        assert_eq!(parse_pre_answer_flag(Some("stop")), Ok(Some(false)));
        assert!(parse_pre_answer_flag(Some("maybe")).is_err());
    }
}
