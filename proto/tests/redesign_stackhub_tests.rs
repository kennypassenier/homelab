//! redesign-stackhub (3.71.0): an app's health and the stack's "Env
//! sealed" verdict travel as optional fields, so an older host (which sends
//! neither, and always said `env_sealed: true`) reads as unknown rather
//! than as healthy or sealed.

use homelab_proto::{AppView, StackView};

/// A stack as a 3.70 host sends it.
const OLD_STACK: &str = r#"{
  "name": "kp-soft", "vmid": 116, "hostname": "116-app-kp-soft",
  "apps": [{"name": "web", "running": true, "restarts": 2}],
  "drift": false, "env_sealed": true, "online": true
}"#;

/// covers: redesign-stackhub-1, redesign-stackhub-2
#[test]
fn redesign_stackhub_an_older_hosts_stack_reads_with_health_and_sealed_unknown() {
    let s: StackView = serde_json::from_str(OLD_STACK).expect("an older host's stack reads");
    assert_eq!(
        s.env_sealed_read, None,
        "the old hard-coded true is not a verdict"
    );
    assert!(s.env_sealed, "the old field keeps its old meaning");
    assert_eq!(s.apps[0].health, None);
}

/// covers: redesign-stackhub-1, redesign-stackhub-2
#[test]
fn redesign_stackhub_a_new_hosts_verdicts_survive_the_wire() {
    let mut s: StackView = serde_json::from_str(OLD_STACK).unwrap();
    s.env_sealed = false;
    s.env_sealed_read = Some(false);
    s.apps = vec![AppView {
        name: "web".into(),
        running: true,
        restarts: 0,
        health: Some("unhealthy".into()),
    }];
    let back: StackView = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    assert_eq!(back.env_sealed_read, Some(false));
    assert_eq!(back.apps[0].health.as_deref(), Some("unhealthy"));
}
