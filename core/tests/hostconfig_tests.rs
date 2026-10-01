//! feat-settings-1: the host.toml key table the dashboard and the host share.

use homelab_core::hostconfig::{
    Access, KEYS, apply_declared, check_shape, check_value, is_secret, key_info, merge_changes,
    redact, valid_window,
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

/// fix-110: `check_shape` is `check_value` without the access gate — a
/// locked or ssh-only key is still shape-checked, but no longer refused for
/// who is asking, since the caller (`homelab host apply`,
/// `config/host.toml`'s declarative commit) already decided it may change.
#[test]
fn fix_110_check_shape_has_no_access_gate() {
    assert!(check_shape("listen", &json!("0.0.0.0:8443")).is_ok());
    assert!(check_shape("listen", &json!(1)).is_err());
    assert!(check_shape("no_touch", &json!([100, 101])).is_ok());
    assert!(check_shape("no_touch", &json!("100")).is_err());
    assert!(check_shape("nope", &json!(1)).is_err());
    // check_value still refuses the same key on shape alone for an editable
    // one, and on access for a locked one.
    assert!(
        check_value("listen", &json!("0.0.0.0:8443"))
            .unwrap_err()
            .contains("ssh")
    );
}

/// fix-110: `merge_changes` is the same TOML surgery the host's
/// `SetHostConfig` does, minus the access gate, factored out so the
/// dashboard's declarative commit of `config/host.toml` can build the same
/// new text the host will later apply.
#[test]
fn fix_110_merge_changes_sets_and_removes_keys() {
    let raw = "backup_concurrency = 3\ngateway_vmid = 104\n";
    let mut changes = std::collections::BTreeMap::new();
    changes.insert("backup_concurrency".to_string(), json!(5));
    changes.insert("gateway_vmid".to_string(), serde_json::Value::Null);
    let text = merge_changes(raw, &changes).unwrap();
    let table: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(
        table.get("backup_concurrency").unwrap().as_integer(),
        Some(5)
    );
    assert!(table.get("gateway_vmid").is_none());

    assert!(merge_changes(raw, &std::collections::BTreeMap::new()).is_err());
    let mut bad = std::collections::BTreeMap::new();
    bad.insert("backup_concurrency".to_string(), json!(-1));
    assert!(merge_changes(raw, &bad).is_err());
}

/// fix-110: `apply_declared` lays `config/host.toml` over the host's own
/// file, keeping the host's secrets and dropping anything the repository
/// does not declare — and refuses a repository file that (wrongly) sets a
/// secret itself.
#[test]
fn fix_110_apply_declared_keeps_secrets_and_drops_the_undeclared() {
    let current: toml::Table = toml::from_str(
        "token = \"s3cret-on-the-host\"\ngateway_vmid = 104\nmirror_remote = \"old\"\n",
    )
    .unwrap();
    let declared: toml::Table = toml::from_str("gateway_vmid = 105\n").unwrap();
    let merged = apply_declared(&declared, &current).unwrap();
    assert_eq!(
        merged.get("token").and_then(|v| v.as_str()),
        Some("s3cret-on-the-host"),
        "the host's secret survives an apply that never mentions it"
    );
    assert_eq!(merged.get("gateway_vmid").unwrap().as_integer(), Some(105));
    assert!(
        merged.get("mirror_remote").is_none(),
        "a key the repository does not declare is dropped, like an undeclared stack file"
    );

    let bad_declared: toml::Table = toml::from_str("token = \"leaked\"\n").unwrap();
    assert!(apply_declared(&bad_declared, &current).is_err());
}
