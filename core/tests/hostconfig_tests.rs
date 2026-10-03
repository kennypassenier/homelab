//! feat-settings-1: the host.toml key table the dashboard and the host share.

use homelab_core::hostconfig::{
    Access, KEYS, apply_declared, check_shape, check_value, default_effective, is_host_held,
    is_secret, key_info, merge_changes, redact, valid_window,
};
use serde_json::json;

/// fix-181: the generic per-`Kind` rule that reads a key's own `default`
/// text as a comparable value — an int's is the leading number (the rest,
/// like "(30 days)", is documentation), a bool's is literal, a one-word
/// text default is itself, and a sentinel for "unset" ("none", "off") or
/// anything with more structure than one token (a table, a sentence) has
/// no single value to assert and stays `None`.
#[test]
fn fix_181_default_effective_reads_the_keys_table_generically() {
    assert_eq!(
        default_effective("incident_bundle_max_age_days"),
        Some(json!(90))
    );
    assert_eq!(
        default_effective("incident_bundle_max_count"),
        Some(json!(200))
    );
    assert_eq!(
        default_effective("integrity_data_read_interval_s"),
        Some(json!(2_592_000)),
        "the trailing \"(30 days)\" is documentation, not part of the value"
    );
    assert_eq!(default_effective("log_level"), Some(json!("info")));
    assert_eq!(
        default_effective("log_ring_max_bytes"),
        Some(json!(4_194_304))
    );
    assert_eq!(default_effective("exec_enabled"), Some(json!(false)));
    // Sentinels for "nothing set" give nothing new to compare against.
    assert_eq!(default_effective("second_copy_dataset"), None);
    assert_eq!(default_effective("backup_hour"), None);
    // Multi-token prose or a table default has no single value either.
    assert_eq!(default_effective("prometheus_url"), None);
    assert_eq!(default_effective("retention"), None);
    // An unknown (e.g. retired) key has no row to read a default from.
    assert_eq!(default_effective("kuma_monitors_file"), None);
}

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

/// fix-170: `tokens` is host-held — generated and kept by the host itself,
/// never a secret value, but still never the repository's to declare.
/// `apply_declared` must keep the host's current table untouched the same
/// way it keeps a secret, and refuse a `config/host.toml` that sets it.
#[test]
fn fix_170_tokens_is_host_held_not_secret() {
    assert!(is_host_held("tokens"), "tokens");
    assert!(
        !is_secret("tokens"),
        "tokens is shown (name/scope), unlike a secret"
    );
    assert_eq!(key_info("tokens").unwrap().access, Access::HostHeld);
    assert!(!key_info("tokens").unwrap().access.editable());
}

/// fix-170: `apply_declared` keeps the host's current `tokens` table
/// untouched across an apply that never mentions it, and refuses a
/// `config/host.toml` that sets `tokens` itself.
#[test]
fn fix_170_apply_declared_keeps_tokens_and_refuses_it_in_the_repo() {
    let current: toml::Table = toml::from_str(
        "gateway_vmid = 104\n\
         [[tokens]]\n\
         name = \"admin\"\n\
         scope = \"all\"\n\
         sha256 = \"a\"\n",
    )
    .unwrap();
    let declared: toml::Table = toml::from_str("gateway_vmid = 105\n").unwrap();
    let merged = apply_declared(&declared, &current).unwrap();
    assert_eq!(
        merged.get("tokens"),
        current.get("tokens"),
        "the host's own tokens table survives an apply that never mentions it"
    );

    let bad_declared: toml::Table = toml::from_str(
        "[[tokens]]\n\
         name = \"sneaked-in\"\n\
         scope = \"all\"\n\
         sha256 = \"b\"\n",
    )
    .unwrap();
    let e = apply_declared(&bad_declared, &current).unwrap_err();
    assert!(e.contains("tokens"), "{e}");
}

/// fix-170: the drift check never compares `tokens` — neither side
/// declares it, so a running host with tokens and a repository without
/// them (always true, since `tokens` can never be declared) must not be
/// reported as drift.
#[test]
fn fix_170_drift_ignores_tokens() {
    use homelab_core::ops::fleetcheck::{LiveFacts, evaluate_host_config_drift};

    let mut live = LiveFacts {
        declared_host_config: Some(std::collections::BTreeMap::from([(
            "gateway_vmid".to_string(),
            json!(104),
        )])),
        live_host_config: std::collections::BTreeMap::from([
            ("gateway_vmid".to_string(), json!(104)),
            (
                "tokens".to_string(),
                json!([{"name": "admin", "scope": "all"}]),
            ),
        ]),
        ..Default::default()
    };
    assert!(evaluate_host_config_drift(&live).is_empty(), "no drift");

    // Even if a stray declaration slipped into the declared side, the
    // comparison still ignores the key by name.
    live.declared_host_config
        .as_mut()
        .unwrap()
        .insert("tokens".to_string(), json!([{"name": "x", "scope": "all"}]));
    assert!(
        evaluate_host_config_drift(&live).is_empty(),
        "tokens is never compared, whichever side names it"
    );
}

/// fix-191: a whole-file apply from a working copy that never took the
/// host's own settings must not rewrite them. `unannounced_changes` names
/// every key that would move without the sender naming it, and
/// `declared_changes` is the list `homelab host apply` shows before asking.
#[test]
fn fix_191_only_named_keys_may_move() {
    use homelab_core::hostconfig::{declared_changes, unannounced_changes};
    let current: toml::Table = toml::from_str(
        "backup_hour = 4\nzfs_jobs = [{ source = \"HDD2TB\", target = \"HDD18TB/replica/HDD2TB\" }]\nlog_level = \"info\"\n",
    )
    .unwrap();
    // A stale repository file: zfs_jobs missing, backup_hour edited.
    let merged: toml::Table = toml::from_str("backup_hour = 5\nlog_level = \"info\"\n").unwrap();
    assert_eq!(
        unannounced_changes(&merged, &current, &["backup_hour".to_string()]),
        vec!["zfs_jobs".to_string()],
        "the dashboard named only backup_hour; dropping zfs_jobs must be refused"
    );
    assert!(
        unannounced_changes(
            &merged,
            &current,
            &["backup_hour".into(), "zfs_jobs".into()]
        )
        .is_empty()
    );
    let to_json = |t: &toml::Table| {
        t.iter()
            .map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let shown = declared_changes(&to_json(&merged), &to_json(&current));
    let keys: Vec<&str> = shown.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, vec!["backup_hour", "zfs_jobs"]);
    assert!(shown[1].to.is_none(), "a dropped key shows as dropped");
}

/// fix-guards-7: the fix-240 shape — the repository says 600, the host's
/// own host.toml says 120 — is drift `homelab host diff` reports (and
/// `make release` refuses on); a key only spelled out at its default on
/// one side is not.
///
/// covers: fix-guards-7
#[test]
fn fix_guards_7_host_diff_reports_the_ask_timeout_drift_and_not_a_spelled_out_default() {
    use homelab_core::hostconfig::{declared_changes, drift_lines, effective_drift};
    let repo: std::collections::BTreeMap<String, serde_json::Value> = [
        ("ask_timeout_s".to_string(), json!(600)),
        // Spelled out at its compiled default; the host leaves it unset.
        ("log_level".to_string(), json!("info")),
    ]
    .into_iter()
    .collect();
    let host: std::collections::BTreeMap<String, serde_json::Value> =
        [("ask_timeout_s".to_string(), json!(120))]
            .into_iter()
            .collect();
    let none = std::collections::BTreeMap::new();
    let drift = effective_drift(declared_changes(&repo, &host), &[], &none);
    let keys: Vec<&str> = drift.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, vec!["ask_timeout_s"]);
    assert_eq!(
        drift_lines(&drift),
        vec!["ask_timeout_s: the host runs 120, config/host.toml says 600"]
    );
    let agreed = effective_drift(declared_changes(&repo, &repo), &[], &none);
    assert!(agreed.is_empty());
}

/// fix-guards-7 (review H5b, H5c): the drift is judged against the RUNNING
/// host. A key its binary does not read is a later release's key, not
/// drift; a default is the host binary's own where it reports it, so a
/// default that moved between releases is not mistaken either way.
///
/// covers: fix-guards-7
#[test]
fn fix_guards_7_host_diff_ignores_keys_the_host_does_not_know_and_uses_its_defaults() {
    use homelab_core::hostconfig::{declared_changes, effective_drift, host_facts};
    use std::collections::BTreeMap;
    let repo: BTreeMap<String, serde_json::Value> = [
        // A key a newer release adds; the running host has never heard of it.
        ("a_key_of_the_next_release".to_string(), json!(5)),
        // Spelled out at 300, which is the RUNNING host's compiled default
        // (this tree's default for it is different).
        ("ask_timeout_s".to_string(), json!(300)),
    ]
    .into_iter()
    .collect();
    let host: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    let known = vec!["ask_timeout_s".to_string()];
    let defaults: BTreeMap<String, serde_json::Value> = [("ask_timeout_s".to_string(), json!(300))]
        .into_iter()
        .collect();
    let drift = effective_drift(declared_changes(&repo, &host), &known, &defaults);
    assert!(
        drift.is_empty(),
        "an unknown key or the host's own default counted as drift: {:?}",
        drift.iter().map(|c| &c.key).collect::<Vec<_>>()
    );
    // The same value against a host whose default is something else IS drift.
    let other: BTreeMap<String, serde_json::Value> = [("ask_timeout_s".to_string(), json!(120))]
        .into_iter()
        .collect();
    let drift = effective_drift(declared_changes(&repo, &host), &known, &other);
    assert_eq!(drift.len(), 1);
    // An older host reports neither list: every key counts, this tree's
    // defaults stand in.
    let old = effective_drift(declared_changes(&repo, &host), &[], &BTreeMap::new());
    assert!(old.iter().any(|c| c.key == "a_key_of_the_next_release"));
    // What this tree's host reports about itself.
    let (keys, defaults) = host_facts();
    assert!(keys.iter().any(|k| k == "ask_timeout_s"));
    assert_eq!(
        defaults.get("ask_timeout_s"),
        homelab_core::hostconfig::default_effective("ask_timeout_s").as_ref()
    );
}

/// redesign-config-2 (3.71.0 review): the Settings page shows every key's
/// label, description and default, and the refusal `check_value` gives, to
/// the person using the dashboard. Internal finding ids ("(arch-self)",
/// "fix-191", "gap-26") mean nothing there; they belong in `///` comments.
#[test]
fn redesign_config_2_no_internal_id_reaches_the_settings_page() {
    let id = |s: &str| {
        s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-'))
            .any(|w| {
                ["arch-", "fix-", "gap-", "feat-", "scope-", "redesign-"]
                    .iter()
                    .any(|p| w.starts_with(p) && w.len() > p.len())
            })
    };
    let mut bad = Vec::new();
    for k in KEYS {
        for (what, s) in [("label", k.label), ("help", k.help), ("default", k.default)] {
            if id(s) {
                bad.push(format!("{} {what}: {s}", k.key));
            }
        }
        if let Err(e) = check_value(k.key, &json!("x"))
            && id(&e)
        {
            bad.push(format!("{} refusal: {e}", k.key));
        }
    }
    assert!(
        bad.is_empty(),
        "internal ids shown to users:\n{}",
        bad.join("\n")
    );
}
