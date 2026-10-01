//! The pre-commit internal-address gate (`.githooks/check-internal-ips.sh`).
//!
//! guards: public-repo-attack-map (deep-dive: "only replace the internal
//! addresses")

use std::path::PathBuf;
use std::process::Command;

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".githooks/check-internal-ips.sh")
}

fn run_on(diff: &str) -> i32 {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("staged.diff");
    std::fs::write(&file, diff).unwrap();
    Command::new("bash")
        .arg(script())
        .env("CHECK_INTERNAL_IPS_DIFF_FILE", &file)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap_or(-1)
}

/// guards: public-repo-attack-map
#[test]
fn an_added_internal_address_is_refused_in_docs() {
    let leak = "+++ b/docs/x.md\n+reach it at 10.10.10.9:8080 from the LAN\n";
    assert_eq!(
        run_on(leak),
        1,
        "an added 10.x.x.x address in docs must block the commit"
    );

    let leak_192 = "+++ b/README.md\n+the pve host is on 192.168.1.1\n";
    assert_eq!(
        run_on(leak_192),
        1,
        "an added 192.168.x.x address must block the commit"
    );

    let leak_172 = "+++ b/CLAUDE.md\n+the bridge sits at 172.20.0.1\n";
    assert_eq!(
        run_on(leak_172),
        1,
        "an added 172.16-31.x.x address must block the commit"
    );
}

/// guards: public-repo-attack-map
#[test]
fn a_machine_name_or_an_rfc5737_placeholder_passes() {
    let named = "+++ b/docs/x.md\n+reach it at kyu (CT 109):8080 from the LAN\n";
    assert_eq!(run_on(named), 0, "naming the machine must pass");

    let placeholder =
        "+++ b/docs/USER_GUIDE.md\n+the validator refuses `198.51.100.4/24` with host bits set\n";
    assert_eq!(
        run_on(placeholder),
        0,
        "the RFC 5737 documentation range must pass"
    );

    let placeholder2 = "+++ b/docs/x.md\n+example backend 192.0.2.10:8443\n";
    assert_eq!(
        run_on(placeholder2),
        0,
        "the other RFC 5737 documentation range must pass"
    );
}

/// guards: public-repo-attack-map
#[test]
fn removing_a_leaked_address_stays_possible() {
    let removed = "+++ b/docs/x.md\n-reach it at 10.10.10.9:8080 from the LAN\n";
    assert_eq!(
        run_on(removed),
        0,
        "removing a leaked internal address must not be blocked"
    );
}
