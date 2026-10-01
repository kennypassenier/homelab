//! checks-automate (Kenny, 2026-09-30: "Alles wat kan"): the manual
//! questions a machine can answer become probes, measured in every fleet
//! check, with the app's link in the finding.

use std::collections::BTreeMap;

use homelab_core::checks::{Healthy, Layer, Probe, ServiceChecks};
use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::probes::{ProbeReading, evaluate, id_for, register};
use homelab_core::state::HostState;

fn missing_files() -> Probe {
    Probe {
        name: "torrents on missingFiles".into(),
        command: "echo 0".into(),
        healthy: Healthy::Equals("0".into()),
        layer: Layer::Application,
        blind_spot: None,
    }
}

fn checks(probes: Vec<Probe>, url: Option<&str>) -> BTreeMap<String, ServiceChecks> {
    let mut m = BTreeMap::new();
    m.insert(
        "qbittorrent".to_string(),
        ServiceChecks {
            checks: Vec::new(),
            manual: Vec::new(),
            probes,
            busy_check: None,
            url: url.map(String::from),
        },
    );
    m
}

#[test]
fn healthy_judges_text_and_whole_numbers() {
    assert!(Healthy::Equals("ok".into()).judge(" ok\n"));
    assert!(!Healthy::Equals("ok".into()).judge("stalled"));
    assert!(Healthy::AtLeast(1).judge("4"));
    assert!(!Healthy::AtLeast(1).judge("0"));
    assert!(
        !Healthy::AtLeast(1).judge("four"),
        "not a number is not healthy"
    );
    assert!(Healthy::AtMost(0).judge("0"));
    assert!(!Healthy::AtMost(0).judge("2"));
    let yaml = "name: n\ncommand: c\nhealthy: {at_least: 1}\nlayer: application\n";
    let p: Probe = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(p.healthy, Healthy::AtLeast(1));
    let two = "name: n\ncommand: c\nhealthy: {at_least: 1, at_most: 3}\nlayer: application\n";
    assert!(
        serde_yaml::from_str::<Probe>(two).is_err(),
        "one key, not two"
    );
}

#[test]
fn a_deploy_replaces_its_stacks_probes_and_leaves_others_alone() {
    let mut st = HostState::default();
    register(
        &mut st,
        "downloader",
        105,
        &checks(vec![missing_files()], None),
    );
    let mut other = missing_files();
    other.name = "something else".into();
    register(&mut st, "media", 106, &checks(vec![other], None));
    assert_eq!(st.probes.len(), 2);
    register(&mut st, "downloader", 105, &checks(vec![], None));
    assert_eq!(
        st.probes.len(),
        1,
        "gone from the files, gone from the state"
    );
    assert!(st.probes.values().all(|r| r.stack == "media"));
}

#[test]
fn an_unhealthy_reading_is_broken_and_links_the_app() {
    let mut st = HostState::default();
    register(
        &mut st,
        "downloader",
        105,
        &checks(vec![missing_files()], Some("https://qbit.kp-soft.dev")),
    );
    let id = id_for("downloader", "qbittorrent", "torrents on missingFiles");
    assert_eq!(st.probes[&id].vmid, 105);

    let quiet = evaluate(
        &st,
        &[ProbeReading {
            id: id.clone(),
            reading: Ok("0".into()),
        }],
    );
    assert!(quiet.is_empty(), "{:?}", quiet);

    let f = evaluate(
        &st,
        &[ProbeReading {
            id: id.clone(),
            reading: Ok("3".into()),
        }],
    );
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].severity, Severity::Broken);
    assert_eq!(f[0].subject, "downloader/qbittorrent");
    assert!(
        f[0].what.contains("reads \"3\"")
            && f[0].what.contains("\"0\"")
            && f[0].what.contains("(https://qbit.kp-soft.dev)"),
        "{}",
        f[0].what
    );

    let unread = evaluate(
        &st,
        &[ProbeReading {
            id,
            reading: Err("exit 1: no such container".into()),
        }],
    );
    assert_eq!(unread[0].severity, Severity::Drift);
    assert!(
        unread[0].what.contains("could not be read"),
        "{}",
        unread[0].what
    );
}

#[test]
fn a_probe_that_was_not_read_is_silent() {
    let mut st = HostState::default();
    register(
        &mut st,
        "downloader",
        105,
        &checks(vec![missing_files()], None),
    );
    assert!(evaluate(&st, &[]).is_empty());
}

/// Every probe in the repository's checks.yml files parses, has a name,
/// says what healthy is, and its command never echoes a key.
#[test]
fn the_repositorys_probes_are_well_formed() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");
    let mut seen = 0;
    for stack in std::fs::read_dir(&root).unwrap().flatten() {
        for app in std::fs::read_dir(stack.path())
            .into_iter()
            .flatten()
            .flatten()
        {
            let f = app.path().join("checks.yml");
            let Ok(text) = std::fs::read_to_string(&f) else {
                continue;
            };
            let sc: ServiceChecks =
                serde_yaml::from_str(&text).unwrap_or_else(|e| panic!("{}: {}", f.display(), e));
            for p in sc.probes {
                seen += 1;
                assert!(!p.name.trim().is_empty(), "{}", f.display());
                assert!(!p.command.trim().is_empty(), "{}", f.display());
                assert!(
                    !p.command.contains("echo $K") && !p.command.contains("echo \"$K\""),
                    "{}: a probe must not print its key",
                    f.display()
                );
            }
        }
    }
    let _ = seen;
}
