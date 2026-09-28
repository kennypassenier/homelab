//! arch-tokens (homelab-admin, 2026-09-28): the scope table and command names.

use homelab_proto::{Command, Scope};

/// `name()` is what audit lines print; it must be the wire name, or an audit
/// line names a command nobody can find in the protocol.
#[test]
fn arch_tokens_a_command_name_is_its_wire_name() {
    let samples = vec![
        Command::Ping,
        Command::GetState,
        Command::ZfsReplicate,
        Command::BackupDevices,
        Command::ForgetStack { stack: "x".into() },
        Command::ExecIn {
            vmid: 1,
            command: "ls".into(),
        },
        Command::SessionOptions {
            reads_beside_queue: true,
        },
        Command::WipeRetired {
            name: "x".into(),
            confirm: None,
        },
    ];
    for c in samples {
        let v = serde_json::to_value(&c).unwrap();
        assert_eq!(v["cmd"].as_str(), Some(c.name()), "{:?}", c);
    }
}

#[test]
fn arch_tokens_scopes_are_ordered_and_the_dangerous_commands_need_all() {
    assert!(Scope::Read < Scope::Operate && Scope::Operate < Scope::All);
    for c in [
        Command::ExecIn {
            vmid: 1,
            command: "ls".into(),
        },
        Command::SelfUpdateHost {
            binary_b64: String::new(),
        },
        Command::ForgetStack { stack: "x".into() },
        Command::WipeRetired {
            name: "x".into(),
            confirm: None,
        },
    ] {
        assert_eq!(c.scope(), Scope::All, "{:?}", c);
    }
    assert_eq!(Command::GetState.scope(), Scope::Read);
    assert!(Command::GetState.is_read_only());
    assert!(!Command::BackupDevices.is_read_only());
}

#[test]
fn arch_tokens_a_scope_reads_as_written_in_host_toml() {
    let s: Scope = serde_json::from_str("\"operate\"").unwrap();
    assert_eq!(s, Scope::Operate);
}

/// feat-platform-1: the CLI's requests stay byte-for-byte what an older host
/// reads, because `json: false` is never written.
#[test]
fn feat_platform_1_a_text_request_carries_no_json_key() {
    let v = serde_json::to_value(Command::Doctor { json: false }).unwrap();
    assert_eq!(v, serde_json::json!({ "cmd": "doctor" }));
    let v = serde_json::to_value(Command::Incidents { json: true }).unwrap();
    assert_eq!(v, serde_json::json!({ "cmd": "incidents", "json": true }));
    let old: Command = serde_json::from_str(r#"{"cmd":"list_manual_checks"}"#).unwrap();
    assert!(matches!(old, Command::ListManualChecks { json: false }));
}
