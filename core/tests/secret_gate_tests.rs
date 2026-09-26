//! The pre-commit secret gate (`.githooks/check-secrets.sh`).
//!
//! covers: fix-25

use std::path::PathBuf;
use std::process::Command;

fn script() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".githooks/check-secrets.sh")
}

fn run_on(diff: &str) -> i32 {
    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("staged.diff");
    std::fs::write(&file, diff).unwrap();
    Command::new("bash")
        .arg(script())
        .env("CHECK_SECRETS_DIFF_FILE", &file)
        .output()
        .unwrap()
        .status
        .code()
        .unwrap_or(-1)
}

/// covers: fix-25
#[test]
fn an_added_webhook_id_is_refused_and_a_redacted_one_passes() {
    // Assembled at runtime: a literal id here would trip the very gate it tests.
    let id = format!("homelab-ops-{}", "0000abcd");
    let leak = format!("+++ b/docs/x.md\n+POST to http://10.10.10.2:8123/api/webhook/{id} now\n");
    assert_eq!(
        run_on(&leak),
        1,
        "an added webhook id must block the commit"
    );

    let redacted =
        "+++ b/docs/x.md\n+POST to /api/webhook/homelab-ops-<id> and /api/webhook/REDACTED\n";
    assert_eq!(
        run_on(redacted),
        0,
        "a redacted id is what the gate asks for"
    );

    let removed = format!("+++ b/docs/x.md\n-POST to /api/webhook/{id}\n");
    assert_eq!(
        run_on(&removed),
        0,
        "removing a leaked id must stay possible"
    );
}
