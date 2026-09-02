//! A5 · reading what a unit file needs before it can start.

use homelab_core::native::unit_prereqs;

const KYU: &str = r#"
[Unit]
Description=kyu — durable message hub
After=network-online.target
StartLimitIntervalSec=0

[Service]
TimeoutStopSec=30
Type=simple
User=kyu
Group=kyu
EnvironmentFile=/appdata/kyu/kyu-config/kyu.env
ExecStart=/usr/local/bin/kyu
Restart=always
ReadWritePaths=/appdata/kyu/kyu-config

[Install]
WantedBy=multi-user.target
"#;

const SWITCHBOARD: &str = r#"
[Unit]
Description=HTTPSwitchboard
After=network-online.target kyu.service

[Service]
Type=simple
LoadCredential=config:/appdata/kyu/http-switchboard-config/config.toml
ExecStart=/usr/local/bin/http-switchboard %d/config --listen 10.10.10.9:8083
EnvironmentFile=/appdata/kyu/http-switchboard-config/token.env
DynamicUser=yes
"#;

#[test]
fn it_reads_the_user_the_env_file_and_the_program() {
    let p = unit_prereqs(KYU);
    assert_eq!(p.user.as_deref(), Some("kyu"));
    assert_eq!(p.env_files, vec!["/appdata/kyu/kyu-config/kyu.env"]);
    assert_eq!(p.binary.as_deref(), Some("/usr/local/bin/kyu"));
    assert!(p.credentials.is_empty());
}

#[test]
fn arguments_are_not_part_of_the_program_path() {
    let p = unit_prereqs(SWITCHBOARD);
    assert_eq!(
        p.binary.as_deref(),
        Some("/usr/local/bin/http-switchboard"),
        "the arguments must not end up in the path we check for existence"
    );
}

#[test]
fn a_dynamic_user_is_not_an_account_to_create() {
    let p = unit_prereqs(SWITCHBOARD);
    assert_eq!(
        p.user, None,
        "systemd invents the account per start; creating one would be wrong"
    );
}

#[test]
fn a_load_credential_counts_as_a_file_that_must_be_there() {
    let p = unit_prereqs(SWITCHBOARD);
    assert_eq!(
        p.credentials,
        vec!["/appdata/kyu/http-switchboard-config/config.toml"],
        "systemd copies it in before the service starts, so a missing one \
         fails the start exactly like a missing env file"
    );
}

/// The F227 lesson, applied to reading rather than writing: a key outside
/// [Service] is invisible to systemd, so it must be invisible here too.
#[test]
fn keys_outside_the_service_section_are_ignored() {
    let text =
        "[Unit]\nUser=wrong\nExecStart=/bin/false\n\n[Service]\nUser=right\nExecStart=/bin/true\n";
    let p = unit_prereqs(text);
    assert_eq!(p.user.as_deref(), Some("right"));
    assert_eq!(p.binary.as_deref(), Some("/bin/true"));
}

#[test]
fn an_optional_env_file_is_not_a_prerequisite() {
    let text = "[Service]\nEnvironmentFile=-/etc/default/maybe\nEnvironmentFile=/etc/must\n";
    let p = unit_prereqs(text);
    assert_eq!(
        p.env_files,
        vec!["/etc/must"],
        "systemd's leading '-' means 'may be absent', and refusing to start \
         over one would be stricter than systemd itself"
    );
}

#[test]
fn a_unit_that_needs_nothing_says_so_rather_than_guessing() {
    let p = unit_prereqs("[Service]\nExecStart=/usr/bin/true\n");
    assert_eq!(p.user, None);
    assert!(p.env_files.is_empty());
    assert!(p.credentials.is_empty());
}
