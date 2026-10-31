//! Tests of the UI relay: refusals, the round trip, a held step and the fall-back to an earlier dashboard.

use super::*;

#[tokio::test]
async fn follow_without_a_dashboard_the_step_is_refused_with_what_why_fix() {
    let r = UiRelay::default();
    let (ok, msg) = r
        .relay(
            "wsl",
            Scope::Operate,
            UiStep::State,
            "3.70.0",
            Duration::from_millis(50),
            &|_| {},
        )
        .await;
    assert!(!ok);
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert_eq!(v["refusal"]["what"], "ui state");
    assert!(
        v["refusal"]["fix"]
            .as_str()
            .unwrap()
            .contains("homelab-admin")
    );
}

#[tokio::test]
async fn follow_the_step_reaches_the_attached_session_and_its_answer_comes_back() {
    let r = std::sync::Arc::new(UiRelay::default());
    let (tx, mut rx) = mpsc::channel(4);
    r.attach(7, "admin", tx);
    let r2 = r.clone();
    let dashboard = tokio::spawn(async move {
        let Some(ServerMsg::Ui {
            relay,
            by,
            scope,
            step,
            client_version,
        }) = rx.recv().await
        else {
            panic!("no step")
        };
        assert_eq!(client_version, "3.70.0");
        assert_eq!(
            (by.as_str(), scope, step),
            ("wsl", Scope::Operate, UiStep::Close)
        );
        // Another session may not answer for the dashboard.
        assert!(r2.reply(8, relay, true, "forged".into()).is_err());
        r2.reply(7, relay, true, "{\"ok\":true}".into()).unwrap();
    });
    let (ok, msg) = r
        .relay(
            "wsl",
            Scope::Operate,
            UiStep::Close,
            "3.70.0",
            Duration::from_secs(2),
            &|_| {},
        )
        .await;
    dashboard.await.unwrap();
    assert!(ok);
    assert_eq!(msg, "{\"ok\":true}");
    // Its end detaches it; an older session's end does not.
    r.detach(3);
    assert!(r.attached());
    r.detach(7);
    assert!(!r.attached());
}

/// fix-158 (see REGISTER.md): a second dashboard on the same token ends
/// and falls back to the earlier one still connected.
#[tokio::test]
async fn fix_158_when_the_attached_dashboard_ends_the_one_before_it_is_attached_again() {
    let r = UiRelay::default();
    let (a_tx, _a_rx) = mpsc::channel(4);
    let (b_tx, _b_rx) = mpsc::channel(4);
    r.attach(1, "admin", a_tx);
    r.attach(2, "admin", b_tx);
    assert_eq!(r.attached_session(), Some(2), "the newest attach wins");
    assert_eq!(r.detach(2), Detached::FellBackTo(1, "admin".into()));
    assert_eq!(r.attached_session(), Some(1), "A is still connected");
    assert_eq!(r.detach(1), Detached::NoneLeft);
    assert_eq!(r.attached_session(), None);

    // A session whose line already closed is no fallback.
    let (a_tx, a_rx) = mpsc::channel(4);
    let (b_tx, _b_rx) = mpsc::channel(4);
    r.attach(3, "admin", a_tx);
    r.attach(4, "admin", b_tx);
    drop(a_rx);
    assert_eq!(r.detach(4), Detached::NoneLeft);
    assert_eq!(r.attached_session(), None);

    // An earlier session's end leaves the attached one attached.
    let (a_tx, _a_rx) = mpsc::channel(4);
    let (b_tx, _b_rx) = mpsc::channel(4);
    r.attach(5, "admin", a_tx);
    r.attach(6, "admin", b_tx);
    assert_eq!(r.detach(5), Detached::NotAttached);
    assert_eq!(r.attached_session(), Some(6));
    r.detach(6);
    assert_eq!(r.attached_session(), None, "5 is gone, not a fallback");
}

/// Live view: a paused step outlives the usual wait because the
/// dashboard holds it; the note reaches the CLI; only the dashboard may
/// hold; past the hold the step fails with what, why and fix.
#[tokio::test(start_paused = true)]
async fn follow_a_held_step_outlives_the_usual_wait_and_its_note_reaches_the_cli() {
    let r = std::sync::Arc::new(UiRelay::default());
    let (tx, mut rx) = mpsc::channel(4);
    r.attach(7, "admin", tx);
    let r2 = r.clone();
    let dashboard = tokio::spawn(async move {
        let Some(ServerMsg::Ui { relay, .. }) = rx.recv().await else {
            panic!("no step")
        };
        assert!(r2.hold(8, relay, 600, None).is_err(), "another session");
        r2.hold(7, relay, 600, Some("paused by the viewer kenny".into()))
            .unwrap();
        // Far past the usual 20 s, inside the hold.
        tokio::time::sleep(Duration::from_secs(300)).await;
        r2.reply(7, relay, true, "{\"ok\":true}".into()).unwrap();
    });
    let notes = std::sync::Arc::new(Mutex::new(Vec::<String>::new()));
    let n2 = notes.clone();
    let (ok, msg) = r
        .relay(
            "wsl",
            Scope::Operate,
            UiStep::Close,
            "3.70.0",
            RELAY_WAIT,
            &move |n| n2.lock().unwrap().push(n),
        )
        .await;
    dashboard.await.unwrap();
    assert!(ok, "{msg}");
    assert_eq!(
        notes.lock().unwrap().as_slice(),
        ["paused by the viewer kenny"]
    );

    // A hold that runs out answers with what, why and fix.
    let (tx, mut rx) = mpsc::channel(4);
    r.attach(9, "admin", tx);
    let r3 = r.clone();
    tokio::spawn(async move {
        if let Some(ServerMsg::Ui { relay, .. }) = rx.recv().await {
            r3.hold(9, relay, 60, Some("paused by the viewer kenny".into()))
                .unwrap();
        }
    });
    let (ok, msg) = r
        .relay(
            "wsl",
            Scope::Operate,
            UiStep::Close,
            "3.70.0",
            RELAY_WAIT,
            &|_| {},
        )
        .await;
    assert!(!ok);
    let v: serde_json::Value = serde_json::from_str(&msg).unwrap();
    assert!(
        v["refusal"]["why"]
            .as_str()
            .unwrap()
            .contains("paused by the viewer kenny"),
        "{v}"
    );
    assert!(!v["refusal"]["fix"].as_str().unwrap().is_empty());

    // The hold is capped, and the dashboard's end answers at once.
    let (tx, mut rx) = mpsc::channel(4);
    r.attach(11, "admin", tx);
    let r4 = r.clone();
    tokio::spawn(async move {
        if let Some(ServerMsg::Ui { relay, .. }) = rx.recv().await {
            r4.hold(11, relay, u64::MAX, None).unwrap();
            r4.detach(11);
        }
    });
    let started = tokio::time::Instant::now();
    let (ok, _) = r
        .relay(
            "wsl",
            Scope::Operate,
            UiStep::Close,
            "3.70.0",
            RELAY_WAIT,
            &|_| {},
        )
        .await;
    assert!(!ok);
    assert!(started.elapsed() < Duration::from_secs(1));
}
