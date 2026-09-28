//! arch-config: the dashboard's [admin] settings.

use homelab_admin::core::config::from_table;

/// The Access settings every valid file carries.
const ACCESS: &str = "access_team_domain = \"example.cloudflareaccess.com\"\naccess_aud = \"e76eb5aa00000000000000000000000000000000000000000000000000000000\"\n";

fn table(s: &str) -> toml::Table {
    s.parse().unwrap()
}

#[test]
fn arch_config_defaults_fill_what_the_file_leaves_out() {
    let c = from_table(table(&format!(
        "[admin]\nhost = \"10.10.10.250:8443\"\nhost_token = \"0123456789abcdef0123\"\n{ACCESS}"
    )))
    .unwrap();
    assert_eq!(
        (c.poll_s, c.sse_buffer, c.backoff_min_s, c.backoff_max_s),
        (10, 256, 1, 60)
    );
}

#[test]
fn arch_config_every_problem_is_named_at_once() {
    let e = from_table(table(
        "[admin]\nhost = \"pve\"\nhost_token = \"short\"\npoll_s = 0\naccess_team_domain = \"evil.example\"\naccess_aud = \"x\"\n",
    ))
    .unwrap_err();
    assert!(
        e.contains("access_team_domain") && e.contains("access_aud"),
        "{e}"
    );
    assert!(
        e.contains("host:port") && e.contains("16 characters") && e.contains("poll_s"),
        "{e}"
    );
}

#[test]
fn arch_config_an_unknown_key_is_refused_not_ignored() {
    let e = from_table(table(
        &format!("[admin]\nhost = \"10.10.10.250:8443\"\nhost_token = \"0123456789abcdef0123\"\npol_s = 5\n{ACCESS}"),
    ))
    .unwrap_err();
    assert!(e.contains("pol_s"), "{e}");
}

#[test]
fn arch_config_a_secret_comes_from_the_environment_and_an_unset_one_is_an_error() {
    use homelab_admin::core::config::expand;
    let env = |k: &str| (k == "TOKEN").then(|| "0123456789abcdef0123".to_string());
    assert_eq!(expand("${TOKEN}", &env).unwrap(), "0123456789abcdef0123");
    assert_eq!(
        expand("a-${TOKEN}-b", &env).unwrap(),
        "a-0123456789abcdef0123-b"
    );
    assert!(expand("${MISSING}", &env).unwrap_err().contains("MISSING"));
    assert!(expand("${OPEN", &env).is_err());
}

/// The settings CT 120 runs with (the unit's Environment= lines plus the
/// secret from admin.env) read, validate, and cannot switch the locks off.
#[test]
fn arch_exposure_the_stacks_settings_keep_the_locks_on() {
    use homelab_admin::core::config::from_env;
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../stacks/admin/admin/admin.service"
    );
    let unit = std::fs::read_to_string(path).unwrap();
    assert!(!unit.contains("LOCKS"), "no variable may name the locks");
    let mut env: std::collections::BTreeMap<String, String> = unit
        .lines()
        .filter_map(|l| l.strip_prefix("Environment="))
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    env.insert(
        "HOMELAB_ADMIN_HOST_TOKEN".into(),
        "0123456789abcdef0123".into(),
    );
    let c = from_env(&|k| env.get(k).cloned()).unwrap();
    assert!(!c.dev_without_locks);
    assert_eq!(c.host, "10.10.10.250:8443");
    assert_eq!(c.access_team_domain, "mendax1.cloudflareaccess.com");
}
