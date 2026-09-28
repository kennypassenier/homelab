//! feat-settings-1: the host.toml key table the dashboard and the host share.

use homelab_core::hostconfig::{
    check_value, is_secret, key_info, redact, valid_window, Access, KEYS,
};
use serde_json::json;

#[test]
fn feat_settings_1_arch_self_keys_are_never_editable() {
    for key in ["token", "tokens", "listen", "state_dir"] {
        let info = key_info(key).expect(key);
        assert!(!info.access.editable(), "{key} is editable");
        assert!(check_value(key, &json!("x")).unwrap_err().contains("ssh"));
    }
    for key in [
        "exec_enabled",
        "no_touch",
        "privileged_vmids",
        "data_mount_roots",
    ] {
        assert_eq!(key_info(key).unwrap().access, Access::SshOnly, "{key}");
    }
    for key in ["notify_auth_bearer", "notify_fallback_auth_bearer", "token"] {
        assert!(is_secret(key), "{key}");
        assert_eq!(redact(key, &json!("s3cret")), serde_json::Value::Null);
    }
    // The dashboard's own route and the backups ask a second confirmation.
    for key in [
        "gateway_vmid",
        "gateway_routes_dir",
        "restic_base",
        "restic_password_file",
    ] {
        assert_eq!(key_info(key).unwrap().access, Access::Confirm, "{key}");
    }
    let mut seen = std::collections::BTreeSet::new();
    for k in KEYS {
        assert!(seen.insert(k.key), "{} twice", k.key);
    }
}

#[test]
fn feat_settings_1_values_are_checked_by_kind() {
    assert!(check_value("backup_hour", &json!(23)).is_ok());
    assert!(check_value("backup_hour", &json!(24)).is_err());
    assert!(check_value("backup_hour", &json!("4")).is_err());
    assert!(check_value("backup_hour", &serde_json::Value::Null).is_ok());
    assert!(check_value("notify_webhook", &json!("https://kyu.kp-soft.dev/x")).is_ok());
    assert!(check_value("notify_webhook", &json!("ftp://x")).is_err());
    assert!(check_value("logs_window", &json!("24h")).is_ok());
    assert!(check_value("logs_window", &json!("24")).is_err());
    assert!(check_value("loki_vmid", &json!(113)).is_ok());
    assert!(check_value("loki_vmid", &json!(99)).is_err());
    assert!(check_value("second_copy_dataset", &json!("HDD4TB/restic")).is_ok());
    assert!(check_value("second_copy_dataset", &json!("a\nb")).is_err());
    assert!(check_value("retention", &json!([{"every_days": 1}])).is_ok());
    assert!(check_value("retention", &json!(3)).is_err());
    assert!(check_value("nope", &json!(1)).is_err());
    assert!(valid_window("7d") && !valid_window("d7") && !valid_window("h"));
}

#[test]
fn feat_settings_1_a_scoped_token_keeps_its_name_not_its_hash() {
    let v = redact(
        "tokens",
        &json!([{"name": "admin", "scope": "all", "sha256": "ab".repeat(32)}]),
    );
    assert_eq!(v, json!([{"name": "admin", "scope": "all"}]));
}
