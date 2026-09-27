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

#[test]
fn fix_35_a_route_without_a_token_sends_no_auth_header() {
    let args = curl_args("{}", "http://ha/api/webhook/<id>", None);
    assert!(args.iter().all(|a| !a.starts_with('@')));
    assert_eq!(args.last().unwrap(), "http://ha/api/webhook/<id>");
    assert!(args
        .windows(2)
        .any(|w| w[0] == "-w" && w[1] == "%{http_code}"));
}
