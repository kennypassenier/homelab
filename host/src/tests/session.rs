//! The websocket session, the RPC handlers and the refusals before the queue.

use super::support::*;
use crate::*;

/// fix-126 (expert panel, unauth-version-plaintext-kyu, 2026-09-27):
/// `/api/version` told any neighbour which release runs, which is what a
/// probe needs to pick a known fault. It now takes the token like the
/// line itself; `/api/health` stays open, it says only `ok`.
#[tokio::test]
pub(crate) async fn fix_126_the_version_endpoint_needs_the_token() {
    let path = format!("/tmp/homelab-fix126-{}.toml", std::process::id());
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
    let (code, body) = http_get(addr, "/api/version", None).await;
    assert_eq!(code, 401, "no token, no version: {}", body);
    assert!(!body.contains(VERSION), "{}", body);
    assert_eq!(state.auth_failures.snapshot().count, 1, "and it is counted");
    let (code, body) = http_get(addr, "/api/version", Some("0123456789abcdef0123")).await;
    assert_eq!((code, body.trim()), (200, VERSION));
    assert_eq!(http_get(addr, "/api/health", None).await.0, 200);
}

/// fix-126: a notification route that sends the bearer token over plain
/// HTTP is named at start. The token to kyu crossed VLAN 10 in clear
/// text, where an ARP-spoofing container reads it.
#[test]
pub(crate) fn fix_126_a_bearer_sent_over_plain_http_is_named() {
    let routes = plaintext_bearer_routes(
        Some("http://10.10.10.9:8080/publish/notify.kenny"),
        Some("tok"),
        Some("http://10.10.5.101:8123/api/webhook/abc"),
        None,
    );
    assert_eq!(routes.len(), 1, "{:?}", routes);
    assert!(
        routes[0].starts_with("http://10.10.10.9:8080"),
        "{:?}",
        routes
    );
    assert!(
        !routes[0].contains("notify.kenny"),
        "path withheld: {:?}",
        routes
    );
    assert!(
        plaintext_bearer_routes(Some("https://kyu.example/publish"), Some("tok"), None, None)
            .is_empty()
    );
    assert!(
        plaintext_bearer_routes(Some("http://127.0.0.1:8080/x"), Some("tok"), None, None)
            .is_empty(),
        "loopback never crosses a wire"
    );
}

/// fix-122 (expert panel, journal-lines-lack-op-context, 2026-09-27):
/// three backups run at once at night, and their journal lines carried
/// only `source=HOST` and the message, so `image not found` could not be
/// tied to one of them; the lines also carried ANSI colour codes, which
/// break `grep` on the journal. Every line of an operation now names the
/// operation and its stack, steps are logged as they start and finish,
/// and no escape code reaches the journal.
#[test]
pub(crate) fn fix_122_journal_lines_name_their_operation_and_stack_without_colour() {
    use homelab_core::runner::{Runner, StepOutcome};
    use tracing::Instrument;
    let out = Captured::default();
    let subscriber = journal_subscriber(out.clone(), "info");
    let dir = std::env::temp_dir().join(format!("homelab-fix122-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let toml = dir.join("host.toml");
    std::fs::write(
        &toml,
        format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        ),
    )
    .unwrap();
    let state = test_state(load_config_from(toml.to_string_lossy().into_owned()));
    tracing::subscriber::with_default(subscriber, || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(
                async {
                    run_op_locked(&state, &RealExecutor, 0, "scheduled-backup", |ctx| {
                        Box::pin(async move {
                            let mut r = Runner::new("backup-paperwork", ctx.sink, ctx.journal);
                            r.plan(&["snapshot"]);
                            let _ = r
                                .step("snapshot", || async {
                                    ctx.sink.emit(PipelineEvent::Line {
                                        level: homelab_core::sink::Level::Warn,
                                        source: "HOST".into(),
                                        msg: "image not found".into(),
                                    });
                                    Ok(StepOutcome::Changed)
                                })
                                .await;
                            r.finish_ok()
                        })
                    })
                    .await
                }
                .instrument(stack_span("paperwork")),
            );
    });
    let _ = std::fs::remove_dir_all(&dir);
    let text = String::from_utf8(out.0.lock().unwrap().clone()).unwrap();
    assert!(!text.contains('\u{1b}'), "no colour codes:\n{}", text);
    let line = text
        .lines()
        .find(|l| l.contains("image not found"))
        .unwrap_or_else(|| panic!("the line reached the journal:\n{}", text));
    assert!(
        line.contains("stack=paperwork") && line.contains("op=scheduled-backup"),
        "the line names its stack and operation: {}",
        line
    );
    assert!(
        text.lines()
            .any(|l| l.contains("snapshot") && l.contains("stack=paperwork")),
        "the step's start and finish are journal lines too:\n{}",
        text
    );
}
