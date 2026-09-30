//! fix-143 (expert panel 2026-09-27, edge-changes-unnoticed): `homelab
//! check` reads the Cloudflare edge with the read-only token Kenny already
//! issued and compares it with the capture. The token is a credential: it
//! goes to curl on stdin, never in argv, where `ps` and the transcripts see
//! it (fix-32 made the same rule for the host's own curl calls).

use homelab_client::edge::{curl_argv, curl_config, token_path};

#[test]
fn fix_143_the_token_goes_to_curl_on_stdin_never_in_argv() {
    let token = "0123456789abcdef0123";
    let url = "https://api.cloudflare.com/client/v4/zones/z/dns_records";
    let argv = curl_argv();
    assert!(
        argv.iter().any(|a| a == "-K"),
        "config from stdin: {argv:?}"
    );
    assert!(argv.iter().any(|a| a == "-"), "{argv:?}");
    assert!(argv.iter().all(|a| !a.contains(token)), "{argv:?}");
    let cfg = curl_config(token, url);
    assert!(
        cfg.contains(&format!("header = \"Authorization: Bearer {}\"", token)),
        "{cfg}"
    );
    assert!(cfg.contains(&format!("url = \"{}\"", url)), "{cfg}");
    // A quote or a newline in the file must not add a curl option.
    let odd = curl_config("a\"b\nurl = \"https://evil\"", url);
    assert_eq!(odd.lines().count(), 2, "one header, one url: {odd}");
    assert_eq!(
        odd.lines().filter(|l| l.starts_with("url = ")).count(),
        1,
        "{odd}"
    );
    assert!(!odd.contains("\"https://evil\""), "{odd}");
}

#[test]
fn fix_143_the_token_is_read_where_kenny_keeps_it() {
    // app-knowledge (2026-09-30): from `edge_token_file` in config/client.toml,
    // found upward from the test's directory like any command's.
    let p = token_path().expect("config/client.toml names edge_token_file");
    assert!(
        p.ends_with(".config/cloudflare/kp-soft.token"),
        "{}",
        p.display()
    );
}
