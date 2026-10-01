//! Pure tests for `core::watch`: the minute watch's down/up/flaky bookkeeping
//! (replace-kuma) and decision "deploys are known outages" (Kenny,
//! 2026-09-30) and "default plus per tile" (2026-09-30).

use homelab_admin::core::watch::{
    step, step_deploying, too_soon, view, Seen, Target, DOWN_AFTER_S,
};

fn target(key: &str, stack: Option<&str>) -> Target {
    Target {
        key: key.to_string(),
        name: key.to_string(),
        stack: stack.map(str::to_string),
        link: "/health".into(),
    }
}

#[test]
fn a_failure_becomes_down_only_past_down_after_s() {
    let mut s = Seen::default();
    assert!(step(&mut s, 0, Err("no answer".into()), DOWN_AFTER_S).is_none());
    assert!(!s.down_told);
    // Just short of the threshold: still no notice.
    assert!(step(
        &mut s,
        DOWN_AFTER_S - 1,
        Err("no answer".into()),
        DOWN_AFTER_S
    )
    .is_none());
    assert!(!s.down_told);
    // At the threshold: a Down notice, once.
    let c = step(&mut s, DOWN_AFTER_S, Err("no answer".into()), DOWN_AFTER_S);
    assert!(matches!(
        c,
        Some(homelab_admin::core::watch::Change::Down { .. })
    ));
    assert!(s.down_told);
    assert!(
        step(
            &mut s,
            DOWN_AFTER_S + 60,
            Err("no answer".into()),
            DOWN_AFTER_S
        )
        .is_none(),
        "not told twice"
    );
    // And it answers again: an Up notice.
    let c = step(&mut s, DOWN_AFTER_S + 120, Ok(()), DOWN_AFTER_S);
    assert!(matches!(
        c,
        Some(homelab_admin::core::watch::Change::Up { was_down_s: 120 })
    ));
    assert!(!s.down_told);
}

#[test]
fn a_shorter_down_after_s_tells_sooner() {
    // A tile's own down_after (say 30 s) fires well before the fleet
    // default (300 s) would.
    let mut s = Seen::default();
    assert!(step(&mut s, 0, Err("x".into()), 30).is_none());
    let c = step(&mut s, 30, Err("x".into()), 30);
    assert!(matches!(
        c,
        Some(homelab_admin::core::watch::Change::Down { .. })
    ));
}

#[test]
fn deploying_sends_no_notice_and_clears_the_down_timer() {
    let mut s = Seen::default();
    // It has been failing for a while and was already told down.
    step(&mut s, 0, Err("x".into()), 10);
    step(&mut s, 10, Err("x".into()), 10);
    assert!(s.down_told);
    step_deploying(&mut s, 20);
    assert!(s.deploying);
    assert_eq!(
        s.failing_since, None,
        "the timer is cleared, not just paused"
    );
    assert!(!s.down_told);
    assert_eq!(s.why, None);
    // Once the deploy ends, the timer restarts from zero: an immediate
    // failure is not instantly down again.
    assert!(
        step(&mut s, 25, Err("x".into()), 10).is_none(),
        "the timer starts over"
    );
    assert!(!s.deploying, "a normal step leaves the deploying state");
}

#[test]
fn a_tile_checked_less_than_its_own_watch_every_ago_is_too_soon() {
    assert!(!too_soon(0, 1_000, 60), "never checked is never too soon");
    assert!(too_soon(1_000, 1_030, 60), "30 s < 60 s watch_every");
    assert!(!too_soon(1_000, 1_060, 60), "60 s has passed");
    assert!(!too_soon(1_000, 1_120, 60));
}

#[test]
fn view_reports_up_flaky_down_and_deploying() {
    let up = target("tile:a", Some("media"));
    let flaky = target("tile:b", Some("media"));
    let down = target("tile:c", Some("media"));
    let deploying = target("tile:d", Some("media"));

    let mut seen = std::collections::BTreeMap::new();
    seen.insert("tile:a".to_string(), {
        let mut s = Seen::default();
        step(&mut s, 100, Ok(()), DOWN_AFTER_S);
        s
    });
    seen.insert("tile:b".to_string(), {
        let mut s = Seen::default();
        step(&mut s, 100, Err("x".into()), DOWN_AFTER_S);
        s
    });
    seen.insert("tile:c".to_string(), {
        let mut s = Seen::default();
        step(&mut s, 0, Err("x".into()), 10);
        step(&mut s, 10, Err("x".into()), 10);
        s
    });
    seen.insert("tile:d".to_string(), {
        let mut s = Seen::default();
        step_deploying(&mut s, 100);
        s
    });
    // A target the watch has never measured: absent from `seen` entirely.
    let never = target("tile:e", None);

    let out = view(&[up, flaky, down, deploying, never], &seen);
    let states: Vec<(String, String)> = out
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["key"].as_str().unwrap().to_string(),
                t["state"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        states,
        vec![
            ("tile:a".into(), "up".into()),
            ("tile:b".into(), "flaky".into()),
            ("tile:c".into(), "down".into()),
            ("tile:d".into(), "deploying".into()),
            ("tile:e".into(), "up".into()),
        ]
    );
}
