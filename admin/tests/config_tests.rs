//! arch-config: the dashboard's [admin] settings.

use homelab_admin::core::config::from_table;

fn table(s: &str) -> toml::Table {
    s.parse().unwrap()
}

#[test]
fn arch_config_defaults_fill_what_the_file_leaves_out() {
    let c = from_table(table(
        "[admin]\nhost = \"10.10.10.250:8443\"\nhost_token = \"0123456789abcdef0123\"\n",
    ))
    .unwrap();
    assert_eq!(
        (c.poll_s, c.sse_buffer, c.backoff_min_s, c.backoff_max_s),
        (10, 256, 1, 60)
    );
}

#[test]
fn arch_config_every_problem_is_named_at_once() {
    let e = from_table(table(
        "[admin]\nhost = \"pve\"\nhost_token = \"short\"\npoll_s = 0\n",
    ))
    .unwrap_err();
    assert!(
        e.contains("host:port") && e.contains("16 characters") && e.contains("poll_s"),
        "{e}"
    );
}

#[test]
fn arch_config_an_unknown_key_is_refused_not_ignored() {
    let e = from_table(table(
        "[admin]\nhost = \"10.10.10.250:8443\"\nhost_token = \"0123456789abcdef0123\"\npol_s = 5\n",
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
