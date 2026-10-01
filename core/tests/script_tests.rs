//! [`Script`]: a quote-safe builder for the shell snippets core sends over
//! `pct exec`/`lxc-attach`, replacing a `format!` that interpolates a value
//! between bare `'{}'` quotes (shell-strings-quoting, expert panel
//! 2026-09-27; fix-132).

use homelab_core::executor::Script;

#[test]
fn cd_and_cmd_quote_every_value() {
    let got = Script::new()
        .cd("/opt/mystack/myapp")
        .cmd("docker", &["compose", "ps", "-q"])
        .build();
    assert_eq!(got, "cd '/opt/mystack/myapp' && docker 'compose' 'ps' '-q'");
}

#[test]
fn a_value_holding_a_quote_is_escaped_not_injected() {
    let got = Script::new().cd("/appdata/x'; rm -rf /").build();
    // The negative twin: the attacker's `;` never becomes an active
    // separator — it stays inside the single-quoted argument.
    assert!(!got.contains("'; rm -rf /'"), "{got}");
    assert_eq!(got, "cd '/appdata/x'\\''; rm -rf /'");
}

#[test]
fn raw_fragments_are_not_quoted() {
    let got = Script::new().raw("true").raw("echo hi").build();
    assert_eq!(got, "true && echo hi");
}

#[test]
fn pieces_join_with_double_ampersand() {
    let got = Script::new()
        .cd("/opt/a/b")
        .raw("docker compose ps -q")
        .cmd("echo", &["done"])
        .build();
    assert_eq!(got, "cd '/opt/a/b' && docker compose ps -q && echo 'done'");
}
