//! O10: the check that keeps an update out of somebody's evening.
//!
//! Every fixture below is the shape Jellyfin actually returned on 2026-08-31,
//! read off the live server with a working key — not a shape imagined from the
//! documentation. That distinction is the whole reason this test exists: the
//! v1 check tested for a field called `IsPlaying` that does not exist, so its
//! one positive case could never fire.

use homelab_core::ops::busy::{interpret, Busy};

/// app-knowledge (2026-09-30): the Jellyfin reading lives in
/// stacks/media/jellyfin/checks.yml. These fixtures run the part of that
/// command after the fetch (the empty-answer guard and the jq program) with
/// `S` set to Jellyfin's body, through a real `sh` and `jq`, and read the
/// verdict the way the host does.
fn jellyfin_busy(body: &str) -> Busy {
    let text = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../stacks/media/jellyfin/checks.yml"),
    )
    .unwrap();
    let sc: homelab_core::checks::ServiceChecks = serde_yaml::from_str(&text).unwrap();
    let command = sc
        .busy_check
        .expect("jellyfin declares a busy check")
        .command;
    let tail: Vec<&str> = command
        .lines()
        .skip_while(|l| !l.starts_with("S=$("))
        .skip(1)
        .collect();
    assert!(!tail.is_empty(), "the command's shape changed: {}", command);
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(tail.join("\n"))
        .env("S", body)
        .output()
        .expect("sh and jq on the test machine");
    interpret(
        out.status.success(),
        &String::from_utf8_lossy(&out.stdout),
        &String::from_utf8_lossy(&out.stderr),
    )
}

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
use homelab_core::ops::busy::{app_busy, register};
use homelab_core::state::{HostState, StateStore};

const STATE: &str = "/var/lib/homelab";

async fn with_busy_check(exec: &MockExecutor, app: &str, command: &str) {
    let mut checks = std::collections::BTreeMap::new();
    checks.insert(
        app.to_string(),
        homelab_core::checks::ServiceChecks {
            busy_check: Some(homelab_core::checks::BusyCheck {
                command: command.into(),
            }),
            ..Default::default()
        },
    );
    let mut st = HostState::default();
    register(&mut st, "media", &checks);
    StateStore::new(exec, STATE).save(st).await.unwrap();
}

/// An app whose checks.yml declares no busy check is never asked.
#[tokio::test]
async fn o10_an_app_that_does_not_ask_is_not_questioned() {
    let exec = MockExecutor::new();
    with_busy_check(&exec, "jellyfin", "echo SESSIONS").await;
    assert_eq!(
        app_busy(&exec, STATE, 106, "media", "sonarr")
            .await
            .unwrap(),
        None
    );
    assert!(
        exec.calls_containing("SESSIONS").is_empty(),
        "an app with no busy check must not be interrogated"
    );
}

/// The declared command runs in the app's container, and what it prints is
/// who is using it.
#[tokio::test]
async fn o10_a_watching_session_is_seen_through_the_declared_command() {
    let exec = MockExecutor::new();
    with_busy_check(&exec, "jellyfin", "echo SESSIONS").await;
    exec.respond_always("SESSIONS", CmdOutput::ok("kenny is watching Arrival\n"));
    let v = app_busy(&exec, STATE, 106, "media", "jellyfin")
        .await
        .unwrap()
        .unwrap();
    assert!(!v.may_update());
    assert!(homelab_core::ops::busy::reason(&v).contains("Arrival"));
}

/// A command that fails is Unknown, and Unknown blocks: fail closed.
#[tokio::test]
async fn o10_a_failing_busy_check_counts_as_in_use() {
    let exec = MockExecutor::new();
    with_busy_check(&exec, "jellyfin", "echo SESSIONS").await;
    exec.respond_always("SESSIONS", CmdOutput::failed(1, "Jellyfin did not answer"));
    let v = app_busy(&exec, STATE, 106, "media", "jellyfin")
        .await
        .unwrap()
        .unwrap();
    assert!(!v.may_update());
    assert!(homelab_core::ops::busy::reason(&v).contains("did not answer"));
}

// ── The night's three states ───────────────────────────────────────────────

#[test]
fn a_deferred_night_neither_parks_the_stack_nor_records_a_backup() {
    let deferred = NightBackup::of(false, Some("kenny is watching Arrival"));
    assert_eq!(
        deferred,
        NightBackup::Deferred("kenny is watching Arrival".into())
    );
    // fix-59: a backup outcome never parks anything; only a failed update
    // parks, and only the updates (`ops::enable::after_night`).
    assert!(
        !deferred.records_a_timestamp(),
        "nothing was backed up, so nothing may claim a fresh backup — the \
         staleness check is what escalates a stack that keeps standing aside"
    );
}

#[test]
fn a_real_failure_still_parks_and_a_good_night_still_records() {
    assert!(!NightBackup::of(false, None).records_a_timestamp());
    assert!(NightBackup::of(true, None).records_a_timestamp());
    // fix-59: what parks is a failed update, and it parks the updates only;
    // `fix_59_a_failed_night_never_stops_the_stacks_backups` holds that.
}

/// fix-60 (updates-run-after-failed-backup, 2026-09-27): the scheduler ran
/// every automatic update after the backup batch whatever the backup did.
/// Somebody watching a film at 02:00 made the media backup stand aside, and
/// sonarr, radarr, prowlarr, bazarr and seerr were still updated and migrated
/// their databases with no backup from that night to go back to.
#[test]
fn fix_60_no_automatic_update_without_tonights_backup() {
    assert!(NightBackup::Done.allows_update());
    assert!(
        !NightBackup::Deferred("kenny is watching Arrival".into()).allows_update(),
        "a backup that stood aside leaves nothing of tonight to go back to"
    );
    assert!(
        !NightBackup::Failed.allows_update(),
        "a failed backup leaves nothing of tonight to go back to"
    );
    let line = NightBackup::Deferred("jellyfin is playing".into()).update_skip_line("media");
    assert!(
        line.contains("media") && line.contains("jellyfin is playing"),
        "{}",
        line
    );
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
