//! Tokens and who may do what.

use super::support::*;
use crate::*;

/// fix-120: the token compare is the digest compare, and a wrong token of
/// any length is refused.
#[test]
pub(crate) fn fix_120_the_bearer_check_refuses_every_near_miss() {
    let token = "0123456789abcdef0123";
    assert!(bearer_ok(Some("Bearer 0123456789abcdef0123"), token));
    for bad in [
        "Bearer 0123456789abcdef012",
        "Bearer 0123456789abcdef01234",
        "Bearer 0123456789abcdef0124",
        "Bearer ",
        "bearer 0123456789abcdef0123",
        "",
    ] {
        assert!(!bearer_ok(Some(bad), token), "{:?}", bad);
    }
}

/// fix-120: `random_token` makes a 64-hex-char token and never repeats
/// (a weak source would have shown up as a collision across a thousand
/// draws).
#[test]
pub(crate) fn fix_120_random_token_is_64_hex_and_does_not_repeat() {
    let mut seen = std::collections::HashSet::new();
    for _ in 0..1000 {
        let t = random_token().unwrap();
        assert_eq!(t.len(), 64, "{}", t);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()), "{}", t);
        assert!(seen.insert(t), "random_token repeated");
    }
}

/// fix-120: issuing a token adds exactly one `[[tokens]]` entry, whose
/// SHA-256 is of the plaintext handed in — never the plaintext itself.
#[test]
pub(crate) fn fix_120_issue_token_adds_one_entry_hashed_not_plain() {
    let (text, tokens) =
        issue_token_in("", "wsl", homelab_proto::Scope::Operate, "a-fresh-token").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "wsl");
    assert_eq!(tokens[0].scope, homelab_proto::Scope::Operate);
    use sha2::{Digest, Sha256};
    let want: String = Sha256::digest(b"a-fresh-token")
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect();
    assert_eq!(tokens[0].sha256, want);
    assert!(!text.contains("a-fresh-token"), "{}", text);
    assert!(text.contains(&want), "{}", text);
    // Issuing a second, differently-named token keeps the first.
    let (_, tokens2) =
        issue_token_in(&text, "ct120", homelab_proto::Scope::All, "another-token").unwrap();
    assert_eq!(tokens2.len(), 2);
}

/// fix-120: a duplicate name, an empty name and the reserved name
/// "legacy" are refused rather than silently applied.
#[test]
pub(crate) fn fix_120_issue_token_refuses_bad_names() {
    assert!(issue_token_in("", "", homelab_proto::Scope::Read, "x").is_err());
    assert!(issue_token_in("", "legacy", homelab_proto::Scope::Read, "x").is_err());
    let (text, _) = issue_token_in("", "wsl", homelab_proto::Scope::Read, "x").unwrap();
    assert!(issue_token_in(&text, "wsl", homelab_proto::Scope::All, "y").is_err());
}

/// fix-120: revoking removes only the named entry, and leaves the rest
/// untouched — the whole point of per-machine tokens is that revoking
/// one never affects another.
#[test]
pub(crate) fn fix_120_revoke_token_removes_only_that_one() {
    let (text, _) = issue_token_in("", "wsl", homelab_proto::Scope::Operate, "a").unwrap();
    let (text, _) = issue_token_in(&text, "ct120", homelab_proto::Scope::All, "b").unwrap();
    let (text, tokens) = revoke_token_in(&text, "wsl").unwrap();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "ct120");
    assert!(!text.contains("\"wsl\""), "{}", text);
    // The positive twin: the other token's entry is still there.
    assert!(text.contains("\"ct120\""), "{}", text);
}

/// fix-120: revoking a name that is not there, or "legacy" (the single
/// `token` key, not a `[[tokens]]` entry), is refused.
#[test]
pub(crate) fn fix_120_revoke_token_refuses_unknown_and_legacy() {
    assert!(revoke_token_in("", "legacy").is_err());
    assert!(revoke_token_in("", "nobody").is_err());
}

/// fix-120 (see REGISTER.md): every refusal is counted with the address
/// it came from, for `homelab doctor`.
#[tokio::test]
pub(crate) async fn fix_120_a_refused_connection_is_counted_with_its_peer() {
    let path = format!("/tmp/homelab-fix120-router-{}.toml", std::process::id());
    std::fs::write(&path, "token = \"0123456789abcdef0123\"\n").unwrap();
    let state = test_state(load_config_from(path.clone()));
    let _ = std::fs::remove_file(&path);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = app_router(state.clone());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap()
    });
    let refused = tokio_tungstenite::connect_async(format!("ws://{}/api/ws", addr)).await;
    assert!(refused.is_err(), "no token, no session");
    let seen = state.auth_failures.snapshot();
    assert_eq!(seen.count, 1, "the refusal is counted");
    assert!(
        seen.last_peer
            .as_deref()
            .unwrap_or("")
            .starts_with("127.0.0.1"),
        "and names where it came from: {:?}",
        seen.last_peer
    );
}

#[tokio::test]
pub(crate) async fn arch_tokens_a_read_token_cannot_operate_and_the_refusal_is_audited() {
    let (state, dir) = scoped_state("read");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let replies = session_replies(
        addr,
        "read-token-aaaaaaaaaaaaaaaa",
        vec![(1, Rpc::GetState), (2, Rpc::ZfsReplicate)],
    )
    .await;
    assert_eq!(replies.len(), 2, "{:?}", replies);
    let read = replies.iter().find(|r| r.0 == 1).unwrap();
    let op = replies.iter().find(|r| r.0 == 2).unwrap();
    assert!(read.1 && read.2 == "ran get_state", "{:?}", read);
    assert!(
        !op.1,
        "an operate command on a read token is refused: {:?}",
        op
    );
    assert!(
        op.2.contains("refused") && op.2.contains("dash-read"),
        "{:?}",
        op
    );
    let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap();
    assert!(
        audit.contains("refused token=dash-read") && audit.contains("cmd=zfs_replicate"),
        "{}",
        audit
    );
}

#[tokio::test]
pub(crate) async fn arch_tokens_an_operate_token_cannot_destroy_exec_or_update_the_host() {
    let (state, _dir) = scoped_state("operate");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let replies = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![
            (1, Rpc::BackupDevices),
            (
                2,
                Rpc::ExecIn {
                    vmid: 106,
                    command: "ls".into(),
                },
            ),
            (
                3,
                Rpc::DestroyRecorded {
                    stack: "media".into(),
                    confirm: "media".into(),
                    skip_backup: false,
                },
            ),
            (
                4,
                Rpc::SelfUpdateHost {
                    binary_b64: String::new(),
                    proof: None,
                },
            ),
        ],
    )
    .await;
    let by_id = |id: u64| replies.iter().find(|r| r.0 == id).cloned().unwrap();
    assert!(by_id(1).1, "operate may back up: {:?}", replies);
    for id in [2, 3, 4] {
        assert!(
            !by_id(id).1 && by_id(id).2.contains("refused"),
            "{:?}",
            by_id(id)
        );
    }
}

#[tokio::test]
pub(crate) async fn arch_tokens_a_scope_all_command_by_a_named_token_is_audited_before_it_runs() {
    let (state, dir) = scoped_state("all");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let replies = session_replies(
        addr,
        "all-token-cccccccccccccccccc",
        vec![(
            1,
            Rpc::ForgetStack {
                stack: "drill".into(),
            },
        )],
    )
    .await;
    assert!(replies[0].1, "{:?}", replies);
    let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap();
    assert!(
        audit.contains("scope-all token=dash-all") && audit.contains("cmd=forget_stack"),
        "{}",
        audit
    );
}

#[tokio::test]
pub(crate) async fn arch_tokens_an_unknown_token_opens_no_session() {
    let (state, _dir) = scoped_state("unknown");
    assert_eq!(
        identify(
            Some("Bearer not-a-token-at-all"),
            &state.config.token,
            &state.config.tokens
        ),
        None
    );
    assert_eq!(
        identify(
            Some("Bearer read-token-aaaaaaaaaaaaaaaa"),
            &state.config.token,
            &state.config.tokens
        ),
        Some(Identity {
            name: "dash-read".into(),
            scope: homelab_proto::Scope::Read
        })
    );
    assert_eq!(
        identify(
            Some("Bearer 0123456789abcdef0123"),
            &state.config.token,
            &state.config.tokens
        )
        .map(|i| i.scope),
        Some(homelab_proto::Scope::All)
    );
    assert_eq!(
        identify(None, &state.config.token, &state.config.tokens),
        None
    );
}

#[test]
pub(crate) fn arch_tokens_a_bad_token_list_is_refused_at_start() {
    let e = |name: &str, sha: &str| TokenEntry {
        name: name.into(),
        scope: homelab_proto::Scope::Read,
        sha256: sha.into(),
    };
    let good = sha_hex("x");
    assert!(validate_tokens(&[e("a", &good)]).is_ok());
    assert!(
        validate_tokens(&[e("a", &good), e("a", &good)]).is_err(),
        "duplicate names"
    );
    assert!(
        validate_tokens(&[e("legacy", &good)]).is_err(),
        "reserved name"
    );
    assert!(validate_tokens(&[e("a", "abc")]).is_err(), "short digest");
    assert!(
        validate_tokens(&[e("a", &good.to_uppercase())]).is_err(),
        "uppercase digest"
    );
    assert!(validate_tokens(&[e(" ", &good)]).is_err(), "empty name");
}

#[test]
pub(crate) fn h12_bearer_check() {
    assert!(bearer_ok(
        Some("Bearer secret-token-123"),
        "secret-token-123"
    ));
    assert!(!bearer_ok(Some("Bearer wrong"), "secret-token-123"));
    assert!(
        !bearer_ok(Some("secret-token-123"), "secret-token-123"),
        "scheme required"
    );
    assert!(!bearer_ok(None, "secret-token-123"));
}
