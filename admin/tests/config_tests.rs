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
    // The positive twin: the unit was actually read, not an empty file
    // that would also pass the check below.
    assert!(unit.contains("Environment="), "{unit}");
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

/// fix-158: a developer's dashboard (`dev_without_locks`, on the same host
/// token) attached for `homelab ui` steps and, when it closed, left CT 120's
/// dashboard detached. A dashboard without locks never says `UiAttach`; the
/// real one still does, to a host at least as new as itself.
#[test]
fn fix_158_a_dashboard_without_locks_never_attaches_for_ui_steps() {
    use homelab_admin::shell::host_link::{greeting, LinkConfig};
    use homelab_proto::Command;
    let base = format!(
        "[admin]\nhost = \"10.10.10.250:8443\"\nhost_token = \"0123456789abcdef0123\"\n{ACCESS}"
    );
    let real = from_table(table(&base)).unwrap();
    let dev = from_table(table(&format!("{base}dev_without_locks = true\n"))).unwrap();
    let own = env!("CARGO_PKG_VERSION");
    let attaches = |c: &homelab_admin::core::config::AdminConfig, host: &str| {
        greeting(&LinkConfig::from_admin(c), host)
            .iter()
            .any(|cmd| matches!(cmd, Command::UiAttach))
    };
    assert!(attaches(&real, own), "the real dashboard attaches");
    assert!(!attaches(&dev, own), "a dev dashboard never does");
    assert!(!attaches(&real, "3.0.0"), "nor to a host older than itself");
    // Both still ask to read beside the queue.
    let g = greeting(&LinkConfig::from_admin(&dev), own);
    assert!(g
        .iter()
        .any(|cmd| matches!(cmd, Command::SessionOptions { .. })));
}

/// dashboard-latch (Kenny, 2026-09-29, form "Latch"): the dashboard deploys
/// the stacks whose secrets come from latch, so its unit names the latch
/// environment (client/src/spec.rs refuses `latch_secrets` without it), and
/// the sandbox leaves latch a home it can read and write: ProtectHome=yes
/// hides /home, so HOME and LATCH_HOME point into the state directory,
/// which is the one ReadWritePaths entry that is always there.
#[test]
fn dashboard_latch_the_unit_gives_latch_its_environment_and_a_writable_home() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../stacks/admin/admin/admin.service"
    );
    let unit = std::fs::read_to_string(path).unwrap();
    let env: std::collections::BTreeMap<&str, &str> = unit
        .lines()
        .filter_map(|l| l.strip_prefix("Environment="))
        .filter_map(|kv| kv.split_once('='))
        .collect();
    assert_eq!(env.get("HOMELAB_LATCH_ENV"), Some(&"prod"));
    let state = "/appdata/admin/admin-config";
    let latch_home = env.get("LATCH_HOME").expect("LATCH_HOME is set");
    assert!(latch_home.starts_with(&format!("{state}/")), "{latch_home}");
    let home = env.get("HOME").expect("HOME is set");
    assert!(
        *home == state || home.starts_with(&format!("{state}/")),
        "{home}"
    );
    let rw: Vec<&str> = unit
        .lines()
        .filter_map(|l| l.strip_prefix("ReadWritePaths="))
        .flat_map(|l| l.split_whitespace())
        .collect();
    assert!(rw.contains(&state), "{rw:?}");
    // latch drives git over HTTPS and reads its own files; none of the
    // hardening may take either away.
    assert!(unit.contains("ProtectHome=yes"));
    assert!(unit.contains("RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX"));
}
