//! Saving and applying settings: comments kept, changes checked like a start, live keys.

use super::support::*;
use crate::*;

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
