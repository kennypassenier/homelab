//! tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
//! dashboard's minute watch asks a tile's own `probe` address directly —
//! no Traefik hop, no Host header, no `HOMELAB_ADMIN_WATCH_VIA` (retired).
//! A tile with a `probe` is measured; one without is silently skipped, the
//! same as one with no reading.
//!
//! Also covers decision "deploys are known outages" (Kenny, 2026-09-30):
//! a stack the host or the dashboard itself says is deploying is never a
//! Down notice.

mod act_support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{
    Behaviour, MemFiles, MockHost, RecPusher, Recorder, Script, TestClock, shared, temp_dir,
};
use homelab_admin::shell::actions::{Actions, ActionsDeps, HostPort};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_admin::shell::watch::{Watched, round, snapshot};
use homelab_proto::Command;

/// A no-op `Actions` queue: these tests only read [`Actions::jobs`].
fn actions(host: Arc<dyn HostPort>, notify: Arc<NotifyCenter>, clock: &TestClock) -> Actions {
    Actions::start(ActionsDeps {
        host,
        publish: Arc::new(Recorder::default()),
        files: Arc::new(MemFiles::default()),
        shared: shared(&[]),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    })
}

/// A tiny HTTP/1.1 server that answers one connection with `status`, then
/// stops. Returns the address a probe would use to reach it.
async fn fake_backend(status: &'static str) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        if let Ok((mut sock, _)) = listener.accept().await {
            let mut buf = [0u8; 1024];
            let _ = sock.read(&mut buf).await;
            let resp =
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            let _ = sock.write_all(resp.as_bytes()).await;
        }
    });
    format!("http://{}/", addr)
}

fn center(tag: &str, clock: &TestClock) -> Arc<NotifyCenter> {
    let dir = temp_dir(tag);
    NotifyCenter::load(
        dir.join("notifications.json"),
        Arc::new(RecPusher::default()),
        Arc::new(Recorder::default()),
        clock.clock(),
    )
    .unwrap()
}

#[tokio::test]
async fn a_tile_with_a_probe_is_asked_directly_one_with_none_is_skipped() {
    let ok_probe = fake_backend("200 OK").await;
    let tiles = serde_json::json!([
        {
            "stack": "kp-soft", "host": "kp-soft-local",
            "url": "http://10.10.10.16:8080/", "probe": ok_probe,
            "name": "kp-soft", "group": "Own", "order": 1,
        },
        // No `probe`: the client found no own-IP url and no route in the
        // stack's own file resolved this Traefik hostname.
        {
            "stack": "media", "host": "fin.kp-soft.dev",
            "url": "https://fin.kp-soft.dev/",
            "name": "Jellyfin", "group": "Media", "order": 1,
        },
    ]);
    let behaviour: Behaviour = Arc::new(move |c: &Command| match c {
        Command::Tiles { .. } => Script {
            steps: vec![],
            skipped: vec![],
            ok: true,
            deferred: None,
            message: serde_json::json!({ "tiles": tiles }).to_string(),
            drop_line: false,
        },
        _ => Script::ok(&[]),
    });
    let clock = TestClock::at(1_000);
    let host: Arc<dyn HostPort> = MockHost::start(
        clock.clone(),
        behaviour,
        serde_json::json!([]),
        Arc::new(Mutex::new(Vec::new())),
    );
    let notify = center("tile-watch", &clock);
    let sh = shared(&[]);
    let watched: Watched = Default::default();
    let acts = actions(host.clone(), notify.clone(), &clock);

    round(&host, &sh, &notify, &watched, &acts, &clock.clock()).await;

    let snap = snapshot(&watched).await;
    let targets = snap.as_array().cloned().unwrap_or_default();
    assert!(
        targets.iter().any(|t| t["key"] == "tile:kp-soft-local"),
        "a tile with a probe must be watched: {targets:?}"
    );
    assert!(
        !targets.iter().any(|t| t["key"] == "tile:fin.kp-soft.dev"),
        "a tile with no probe must not be watched: {targets:?}"
    );
    // The probed tile answered 200: not down.
    let probed = targets
        .iter()
        .find(|t| t["key"] == "tile:kp-soft-local")
        .unwrap();
    assert_eq!(probed["down"], false, "{probed:?}");
    assert_eq!(probed["state"], "up", "{probed:?}");
}

/// Decision "deploys are known outages" (Kenny, 2026-09-30): a stack the
/// host's `Tiles` answer names as deploying is never asked and never a Down
/// notice — its tile and its container both read "deploying", not "down".
#[tokio::test]
async fn a_stack_the_host_says_is_deploying_is_never_asked_or_reported_down() {
    // A probe nothing answers: if the watch asked it anyway, it would see a
    // connection refused, not silence.
    let dead_probe = "http://127.0.0.1:1/";
    let tiles = serde_json::json!([
        {
            "stack": "media", "host": "fin.kp-soft.dev",
            "url": "https://fin.kp-soft.dev/", "probe": dead_probe,
            "name": "Jellyfin", "group": "Media", "order": 1,
        },
    ]);
    let behaviour: Behaviour = Arc::new(move |c: &Command| match c {
        Command::Tiles { .. } => Script {
            steps: vec![],
            skipped: vec![],
            ok: true,
            deferred: None,
            message: serde_json::json!({
                "tiles": tiles,
                "deploying_stack": "media",
            })
            .to_string(),
            drop_line: false,
        },
        _ => Script::ok(&[]),
    });
    let clock = TestClock::at(1_000);
    let host: Arc<dyn HostPort> = MockHost::start(
        clock.clone(),
        behaviour,
        serde_json::json!([]),
        Arc::new(Mutex::new(Vec::new())),
    );
    let notify = center("tile-watch-deploying", &clock);
    let sh = shared(&[("media", 106, None)]);
    let watched: Watched = Default::default();
    let acts = actions(host.clone(), notify.clone(), &clock);

    round(&host, &sh, &notify, &watched, &acts, &clock.clock()).await;

    let snap = snapshot(&watched).await;
    let targets = snap.as_array().cloned().unwrap_or_default();
    let tile = targets
        .iter()
        .find(|t| t["key"] == "tile:fin.kp-soft.dev")
        .unwrap();
    assert_eq!(tile["state"], "deploying", "{tile:?}");
    assert_eq!(tile["down"], false, "{tile:?}");
    let container = targets.iter().find(|t| t["key"] == "stack:media").unwrap();
    assert_eq!(container["state"], "deploying", "{container:?}");
    assert_eq!(container["down"], false, "{container:?}");
    let snap = notify.snapshot().await;
    assert_eq!(
        snap["notices"].as_array().map(|a| a.len()).unwrap_or(0),
        0,
        "a known outage sends no notice: {snap:?}"
    );
}

/// fix-194: a tile already told Down must not lose that state just because
/// one round in between could not read the host at all (a host restart, a
/// failed `Tiles` ask) — reproduces the 2026-10-02 incident where Proxmox
/// and OPNsense stayed "does not answer" forever because the recovery round
/// wiped `down_told` before it ever had the chance to see the tile answer
/// again.
#[tokio::test]
async fn a_link_blip_after_down_told_still_lets_the_recovery_be_seen() {
    // The probe the tile asks: dead at first, swapped for a live one once
    // the host recovers.
    let probe = Arc::new(Mutex::new("http://127.0.0.1:1/".to_string()));
    let probe_for_closure = probe.clone();
    let behaviour: Behaviour = Arc::new(move |c: &Command| match c {
        Command::Tiles { .. } => {
            let probe_url = probe_for_closure.lock().unwrap().clone();
            let tiles = serde_json::json!([
                {
                    "stack": "media", "host": "fin.kp-soft.dev",
                    "url": "https://fin.kp-soft.dev/", "probe": probe_url,
                    "name": "Jellyfin", "group": "Media", "order": 1,
                    // A short down_after so the test does not need to move
                    // the clock by a real five minutes, and a short
                    // watch_every so every round below actually asks it
                    // rather than being skipped as "too soon" against the
                    // 60 s fleet default.
                    "down_after": 2,
                    "watch_every": 1,
                },
            ]);
            Script {
                steps: vec![],
                skipped: vec![],
                ok: true,
                deferred: None,
                message: serde_json::json!({ "tiles": tiles }).to_string(),
                drop_line: false,
            }
        }
        _ => Script::ok(&[]),
    });
    let clock = TestClock::at(1_000);
    let host: Arc<dyn HostPort> = MockHost::start(
        clock.clone(),
        behaviour,
        serde_json::json!([]),
        Arc::new(Mutex::new(Vec::new())),
    );
    let notify = center("tile-watch-link-blip", &clock);
    let sh = shared(&[("media", 106, None)]);
    let watched: Watched = Default::default();
    let acts = actions(host.clone(), notify.clone(), &clock);

    // Round 1: the tile fails, but not long enough to be told Down yet.
    round(&host, &sh, &notify, &watched, &acts, &clock.clock()).await;
    let snap = snapshot(&watched).await;
    let tile = snap
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["key"] == "tile:fin.kp-soft.dev")
        .unwrap()
        .clone();
    assert_eq!(tile["down"], false, "not down yet: {tile:?}");

    // Round 2: past down_after (2s) — told Down.
    clock.advance(3);
    round(&host, &sh, &notify, &watched, &acts, &clock.clock()).await;
    let snap = snapshot(&watched).await;
    let tile = snap
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["key"] == "tile:fin.kp-soft.dev")
        .unwrap()
        .clone();
    assert_eq!(tile["down"], true, "told down: {tile:?}");
    let down_notice = notify.snapshot().await;
    assert!(
        down_notice["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["title"]
                .as_str()
                .unwrap_or_default()
                .contains("does not answer")),
        "a Down notice went out: {down_notice:?}"
    );

    // Round 3: the host restarts — the line is down, so neither the stacks
    // nor the tiles can be read this round. Before fix-194 this round would
    // forget the tile entirely (`seen.retain` ran unconditionally), wiping
    // `down_told` and erasing the outage that was just told.
    {
        let mut s = sh.write().await;
        s.link_error = Some("the host does not answer".into());
    }
    clock.advance(10);
    round(&host, &sh, &notify, &watched, &acts, &clock.clock()).await;

    // Round 4: the line and the tile both recover.
    {
        let mut s = sh.write().await;
        s.link_error = None;
    }
    *probe.lock().unwrap() = fake_backend("200 OK").await;
    clock.advance(1);
    round(&host, &sh, &notify, &watched, &acts, &clock.clock()).await;

    let snap = snapshot(&watched).await;
    let tile = snap
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["key"] == "tile:fin.kp-soft.dev")
        .unwrap()
        .clone();
    assert_eq!(tile["down"], false, "answers again: {tile:?}");
    let final_notices = notify.snapshot().await;
    assert!(
        final_notices["notices"]
            .as_array()
            .unwrap()
            .iter()
            .any(|n| n["title"]
                .as_str()
                .unwrap_or_default()
                .contains("answers again")),
        "the recovery after the link blip must be told, same as any other \
         Up: {final_notices:?}"
    );
}
