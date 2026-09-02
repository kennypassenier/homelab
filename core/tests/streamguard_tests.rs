//! A7 · Kenny chose fail closed: no answer means no update.

use homelab_core::ops::streamguard::{gate, parse_sessions, Gate, StreamFact};

const IDLE: &str = r#"[{"UserName":"Kenny","Id":"a"},{"UserName":"gast","Id":"b"}]"#;
const WATCHING: &str = r#"[
  {"UserName":"Kenny","NowPlayingItem":{"Name":"Pantheon"},"PlayState":{"IsPaused":false}},
  {"UserName":"gast","Id":"b"}
]"#;
const PAUSED: &str = r#"[
  {"UserName":"Kenny","NowPlayingItem":{"Name":"Pantheon"},"PlayState":{"IsPaused":true}}
]"#;

#[test]
fn a_session_with_nothing_loaded_is_not_watching() {
    let f = parse_sessions(IDLE).unwrap();
    assert_eq!(f.playing, 0, "an open app is not a viewing");
    assert_eq!(gate(&f), Gate::Proceed);
}

#[test]
fn a_playing_session_names_who_and_what() {
    let f = parse_sessions(WATCHING).unwrap();
    assert_eq!(f.playing, 1);
    let Gate::Skip(why) = gate(&f) else {
        panic!("an update during a stream must be skipped");
    };
    assert!(why.contains("Kenny") && why.contains("Pantheon"), "{}", why);
}

/// A paused episode is a viewing in progress. Restarting under it loses the
/// place just as thoroughly as restarting during playback.
#[test]
fn a_paused_session_still_blocks_and_says_it_is_paused() {
    let f = parse_sessions(PAUSED).unwrap();
    assert_eq!(f.playing, 1);
    let Gate::Skip(why) = gate(&f) else {
        panic!("paused is still watching");
    };
    assert!(why.contains("(paused)"), "{}", why);
}

#[test]
fn an_unreachable_media_server_skips_the_update_rather_than_assuming_nobody_is_home() {
    let f = StreamFact {
        error: Some("connection refused".into()),
        ..Default::default()
    };
    let Gate::Skip(why) = gate(&f) else {
        panic!("fail closed was Kenny's choice in form A7");
    };
    assert!(why.contains("connection refused"), "{}", why);
    assert!(
        why.contains("skipped rather than guessed"),
        "the reason has to be readable, not a code: {}",
        why
    );
}

#[test]
fn an_answer_that_is_not_json_is_an_error_and_not_an_empty_list() {
    // The failure that would silently permit every update: a login page, a
    // proxy error, an empty body — all of which parse as "no sessions" if
    // nobody checks.
    assert!(parse_sessions("<html>401</html>").is_err());
    assert!(parse_sessions("").is_err());
    assert!(
        parse_sessions("{\"error\":\"nope\"}").is_err(),
        "an object is not a list of sessions"
    );
}

#[test]
fn an_empty_list_really_does_mean_nobody_is_watching() {
    let f = parse_sessions("[]").unwrap();
    assert_eq!(gate(&f), Gate::Proceed);
}

#[test]
fn a_crowd_is_summarised_rather_than_listed_in_full() {
    let many: String = format!(
        "[{}]",
        (0..8)
            .map(|i| format!(
                r#"{{"UserName":"u{}","NowPlayingItem":{{"Name":"f{}"}}}}"#,
                i, i
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    let f = parse_sessions(&many).unwrap();
    assert_eq!(f.playing, 8);
    assert_eq!(f.detail.len(), 3, "the reason is a sentence, not a report");
    let Gate::Skip(why) = gate(&f) else { panic!() };
    assert!(why.contains("8 session(s)"), "{}", why);
}
