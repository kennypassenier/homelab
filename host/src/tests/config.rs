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

/// config-four-representations (see REGISTER.md fix-134): a settings
/// save writes back the file it read, changing only what the settings
/// tab changed. Nested tables write their defaults out (`max_age_hours`,
/// `pull_timeout_secs`), so the fixture states them.
///
/// covers: F208
#[test]
pub(crate) fn a_settings_save_writes_back_the_file_it_read() {
    let raw = r#"
token = "0123456789abcdef0123"
listen = "0.0.0.0:8443"
backup_hour = 4
notify_webhook = "http://ha/webhook/x"
notify_auth_bearer = "a-token-that-must-survive-a-save"
notify_fallback_webhook = "http://10.10.5.101:8123/api/webhook/homelab"
exec_enabled = true
no_touch = [200]
gateway_vmid = 112
restic_base = "rclone:hdd:homelab-backups"
restic_restore_timeout_s = 9999
logs_window = "6h"
ask_timeout_s = 300
metrics_targets_dir = "/appdata/metrics/prometheus-config/targets"

[[retention]]
every_days = 1
span_days = 7

[[zfs_jobs]]
source = "HDD2TB"
target = "HDD18TB/replica/HDD2TB"

[[watched_backups]]
name = "opnsense-config"
rclone_path = "gdrive:homelab-backups/OPNSense-backups"
max_age_hours = 26

[[device_backups]]
name = "opnsense"
url = "https://10.10.10.1/api/core/backup/download/this"
cred_file = "/var/lib/homelab/secrets/opnsense-backup.conf"
filename = "config.xml"
pin = "sha256//abc"

[registry_cache]
host = "10.10.10.17"
pull_timeout_secs = 180

[[registry_cache.upstreams]]
registry = "lscr.io"
port = 5003
"#;
    let config = config_from_text(raw);

    let mut settings = config.initial_settings.clone();
    settings.backup_hour = Some(5);
    let rendered = render_settings_toml(&config, &settings).expect("render");

    let mut want: toml::Table = toml::from_str(raw).unwrap();
    want.insert("backup_hour".into(), toml::Value::Integer(5));
    let got: toml::Table = toml::from_str(&rendered).unwrap();
    assert_eq!(got, want, "rendered:\n{rendered}");
}

/// fix-134 (see REGISTER.md): `config_from_text` deletes its temp file
/// the moment `load_config_from` returns, so it cannot prove anything
/// about a later read of the same path; this test keeps the file on
/// disk across the save, the way a real settings save finds it.
///
/// covers: fix-134
#[test]
pub(crate) fn fix_134_a_settings_save_keeps_the_file_s_own_comments() {
    let raw = "\
# the token nobody is allowed to rotate without telling Kenny first\n\
token = \"0123456789abcdef0123\"\n\
# backed up at 4am because that is when the drive is quietest\n\
backup_hour = 4\n\
\n\
[[retention]]\n\
every_days = 1\n\
span_days = 7\n";

    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "homelab-host-test-fix134-{}-{}.toml",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, raw).unwrap();
    let config = load_config_from(path.display().to_string());

    let mut settings = config.initial_settings.clone();
    settings.backup_hour = Some(5);
    let rendered = render_settings_toml(&config, &settings).expect("render");
    let _ = std::fs::remove_file(&path);

    assert!(
        rendered.contains("# the token nobody is allowed to rotate without telling Kenny first"),
        "a comment on an untouched key must survive a save:\n{rendered}"
    );
    assert!(
        rendered.contains("# backed up at 4am because that is when the drive is quietest"),
        "a comment on the CHANGED key must survive too — only the value moves:\n{rendered}"
    );
    // And the value changed as asked.
    let got: toml::Table = toml::from_str(&rendered).unwrap();
    assert_eq!(got["backup_hour"].as_integer(), Some(5));
}

/// The dashboard's key table and the file the host reads cannot drift:
/// every field of `FileConfig` has a row (a new field without one fails
/// here), and every row names a field.
#[test]
pub(crate) fn feat_settings_1_every_host_toml_key_is_described_once() {
    let json = serde_json::to_value(FileConfig::default()).unwrap();
    let mut fields: Vec<String> = json.as_object().unwrap().keys().cloned().collect();
    // Skipped when absent, so it is not in the default's JSON.
    fields.push("tokens".into());
    fields.sort();
    let mut rows: Vec<String> = homelab_core::hostconfig::KEYS
        .iter()
        .map(|k| k.key.to_string())
        .collect();
    rows.sort();
    assert_eq!(fields, rows);
}

/// GetHostConfig: every key the file sets, a secret only as "set", a
/// scoped token without its hash, the keys the host does not read, and
/// the file's hash for the write that follows.
#[test]
pub(crate) fn feat_settings_1_the_view_hides_secrets_and_hashes() {
    let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nnotify_auth_bearer = \"s3cret-bearer\"\n\
               loki_url = \"http://10.10.10.13:3100\"\nbogus_key = 1\n\
               [[tokens]]\nname = \"admin\"\nscope = \"all\"\nsha256 = \"{h}\"\n"
        .replace("{h}", &"a".repeat(64));
    let v = host_config_view("/etc/homelab/host.toml", &raw).unwrap();
    let json = serde_json::to_string(&v).unwrap();
    assert!(!json.contains("0123456789abcdef0123"), "{json}");
    assert!(!json.contains("s3cret-bearer"), "{json}");
    assert!(!json.contains(&"a".repeat(64)), "{json}");
    assert_eq!(v.values["backup_hour"], 4);
    assert_eq!(v.values["tokens"][0]["name"], "admin");
    assert_eq!(v.secrets_set, vec!["notify_auth_bearer", "token"]);
    assert_eq!(v.unknown, vec!["bogus_key"]);
    assert_eq!(v.sha256, homelab_core::manifest::sha256_hex(raw.as_bytes()));
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

/// fix-191: a whole-file apply may move only the keys it names. The
/// dashboard's Settings save names the one key it edited; a stale
/// `config/host.toml` that lacks `zfs_jobs` must then be refused, not
/// allowed to drop the host's replication.
#[test]
pub(crate) fn fix_191_a_whole_file_apply_moves_only_the_named_keys() {
    let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nexec_enabled = false\n\
               zfs_jobs = [{ source = \"HDD2TB\", target = \"HDD18TB/replica/HDD2TB\" }]\n";
    let stale = "backup_hour = 5\nexec_enabled = false\n";
    let err = apply_host_config_whole(raw, stale, None, &["backup_hour".to_string()]).unwrap_err();
    assert!(err.contains("zfs_jobs"), "{err}");
    let err = apply_host_config_whole(raw, stale, None, &[]).unwrap_err();
    assert!(
        err.contains("backup_hour") && err.contains("zfs_jobs"),
        "{err}"
    );
    // Named: it goes through, and the secret token is kept.
    let (text, _) = apply_host_config_whole(
        raw,
        stale,
        None,
        &["backup_hour".to_string(), "zfs_jobs".to_string()],
    )
    .unwrap();
    assert!(
        text.contains("backup_hour = 5") && !text.contains("zfs_jobs"),
        "{text}"
    );
    assert!(text.contains("0123456789abcdef0123"));
}

/// SetHostConfig: a change lands with every other key kept; a file that
/// moved meanwhile, a locked or ssh-only key, a secret, a stray sub-key
/// and a value the start-up validation refuses are each refused before
/// anything is written.
#[test]
pub(crate) fn feat_settings_1_a_change_is_checked_like_a_start() {
    use std::collections::BTreeMap;
    let raw = "token = \"0123456789abcdef0123\"\nbackup_hour = 4\nexec_enabled = false\n";
    let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
    let one = |k: &str, v: serde_json::Value| BTreeMap::from([(k.to_string(), v)]);

    let (text, saved) = apply_host_config_changes(
        raw,
        &BTreeMap::from([
            ("backup_hour".to_string(), serde_json::json!(5)),
            ("status_interval_s".to_string(), serde_json::json!(30)),
        ]),
        &sha,
    )
    .unwrap();
    let t: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(t["backup_hour"].as_integer(), Some(5));
    assert_eq!(t["status_interval_s"].as_integer(), Some(30));
    assert_eq!(t["token"].as_str(), Some("0123456789abcdef0123"));
    assert_eq!(saved.live, vec!["backup_hour"]);
    assert_eq!(saved.restart, vec!["status_interval_s"]);
    assert_eq!(
        saved.sha256,
        homelab_core::manifest::sha256_hex(text.as_bytes())
    );

    // null removes the key: the host takes its default.
    let (text, _) =
        apply_host_config_changes(raw, &one("backup_hour", serde_json::Value::Null), &sha).unwrap();
    assert!(!text.contains("backup_hour"), "{text}");
    // The positive twin: the rest of the file is still there, not an
    // empty file that would also pass "no backup_hour".
    assert!(text.contains("token"), "{text}");

    let refused = |changes: BTreeMap<String, serde_json::Value>, expect: &str| {
        apply_host_config_changes(raw, &changes, expect).unwrap_err()
    };
    assert!(refused(one("backup_hour", serde_json::json!(5)), "stale").contains("changed since"));
    for key in [
        "listen",
        "state_dir",
        "tokens",
        "token",
        "exec_enabled",
        "no_touch",
    ] {
        let e = refused(one(key, serde_json::json!("x")), &sha);
        assert!(e.contains("ssh"), "{key}: {e}");
    }
    assert!(refused(one("backup_hour", serde_json::json!(24)), &sha).contains("0 to 23"));
    assert!(refused(one("status_interval_s", serde_json::json!(5)), &sha).contains("10"));
    let e = refused(
        one(
            "registry_cache",
            serde_json::json!({"host": "10.10.10.17", "upstreams": [], "hots": 1}),
        ),
        &sha,
    );
    assert!(e.contains("registry_cache.hots"), "{e}");
    let e = refused(
        one(
            "zfs_jobs",
            serde_json::json!([{"source": "HDD2TB", "target": "HDD2TB"}]),
        ),
        &sha,
    );
    assert!(e.contains("zfs_jobs"), "{e}");
    assert!(refused(one("nonsense", serde_json::json!(1)), &sha).contains("not a setting"));
}

/// fix-122 (AR15's runtime debug toggle, Kenny's go 2026-10-01): a
/// directive `EnvFilter` cannot parse is refused the same way an
/// out-of-range `backup_hour` is — at the save, not discovered on the
/// next restart when the daemon refuses to start at all.
#[test]
pub(crate) fn fix_122_an_unparsable_log_level_is_refused() {
    let raw = "token = \"0123456789abcdef0123\"\n";
    let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
    let changes = std::collections::BTreeMap::from([(
        "log_level".to_string(),
        serde_json::json!("not a directive!!"),
    )]);
    let e = apply_host_config_changes(raw, &changes, &sha).unwrap_err();
    assert!(e.contains("log_level"), "{e}");
    assert!(e.contains("EnvFilter"), "{e}");
}

/// fix-122: a valid directive saves and is live (G8), the same way
/// `backup_hour` is — `saved.live` names it, not `saved.restart`.
#[test]
pub(crate) fn fix_122_a_valid_log_level_saves_as_a_live_key() {
    let raw = "token = \"0123456789abcdef0123\"\n";
    let sha = homelab_core::manifest::sha256_hex(raw.as_bytes());
    let changes =
        std::collections::BTreeMap::from([("log_level".to_string(), serde_json::json!("debug"))]);
    let (text, saved) = apply_host_config_changes(raw, &changes, &sha).unwrap();
    let t: toml::Table = toml::from_str(&text).unwrap();
    assert_eq!(t["log_level"].as_str(), Some("debug"));
    assert_eq!(saved.live, vec!["log_level"]);
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
