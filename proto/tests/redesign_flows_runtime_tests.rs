//! redesign-flows-6 (3.71.0): `StackRuntime` is a read-only command with
//! its own wire name, so an older host refuses it by name (the dashboard
//! then says the version was not reported) and a token of any scope may ask.

use homelab_proto::Command;

#[test]
fn redesign_flows_6_stack_runtime_is_a_named_read_only_command() {
    let c = Command::StackRuntime {
        stack: "kp-soft".into(),
    };
    assert_eq!(c.name(), "stack_runtime");
    assert!(c.is_read_only());
    let back: Command = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
    assert!(matches!(back, Command::StackRuntime { stack } if stack == "kp-soft"));
}
