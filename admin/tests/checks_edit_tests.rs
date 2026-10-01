//! feat-checks-1: the checks form — creating a `checks.yml` an app does not
//! have yet, editing one it does, keeping unchanged items' text, and the
//! validation the browser's form and `stackedit::changes` both apply.

use homelab_admin::core::stackedit::{StackEdit, StackTexts, changes};
use homelab_admin::core::stackedit_checks::{
    CheckEdit, ChecksEdit, ManualEdit, ProbeEdit, checks_problems,
};
use homelab_core::checks::{Check, Expect, Healthy, Layer, Probe};

const BASE: &str = "stack_name: x\nvmid: 150\nhostname: 150-app-x\nnetwork:\n  ip: 10.10.10.50/24\n  gateway: 10.10.10.1\n  bridge: vmbr0\n  vlan: 10\nresources:\n  cores: 1\n  memory_mb: 512\n  swap_mb: 0\n  disk_gb: 8\n  storage: local-lvm\nlxc:\n  template: clone:996\n  unprivileged: true\n  features: nesting=1\n  protection: true\nboot:\n  onboot: true\napps: [sonarr, jellyfin]\n";

const JELLY: &str = "checks:\n  - name: \"films\"\n    command: \"echo 1\"\n    expect: never_decreases\n    layer: application\nmanual:\n  - \"kijk of het werkt\"\nprobes:\n  - name: \"errors\"\n    command: \"echo 0\"\n    healthy: {equals: \"0\"}\n    layer: application\nbusy_check:\n  command: \"echo busy\"\nurl: \"https://fin.example/\"\n";

fn texts() -> StackTexts {
    let mut t = StackTexts::new();
    t.insert("lxc-compose.yml".to_string(), BASE.to_string());
    t.insert("jellyfin/checks.yml".to_string(), JELLY.to_string());
    t
}

fn jelly_check() -> Check {
    Check {
        name: "films".to_string(),
        command: "echo 1".to_string(),
        expect: Expect::NeverDecreases,
        layer: Layer::Application,
        blind_spot: None,
    }
}

fn jelly_probe() -> Probe {
    Probe {
        name: "errors".to_string(),
        command: "echo 0".to_string(),
        healthy: Healthy::Equals("0".to_string()),
        layer: Layer::Application,
        blind_spot: None,
    }
}

fn jelly_as_is() -> ChecksEdit {
    ChecksEdit {
        app: "jellyfin".to_string(),
        checks: Some(vec![CheckEdit {
            origin: Some(0),
            check: jelly_check(),
        }]),
        manual: Some(vec![ManualEdit {
            origin: Some(0),
            text: "kijk of het werkt".to_string(),
            once: false,
        }]),
        probes: Some(vec![ProbeEdit {
            origin: Some(0),
            probe: jelly_probe(),
        }]),
        busy_check: Some("echo busy".to_string()),
        url: Some("https://fin.example/".to_string()),
    }
}

#[test]
fn an_unchanged_checks_yml_writes_no_file() {
    let out = changes("x", &texts(), &StackEdit::Checks(jelly_as_is()), None).unwrap();
    assert!(out.is_empty(), "{out:?}");
}

#[test]
fn a_new_check_is_added_and_the_old_one_is_kept() {
    let mut edit = jelly_as_is();
    edit.checks.as_mut().unwrap().push(CheckEdit {
        origin: None,
        check: Check {
            name: "series".to_string(),
            command: "echo 2".to_string(),
            expect: Expect::NeverDecreases,
            layer: Layer::Application,
            blind_spot: None,
        },
    });
    let out = changes("x", &texts(), &StackEdit::Checks(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let new = out[0].new.as_ref().unwrap();
    assert!(new.contains("films"), "{new}");
    assert!(new.contains("series"), "{new}");
    let parsed: homelab_core::checks::ServiceChecks = serde_yaml::from_str(new).unwrap();
    assert_eq!(parsed.checks.len(), 2);
}

#[test]
fn removing_a_check_drops_it_from_the_file() {
    let mut edit = jelly_as_is();
    edit.checks = Some(Vec::new());
    let out = changes("x", &texts(), &StackEdit::Checks(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    let parsed: homelab_core::checks::ServiceChecks =
        serde_yaml::from_str(out[0].new.as_ref().unwrap()).unwrap();
    assert!(parsed.checks.is_empty());
}

#[test]
fn an_app_with_no_checks_yml_gets_one_created() {
    let edit = ChecksEdit {
        app: "sonarr".to_string(),
        checks: Some(vec![CheckEdit {
            origin: None,
            check: Check {
                name: "queue".to_string(),
                command: "echo 0".to_string(),
                expect: Expect::MustBePresent,
                layer: Layer::Application,
                blind_spot: None,
            },
        }]),
        manual: None,
        probes: None,
        busy_check: None,
        url: Some("https://sonarr.example/".to_string()),
    };
    let out = changes("x", &texts(), &StackEdit::Checks(edit), None).unwrap();
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].path, "stacks/x/sonarr/checks.yml");
    assert!(out[0].old.is_none());
    let parsed: homelab_core::checks::ServiceChecks =
        serde_yaml::from_str(out[0].new.as_ref().unwrap()).unwrap();
    assert_eq!(parsed.checks.len(), 1);
    assert_eq!(parsed.url.as_deref(), Some("https://sonarr.example/"));
}

#[test]
fn an_app_the_stack_does_not_have_is_refused() {
    let edit = ChecksEdit {
        app: "not-an-app".to_string(),
        ..Default::default()
    };
    let err = changes("x", &texts(), &StackEdit::Checks(edit), None).unwrap_err();
    assert!(err.why.contains("is not an app of this stack"), "{err:?}");
}

#[test]
fn a_shallow_check_needs_a_blind_spot() {
    let edit = ChecksEdit {
        app: "sonarr".to_string(),
        checks: Some(vec![CheckEdit {
            origin: None,
            check: Check {
                name: "port open".to_string(),
                command: "nc -z localhost 8989".to_string(),
                expect: Expect::MustBePresent,
                layer: Layer::Network,
                blind_spot: None,
            },
        }]),
        ..Default::default()
    };
    let problems = checks_problems(&edit);
    assert!(
        problems.iter().any(|p| p.contains("blind spot")),
        "{problems:?}"
    );
}

#[test]
fn a_check_with_no_name_or_command_is_refused() {
    let edit = ChecksEdit {
        app: "sonarr".to_string(),
        checks: Some(vec![CheckEdit {
            origin: None,
            check: Check {
                name: String::new(),
                command: String::new(),
                expect: Expect::MustBePresent,
                layer: Layer::Application,
                blind_spot: None,
            },
        }]),
        ..Default::default()
    };
    let problems = checks_problems(&edit);
    assert!(
        problems.iter().any(|p| p.contains("needs a name")),
        "{problems:?}"
    );
    assert!(
        problems.iter().any(|p| p.contains("needs a command")),
        "{problems:?}"
    );
}
