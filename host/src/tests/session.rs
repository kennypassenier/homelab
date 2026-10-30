//! The websocket session, the RPC handlers and the refusals before the queue.

use super::support::*;
use crate::*;

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): the
/// Hello every client prints names the build, not only the version.
#[test]
pub(crate) fn fix_141_the_hello_names_the_build() {
    match crate::hello() {
        homelab_proto::ServerMsg::Hello { version, build, .. } => {
            assert_eq!(version, env!("CARGO_PKG_VERSION"));
            assert_eq!(build.as_deref(), option_env!("HOMELAB_BUILD"));
            assert!(build.is_some_and(|b| !b.is_empty()));
        }
        other => panic!("not a Hello: {other:?}"),
    }
}

/// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): a
/// frame the host could not parse was logged whole. A deploy frame carries
/// every `.env` value of its stack (`DeploySpec.env` is the secrets
/// channel) and a staging frame tens of MB of base64, so one version skew
/// put a stack's secrets, or a whole binary, into one pve journal line.
#[test]
pub(crate) fn fix_55_an_unparseable_frame_is_logged_without_its_body() {
    let secret = "hunter2-db-password";
    let frame = format!(
        r#"{{"id":7,"cmd":"deploy_stack","manifest":{{"vmid":"not-a-number"}},"env":{{"app":{{"DB_PASSWORD":"{}"}}}},"blob":"{}"}}"#,
        secret,
        "A".repeat(100_000)
    );
    let e = serde_json::from_str::<RpcRequest>(&frame).unwrap_err();
    let line = crate::unparseable_frame_line(&e, &frame);
    assert!(!line.contains(secret), "{}", line);
    assert!(line.len() < 500, "{} bytes", line.len());
    assert!(line.contains("deploy_stack"), "names the method: {}", line);
    assert!(
        line.contains(&frame.len().to_string()),
        "and the size: {}",
        line
    );
    // serde's own message quotes the offending value; that value may be a
    // secret too, so only its position is kept.
    let e =
        serde_json::from_str::<RpcRequest>(r#"{"id":"s3cr3t-token","cmd":"ping"}"#).unwrap_err();
    let line = crate::unparseable_frame_line(&e, r#"{"id":"s3cr3t-token","cmd":"ping"}"#);
    assert!(!line.contains("s3cr3t-token"), "{}", line);
    // The positive twin: still a real diagnostic line, not an empty one
    // that would also pass "no secret in it".
    assert!(!line.is_empty(), "the frame still gets a log line");
}

/// covers: fix-211
///
/// (1) a mutating command carrying a field this host's own build has
/// no struct field for ("force_wipe" is not a field of
/// `set_stack_enabled`) is refused, naming that field's exact path,
/// before it ever reaches a handler.
#[test]
pub(crate) fn fix_211_a_field_the_host_does_not_know_is_refused_by_name() {
    let (value, req) = parsed(
        r#"{"id":1,"cmd":"set_stack_enabled","stack":"media","enabled":true,"force_wipe":true}"#,
    );
    let why = crate::unknown_field_refusal(&value, &req).expect("refused");
    assert!(why.contains("force_wipe"), "{}", why);
    assert!(why.contains("release-update"), "{}", why);
}

/// covers: fix-211
///
/// (2) a mutating command whose JSON carries only fields this build
/// knows is never refused by this check.
#[test]
pub(crate) fn fix_211_a_command_with_only_known_fields_is_never_refused() {
    let (value, req) =
        parsed(r#"{"id":1,"cmd":"set_stack_enabled","stack":"media","enabled":true}"#);
    assert!(crate::unknown_field_refusal(&value, &req).is_none());
}

/// covers: fix-211
///
/// A READ-ONLY command carrying a field this build does not know is
/// never refused: it changes nothing, so an unknown field in it
/// cannot repeat the 2026-08-31 incident.
#[test]
pub(crate) fn fix_211_a_read_only_command_is_never_refused_for_an_unknown_field() {
    let (value, req) = parsed(r#"{"id":1,"cmd":"status","mystery_field":true}"#);
    assert!(req.command.is_read_only());
    assert!(crate::unknown_field_refusal(&value, &req).is_none());
}

/// fix-121 (expert panel, self-update-acceptance-weak, 2026-09-27): a new
/// daemon was accepted after five seconds alive, before anything had
/// talked to it, so a binary that ran but could not serve a client was
/// kept. It is accepted now by the first authenticated request it
/// answers.
#[tokio::test]
pub(crate) async fn fix_121_a_self_update_is_accepted_by_an_answered_request() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (state, marker) = state_with_marker("fix121-new", now - 100);
    let addr =
        serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await }).await;
    assert!(marker.exists(), "precondition: the update is pending");
    ping_over(addr).await;
    let accepted = !marker.exists();
    let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    assert!(
        accepted,
        "an answered request from the new daemon accepts the update"
    );
}

/// fix-121: the daemon that ARMED the marker answers the self-update
/// request itself; that answer must not accept the binary that replaces
/// it.
#[tokio::test]
pub(crate) async fn fix_121_the_old_daemon_never_accepts_its_successor() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (state, marker) = state_with_marker("fix121-old", now + 100);
    let addr =
        serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await }).await;
    ping_over(addr).await;
    let kept = marker.exists();
    let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    assert!(kept, "armed after this daemon started: not its own");
}

/// fix-124 (expert panel, exec-logged-verbatim, 2026-09-27): a remote
/// exec is recorded in audit.log and the journal before it runs, and was
/// recorded verbatim, so a password typed on the command line was kept
/// on pve indefinitely. The record still names the vmid and the command,
/// with the values the shared masker recognises taken out.
#[test]
pub(crate) fn fix_124_an_exec_command_is_masked_before_it_is_recorded() {
    let line = exec_audit_line(
        1_800_000_000,
        108,
        "PGPASSWORD=hunter2-x9 psql -h db -c 'select 1'; curl -H 'Authorization: Bearer tk-77' x",
    );
    assert!(
        !line.contains("hunter2-x9") && !line.contains("tk-77"),
        "{}",
        line
    );
    assert!(
        line.starts_with("1800000000 exec vmid=108 ") && line.contains("psql -h db"),
        "the record still says what ran where: {}",
        line
    );
    assert!(
        line.ends_with('\n') && line.lines().count() == 1,
        "{:?}",
        line
    );
}

/// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27):
/// audit.log (every exec command) and the incident bundles were readable
/// by any local account on pve. A new audit.log is created 0600, and at
/// start the daemon takes group and world access off what exists.
#[test]
pub(crate) fn fix_125_the_audit_log_and_existing_bundles_are_made_private() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let set = |p: &std::path::Path, m: u32| {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
    };
    let dir = std::env::temp_dir().join(format!("homelab-fix125-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("incidents/1800000000-deploy-x");
    std::fs::create_dir_all(&bundle).unwrap();
    for (f, m) in [("report.json", 0o644), ("commands.sh", 0o755)] {
        std::fs::write(bundle.join(f), "x").unwrap();
        set(&bundle.join(f), m);
    }
    set(&dir.join("incidents"), 0o755);
    set(&bundle, 0o755);
    std::fs::write(dir.join("audit.log"), "old\n").unwrap();
    set(&dir.join("audit.log"), 0o644);

    tighten_private_paths(&dir.to_string_lossy());
    let fresh = dir.join("fresh-audit.log");
    append_audit(&fresh.to_string_lossy(), "1 exec vmid=108 cmd=\"ls\"\n").unwrap();

    let got = [
        mode(&dir.join("incidents")),
        mode(&bundle),
        mode(&bundle.join("report.json")),
        mode(&bundle.join("commands.sh")),
        mode(&dir.join("audit.log")),
        mode(&fresh),
    ];
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, [0o700, 0o700, 0o600, 0o700, 0o600, 0o600]);
}

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
