//! T49: the reachability monitors follow the fleet.

use homelab_core::ops::monitors::{host_monitors, stale_monitors, Monitor};

fn fleet() -> Vec<(String, String)> {
    vec![
        ("media".into(), "10.10.10.6/24".into()),
        ("gateway".into(), "10.10.10.4/24".into()),
        ("uptime".into(), "10.10.10.7/24".into()),
    ]
}

/// covers: F158
#[test]
fn one_monitor_per_stack_named_the_way_the_existing_set_is() {
    let got = host_monitors(&fleet());
    assert_eq!(
        got,
        vec![
            Monitor {
                name: "host · gateway".into(),
                host: "10.10.10.4".into()
            },
            Monitor {
                name: "host · media".into(),
                host: "10.10.10.6".into()
            },
            Monitor {
                name: "host · uptime".into(),
                host: "10.10.10.7".into()
            },
        ],
        "the names must match the set already on Kuma, or a re-run adds \
         everything a second time under a slightly different name"
    );
    // The CIDR suffix must be gone: a ping monitor for "10.10.10.6/24" is a
    // monitor that can never succeed, and a monitor red from birth teaches
    // its reader to ignore the board.
    assert!(!format!("{:?}", got).contains("/24"));
}

/// covers: F158
#[test]
fn a_monitor_for_a_stack_that_no_longer_exists_is_reported_not_deleted() {
    let existing = vec![
        "host · media".to_string(),
        "host · synctest".to_string(),
        "media · jellyfin".to_string(),
        "extern · fin.kp-soft.dev".to_string(),
    ];
    let stale = stale_monitors(&existing, &fleet());
    assert_eq!(
        stale,
        vec!["host · synctest".to_string()],
        "only the host monitor for a stack the fleet does not have"
    );
    // Application monitors are somebody's own knowledge and are never judged
    // by this rule; neither is an external one.
    assert!(!stale
        .iter()
        .any(|s| s.contains("jellyfin") || s.contains("extern")));
}

#[test]
fn a_stack_without_an_address_is_skipped_rather_than_guessed() {
    let got = host_monitors(&[("native".into(), "".into())]);
    assert!(
        got.is_empty(),
        "no address means no reachable target; inventing one produces a \
         monitor that is red forever: {:?}",
        got
    );
}

#[test]
fn the_generated_file_is_json_the_seeder_can_read() {
    let json = homelab_core::ops::monitors::monitors_json(&host_monitors(&fleet()));
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    let list = v["host_monitors"].as_array().expect("host_monitors array");
    assert_eq!(list.len(), 3);
    assert_eq!(list[0]["name"], "host · gateway");
    assert_eq!(list[0]["hostname"], "10.10.10.4");
    // The header has to say who writes it: this file lands beside a
    // hand-written script and the two are edited in opposite directions.
    assert!(v["_comment"].as_str().unwrap().contains("GENERATED"));
}

#[test]
fn an_empty_fleet_still_renders_a_file_the_seeder_can_parse() {
    // The degenerate case matters: it is what a first deploy on a fresh host
    // produces, and a seeder that dies on startup takes the whole stack's
    // deploy verification with it.
    let json = homelab_core::ops::monitors::monitors_json(&[]);
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid JSON when empty");
    assert!(v["host_monitors"].as_array().unwrap().is_empty());
}

#[test]
fn a_stack_name_with_a_quote_in_it_cannot_break_the_file() {
    let json = homelab_core::ops::monitors::monitors_json(&host_monitors(&[(
        "we\"ird\\one".into(),
        "10.10.10.9/24".into(),
    )]));
    let v: serde_json::Value = serde_json::from_str(&json)
        .expect("an operator-typed stack name must not produce unparseable JSON");
    assert_eq!(v["host_monitors"][0]["name"], "host · we\"ird\\one");
}
