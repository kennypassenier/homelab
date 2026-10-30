//! host.toml: reading, saving, checking and describing every key.

use super::support::*;
use crate::*;

/// F8: host.toml may only ADD to the no-touch list. Assigning used to be
/// possible, which meant one line in a hand-edited file could drop VM 100
/// and VM 101 out of protection without a word.
#[test]
pub(crate) fn the_config_can_widen_the_no_touch_list_but_never_shrink_it() {
    let raw = "token = \"0123456789abcdef0123\"\nno_touch = [200, 201]\n";
    let cfg = config_from_text(raw);

    for compiled in homelab_core::safety::DEFAULT_NO_TOUCH {
        assert!(
            cfg.safety.no_touch.contains(compiled),
            "config dropped {} out of the no-touch list",
            compiled
        );
    }
    assert!(cfg.safety.no_touch.contains(&200));
    assert!(cfg.safety.no_touch.contains(&201));
}

/// fix-36: a vmid host.toml adds to the no-touch list is refused by
/// remote exec too, not only by the operations.
#[test]
pub(crate) fn fix_36_remote_exec_refuses_a_vmid_the_config_added_to_the_no_touch_list() {
    let raw = "token = \"0123456789abcdef0123\"\nexec_enabled = true\nno_touch = [200]\n";
    let cfg = config_from_text(raw);
    assert!(
        exec_allowed(&cfg, 200).is_err(),
        "vmid 200 is on the configured no-touch list; exec must refuse it"
    );
    assert!(exec_allowed(&cfg, 101).is_err());
    assert!(exec_allowed(&cfg, 150).is_ok());
}

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): without the two
/// keys the host runs with what the fleet uses today, so the policy lands
/// without refusing a single existing deploy; with them it runs with
/// exactly what the file says.
#[test]
pub(crate) fn fix_120_host_policy_defaults_to_the_fleet_and_follows_host_toml() {
    let dir = std::env::temp_dir().join(format!("homelab-fix120-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bare = dir.join("bare.toml");
    std::fs::write(&bare, "token = \"0123456789abcdef0123\"\n").unwrap();
    let cfg = load_config_from(bare.to_string_lossy().into_owned());
    assert_eq!(
        cfg.safety.privileged_vmids.as_deref(),
        Some(homelab_core::safety::FLEET_PRIVILEGED_VMIDS)
    );
    assert_eq!(
        cfg.safety.data_mount_roots,
        Some(
            homelab_core::safety::FLEET_DATA_MOUNT_ROOTS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
        )
    );
    let set = dir.join("set.toml");
    std::fs::write(
        &set,
        "token = \"0123456789abcdef0123\"\nprivileged_vmids = [106]\n\
         data_mount_roots = [\"/HDD18TB/media\"]\n",
    )
    .unwrap();
    let cfg = load_config_from(set.to_string_lossy().into_owned());
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(cfg.safety.privileged_vmids, Some(vec![106]));
    assert_eq!(
        cfg.safety.data_mount_roots,
        Some(vec!["/HDD18TB/media".to_string()])
    );
}

/// F186: the exact file that was live on 2026-09-02 (see REGISTER.md).
#[test]
pub(crate) fn misplaced_keys_are_reported_not_swallowed() {
    let raw = r#"
token = "0123456789abcdef0123"
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"

[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[registry_cache]
host = "10.10.10.17"
pull_timeout_secs = 180

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003

opnsense_url = "https://10.10.5.1"
opnsense_cred_file = "/var/lib/homelab/secrets/opnsense.cred"
"#;
    let t: toml::Table = toml::from_str(raw).unwrap();

    // Serde's own view: the file parses cleanly and says nothing. That
    // silence is what made the fault invisible, and it is still the
    // behaviour — which is why the check below has to exist. (The two
    // keys were removed from the daemon altogether on 2026-09-02, form
    // V4; the fixture stays as it was found, because the trap it
    // demonstrates belongs to TOML, not to those particular settings.)
    let _parsed: FileConfig = t.clone().try_into().unwrap();

    assert_eq!(
        unknown_keys(&t),
        vec![
            "registry_cache.upstreams[0].opnsense_cred_file".to_string(),
            "registry_cache.upstreams[0].opnsense_url".to_string(),
        ]
    );
}

/// The same settings written in the right place: nothing to report, and
/// the daemon actually gets them.
#[test]
pub(crate) fn correctly_placed_keys_are_clean_and_read() {
    let raw = r#"
token = "0123456789abcdef0123"
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"
tile_watch_source = "10.10.10.20"

[registry_cache]
host = "10.10.10.17"

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
    let t: toml::Table = toml::from_str(raw).unwrap();
    assert!(unknown_keys(&t).is_empty(), "{:?}", unknown_keys(&t));

    let parsed: FileConfig = t.try_into().unwrap();
    assert!(parsed.metrics_targets_dir.is_some());
    assert!(parsed.tile_watch_source.is_some());
}

/// A plain typo is loud too — the direction that costs a warning rather
/// than a silence.
#[test]
pub(crate) fn a_typo_is_reported() {
    let t: toml::Table = toml::from_str("metrics_target_dir = \"/x\"\n").unwrap();
    assert_eq!(unknown_keys(&t), vec!["metrics_target_dir".to_string()]);
}

/// config-four-representations (see REGISTER.md fix-134): the known keys
/// are whatever the one struct reads.
#[test]
pub(crate) fn a_key_the_struct_does_not_read_is_reported_even_where_a_list_named_it() {
    let raw = "[[retention]]\nevery_days = 1\nspan_days = 7\nkeep = 3\n";
    let t: toml::Table = toml::from_str(raw).unwrap();
    assert_eq!(unknown_keys(&t), vec!["retention[0].keep".to_string()]);
}

/// fix-false-alarm (2026-09-30): what to check before a `watch_url`
/// save is accepted — absent or empty needs no request (turning the
/// watch off is always allowed), a non-empty value does.
#[test]
pub(crate) fn fix_false_alarm_watch_url_to_check() {
    use std::collections::BTreeMap;
    assert_eq!(crate::watch_url_to_check(&BTreeMap::new()), None);
    assert_eq!(
        crate::watch_url_to_check(&BTreeMap::from([(
            "watch_url".to_string(),
            serde_json::json!("")
        )])),
        None
    );
    assert_eq!(
        crate::watch_url_to_check(&BTreeMap::from([(
            "watch_url".to_string(),
            serde_json::json!("  ")
        )])),
        None
    );
    assert_eq!(
        crate::watch_url_to_check(&BTreeMap::from([(
            "backup_hour".to_string(),
            serde_json::json!(4)
        )])),
        None,
        "a save of some other key must not run the probe"
    );
    assert_eq!(
        crate::watch_url_to_check(&BTreeMap::from([(
            "watch_url".to_string(),
            serde_json::json!("http://10.10.10.20:8080/health")
        )])),
        Some("http://10.10.10.20:8080/health".to_string())
    );
}

/// fix-false-alarm: the save is refused with a reason a person can act
/// on when the host's own request failed, and let through otherwise.
#[test]
pub(crate) fn fix_false_alarm_watch_url_refusal() {
    assert_eq!(
        crate::watch_url_refusal("http://10.10.10.20:8080/health", true),
        None
    );
    let why = crate::watch_url_refusal("http://10.10.10.20:8080/health", false)
        .expect("a failed request refuses the save");
    assert!(why.contains("10.10.10.20:8080/health"), "{why}");
    assert!(why.contains("firewall"), "{why}");
}

/// gap-26: absent, the default (`incidents::JOURNAL_MAX_BYTES`) still
/// applies; set, host.toml wins. See REGISTER.md.
#[test]
pub(crate) fn gap_26_journal_max_bytes_defaults_and_follows_host_toml() {
    let cfg = config_from_text("token = \"0123456789abcdef0123\"\n");
    assert_eq!(
        cfg.journal_max_bytes,
        homelab_core::incidents::JOURNAL_MAX_BYTES as u64
    );
    let cfg = config_from_text("token = \"0123456789abcdef0123\"\njournal_max_bytes = 65536\n");
    assert_eq!(cfg.journal_max_bytes, 65536);
}

/// gap-26: `compact_journal_file` honours the configured limit rather
/// than the hardcoded default — a small `max_bytes` cuts a journal the
/// default would have left alone.
#[test]
pub(crate) fn gap_26_compact_journal_file_uses_the_configured_limit() {
    let dir = std::env::temp_dir().join(format!("homelab-gap26-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let line = "{\"op\":\"deploy\",\"at\":1,\"running\":false}\n";
    let content = line.repeat(2000); // well under JOURNAL_MAX_BYTES, well over 64 KiB
    assert!(content.len() < homelab_core::incidents::JOURNAL_MAX_BYTES);
    let path = dir.join("journal.jsonl");
    std::fs::write(&path, &content).unwrap();
    compact_journal_file(&dir.to_string_lossy(), 65536);
    let cut = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        cut.len() < content.len(),
        "a 64 KiB configured limit must cut a journal the 4 MiB default would not: \
         {} -> {}",
        content.len(),
        cut.len()
    );
}

#[test]
pub(crate) fn y1_a_configured_zero_never_means_no_backups() {
    // The dangerous reading: "0 = unlimited" to a person, "0 = none" to
    // the stream. A night with no backups at all, and the only trace is
    // last_backup standing still.
    assert_eq!(effective_concurrency(0), 1, "zero must not mean none");
    assert_eq!(effective_concurrency(1), 1);
    assert_eq!(effective_concurrency(3), 3, "Kenny's choice, form Y4");
    assert_eq!(effective_concurrency(13), 13);
}

#[test]
pub(crate) fn y1_the_default_is_the_number_kenny_chose() {
    assert_eq!(
        default_backup_concurrency(),
        3,
        "form Y4: about a third of the wait, never more than three \
         services quiet at once"
    );
}
