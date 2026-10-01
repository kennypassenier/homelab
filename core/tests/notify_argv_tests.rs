//! fix-35: the notification bearer token never reaches curl's argv.

use homelab_core::notify::{curl_args, header_file_content, header_file_path};

#[test]
fn fix_35_the_bearer_token_is_in_the_header_file_not_in_argv() {
    let token = "tok-5f3a9e";
    let path = header_file_path("/var/lib/homelab", 0);
    let args = curl_args("{\"x\":1}", "http://10.10.10.9:8080/publish", Some(&path));
    assert!(
        args.iter().all(|a| !a.contains(token)),
        "token in argv: {args:?}"
    );
    assert!(args.iter().any(|a| a == &format!("@{path}")));
    assert_eq!(
        header_file_content(token),
        format!("authorization: Bearer {token}\n")
    );
    assert!(path.starts_with("/var/lib/homelab/secrets/"));
}

/// fix-123 (expert panel, webhook-id-in-warn-line, 2026-09-27): a failed
/// route logged its whole URL, and the path of a Home Assistant webhook is
/// its id, which fix-25 treats as a secret. The log line keeps what tells
/// the routes apart, scheme, host and port, and withholds the rest.
#[test]
fn fix_123_a_route_in_a_log_line_keeps_its_host_and_withholds_its_path() {
    use homelab_core::notify::route_for_log;
    let id = "homelab-3f9c1e7a2b";
    let shown = route_for_log(&format!("http://10.10.5.101:8123/api/webhook/{id}"));
    assert!(!shown.contains(id), "{shown}");
    assert!(shown.starts_with("http://10.10.5.101:8123"), "{shown}");
    let shown = route_for_log("https://user:pw-9x@kyu.example/publish?token=abc");
    assert!(
        !shown.contains("pw-9x") && !shown.contains("abc") && !shown.contains("publish"),
        "{shown}"
    );
    assert!(shown.contains("kyu.example"), "{shown}");
    // Something that is not a URL at all is not echoed either — it is said
    // plainly to be not one, rather than silently producing an empty line.
    assert_eq!(route_for_log("webhook-id-only"), "<not a URL>");
    assert!(!route_for_log("webhook-id-only").contains("webhook-id-only"));
}

#[test]
fn fix_35_a_route_without_a_token_sends_no_auth_header() {
    let args = curl_args("{}", "http://ha/api/webhook/<id>", None);
    assert!(args.iter().all(|a| !a.starts_with('@')));
    assert_eq!(args.last().unwrap(), "http://ha/api/webhook/<id>");
    assert!(args
        .windows(2)
        .any(|w| w[0] == "-w" && w[1] == "%{http_code}"));
}
