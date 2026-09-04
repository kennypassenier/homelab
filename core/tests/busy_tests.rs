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

// ── F280: the same question, asked from the backup path too ────────────────
//
// Everything above tests the verdict. What follows tests WHO ASKS — which is
// the half that was missing. On 2026-09-04 at 04:17 the nightly backup ran
// `docker stop bazarr prowlarr jellyfin seerr radarr sonarr` on CT 106 while
// Kenny was watching an episode; it came back thirty seconds later and his
// player skipped to the next one. The check that prevents exactly this was
// written, correct, armed, and wired into the UPDATE path only. The backup
// path stops the same containers every night and never asked.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::backup::NightBackup;
use homelab_core::ops::busy::{app_busy, wanted_check};

const LABEL_CMD: &str = "com.homelab.update.busy-check";
const SESSIONS_CMD: &str = "jellyfin.db";

fn watching() -> CmdOutput {
    CmdOutput::ok(
        r#"[{"Client":"Jellium Desktop","UserName":"kenny",
             "NowPlayingItem":{"Name":"Arrival"},
             "PlayState":{"IsPaused":false}}]"#,
    )
}

/// An app that carries no label is never asked — one call, not two.
#[tokio::test]
async fn o10_an_app_that_does_not_ask_is_not_questioned() {
    let exec = MockExecutor::new();
    exec.respond_always(LABEL_CMD, CmdOutput::ok("\n"));
    assert_eq!(
        wanted_check(&exec, 106, "media", "sonarr").await.unwrap(),
        None
    );
    assert_eq!(app_busy(&exec, 106, "media", "sonarr").await.unwrap(), None);
    assert!(
        exec.calls_containing(SESSIONS_CMD).is_empty(),
        "an app with no busy-check label must not be interrogated"
    );
}

/// `docker inspect` prints `<no value>` for a label that is not there, and a
/// non-empty string that means "absent" is exactly the kind of value that
/// reads as present. It must not become a check named `<no value>`.
#[tokio::test]
async fn o10_dockers_word_for_absent_is_not_a_check_name() {
    let exec = MockExecutor::new();
    exec.respond_always(LABEL_CMD, CmdOutput::ok("<no value>\n"));
    assert_eq!(
        wanted_check(&exec, 106, "media", "sonarr").await.unwrap(),
        None
    );
}

/// A label naming a check nobody implemented fails closed rather than
/// silently allowing the stop.
#[tokio::test]
async fn o10_an_unknown_check_name_counts_as_in_use() {
    let exec = MockExecutor::new();
    exec.respond_always(LABEL_CMD, CmdOutput::ok("plex\n"));
    let v = app_busy(&exec, 106, "media", "plex")
        .await
        .unwrap()
        .unwrap();
    assert!(!v.may_update(), "an unimplemented check must not allow it");
    assert!(homelab_core::ops::busy::reason(&v).contains("no such check exists"));
}

#[tokio::test]
async fn o10_a_watching_session_is_seen_through_the_shared_probe() {
    let exec = MockExecutor::new();
    exec.respond_always(LABEL_CMD, CmdOutput::ok("jellyfin\n"));
    exec.respond_always(SESSIONS_CMD, watching());
    let v = app_busy(&exec, 106, "media", "jellyfin")
        .await
        .unwrap()
        .unwrap();
    assert!(!v.may_update());
    assert!(homelab_core::ops::busy::reason(&v).contains("Arrival"));
}

// ── The night's three states ───────────────────────────────────────────────

#[test]
fn a_deferred_night_neither_parks_the_stack_nor_records_a_backup() {
    let deferred = NightBackup::of(false, Some("kenny is watching Arrival"));
    assert_eq!(
        deferred,
        NightBackup::Deferred("kenny is watching Arrival".into())
    );
    assert!(
        !deferred.parks_the_stack(true),
        "H8 must not park a stack for being in use — that punishes the house \
         for using its own services"
    );
    assert!(
        !deferred.records_a_timestamp(),
        "nothing was backed up, so nothing may claim a fresh backup — the \
         staleness check is what escalates a stack that keeps standing aside"
    );
}

#[test]
fn a_real_failure_still_parks_and_a_good_night_still_records() {
    assert!(NightBackup::of(false, None).parks_the_stack(true));
    assert!(!NightBackup::of(false, None).records_a_timestamp());
    assert!(NightBackup::of(true, None).records_a_timestamp());
    assert!(!NightBackup::of(true, None).parks_the_stack(true));
    // A failed update parks the stack whatever the backup did.
    assert!(NightBackup::of(true, None).parks_the_stack(false));
    assert!(NightBackup::of(false, Some("in use")).parks_the_stack(false));
}

/// T5: services sharing one container share a fate.
#[test]
fn the_worst_service_decides_the_stacks_night() {
    let deferred = NightBackup::Deferred("in use".into());
    assert_eq!(
        NightBackup::Done.worse_of(deferred.clone()),
        deferred,
        "one deferral defers the stack"
    );
    assert_eq!(
        deferred.clone().worse_of(NightBackup::Failed),
        NightBackup::Failed,
        "a failure outranks a deferral"
    );
    assert_eq!(
        NightBackup::Failed.worse_of(NightBackup::Done),
        NightBackup::Failed
    );
    assert_eq!(
        NightBackup::Done.worse_of(NightBackup::Done),
        NightBackup::Done
    );
}
