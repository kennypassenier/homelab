//! O10: the check that keeps an update out of somebody's evening.
//!
//! Every fixture below is the shape Jellyfin actually returned on 2026-08-31,
//! read off the live server with a working key — not a shape imagined from the
//! documentation. That distinction is the whole reason this test exists: the
//! v1 check tested for a field called `IsPlaying` that does not exist, so its
//! one positive case could never fire.

use homelab_core::ops::busy::{jellyfin_busy, Busy};

/// The real answer when somebody has a film open and paused. It still counts:
/// restarting the server drops the session either way.
#[test]
fn o10_a_paused_film_still_counts_as_busy() {
    let body = r#"[
      {"Client":"Jellium Desktop","UserName":"kenny",
       "NowPlayingItem":{"Name":"Arrival"},
       "PlayState":{"IsPaused":true,"PositionTicks":27506650000}},
      {"Client":"Jellyfin Web","UserName":"kenny","PlayState":{"IsPaused":false}}
    ]"#;
    match jellyfin_busy(body) {
        Busy::Yes(who) => {
            assert!(who.contains("kenny") && who.contains("Arrival"), "{}", who);
            assert!(
                who.contains("paused"),
                "the reason should be readable: {}",
                who
            );
        }
        other => panic!("a paused film must count as busy, got {:?}", other),
    }
    assert!(!jellyfin_busy(body).may_update());
}

/// Sessions open but nothing playing: an idle browser tab is not an evening.
#[test]
fn o10_open_but_idle_sessions_do_not_block() {
    let body = r#"[{"Client":"Jellyfin Web","UserName":"kenny","PlayState":{"IsPaused":false}}]"#;
    assert_eq!(jellyfin_busy(body), Busy::No);
    assert!(jellyfin_busy(body).may_update());
}

#[test]
fn o10_nobody_connected_allows_the_update() {
    assert_eq!(jellyfin_busy("[]"), Busy::No);
}

/// The heart of it. The v1 check exited 0 — "safe to update" — for every one
/// of these, so the conditions in which it could not tell whether somebody was
/// watching were exactly the conditions in which it said go ahead.
#[test]
fn o10_every_uncertain_answer_blocks_the_update() {
    for (label, body) in [
        ("an unreachable server", ""),
        (
            "an error page instead of json",
            "<html>502 Bad Gateway</html>",
        ),
        ("a 401 body", r#"{"error":"unauthorized"}"#),
        ("truncated json", r#"[{"NowPlayingItem":"#),
    ] {
        let verdict = jellyfin_busy(body);
        assert!(
            matches!(verdict, Busy::Unknown(_)),
            "{} must be Unknown, got {:?}",
            label,
            verdict
        );
        assert!(
            !verdict.may_update(),
            "{} must block the update — this is the whole point",
            label
        );
    }
}

/// A field that does not exist cannot be a test. Jellyfin has no `IsPlaying`;
/// a body carrying only that must not read as busy OR as idle-by-accident.
#[test]
fn o10_the_v1_field_is_not_what_decides() {
    let body = r#"[{"Client":"x","IsPlaying":true,"PlayState":{"IsPaused":false}}]"#;
    assert_eq!(
        jellyfin_busy(body),
        Busy::No,
        "IsPlaying is not a Jellyfin field; only NowPlayingItem decides"
    );
}
