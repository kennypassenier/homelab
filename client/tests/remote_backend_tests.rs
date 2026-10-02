#![cfg(feature = "tui")]
//! The TUI's real connection (`RemoteBackend`) against a fake host on a
//! loopback port: real TLS, real WebSocket, a certificate made for the test.
//!
//! The pin lives under `$HOME`, so this file points HOME at a directory of
//! its own before anything connects. Every test here presents the same
//! certificate, so whichever test pins it first pins it for all of them. The
//! tests run one at a time: the repository-pin test starts from a machine
//! with no pin and would otherwise hand its foreign pin to the others.

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

use homelab_client::tui::backend::{Backend, BackendEvent, Channels, RemoteBackend};
use homelab_proto::{Command, RpcRequest, RpcResponse, ServerMsg};

struct TestCert {
    cert: rustls::pki_types::CertificateDer<'static>,
    key_der: Vec<u8>,
}

fn test_cert() -> &'static TestCert {
    static CERT: OnceLock<TestCert> = OnceLock::new();
    CERT.get_or_init(|| {
        let home =
            std::env::temp_dir().join(format!("homelab-remote-backend-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        // SAFETY: only reachable through fake_host(), which holds SERIAL for
        // the whole test (see the file comment); no other thread touches the
        // environment while this runs.
        #[allow(unsafe_code)]
        unsafe {
            std::env::set_var("HOME", &home)
        };
        let mut params = rcgen::CertificateParams::new(vec!["homelab-host".to_string()]).unwrap();
        params.distinguished_name = rcgen::DistinguishedName::new();
        let key = rcgen::KeyPair::generate().unwrap();
        let cert = params.self_signed(&key).unwrap();
        TestCert {
            cert: rustls::pki_types::CertificateDer::from(cert.der().to_vec()),
            key_der: key.serialize_der(),
        }
    })
}

/// The far end of one connection: what the client sent, and a way to send it
/// frames.
struct FakeHost {
    addr: String,
    received: mpsc::UnboundedReceiver<String>,
    push: mpsc::UnboundedSender<String>,
    /// Held for the whole test; see the file comment.
    _serial: tokio::sync::MutexGuard<'static, ()>,
}

static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn pin_file() -> std::path::PathBuf {
    homelab_client::pin_path()
}

/// A host that says `Hello { version }` and then relays frames both ways.
async fn fake_host(version: &str) -> FakeHost {
    let serial = SERIAL.lock().await;
    let tc = test_cert();
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let key = rustls::pki_types::PrivateKeyDer::try_from(tc.key_der.clone()).unwrap();
    let server_cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![tc.cert.clone()], key)
        .unwrap();
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_cfg));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (rec_tx, received) = mpsc::unbounded_channel::<String>();
    let (push, mut push_rx) = mpsc::unbounded_channel::<String>();
    let hello = serde_json::to_string(&ServerMsg::Hello {
        version: version.to_string(),
        proto: homelab_proto::PROTO_VERSION,
        build: None,
    })
    .unwrap();
    tokio::spawn(async move {
        let Ok((tcp, _)) = listener.accept().await else {
            return;
        };
        let Ok(tls) = acceptor.accept(tcp).await else {
            return;
        };
        let cfg = tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default()
            .max_message_size(None)
            .max_frame_size(None);
        let Ok(ws) = tokio_tungstenite::accept_async_with_config(tls, Some(cfg)).await else {
            return;
        };
        let (mut tx, mut rx) = ws.split();
        if tx.send(Message::Text(hello.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                msg = rx.next() => match msg {
                    Some(Ok(Message::Text(t))) => { let _ = rec_tx.send(t.to_string()); }
                    Some(Ok(_)) => {}
                    _ => break,
                },
                out = push_rx.recv() => match out {
                    Some(t) => { if tx.send(Message::Text(t.into())).await.is_err() { break; } }
                    None => break,
                },
            }
        }
    });
    FakeHost {
        addr,
        received,
        push,
        _serial: serial,
    }
}

fn start(addr: &str) -> Channels {
    start_with_repo_pin(addr, None)
}

fn start_with_repo_pin(addr: &str, repo_pin: Option<&str>) -> Channels {
    Box::new(RemoteBackend {
        host: addr.to_string(),
        token: "0123456789abcdef0123".into(),
        repo_pin: repo_pin.map(str::to_string),
        // The loopback host presents a certificate made for the test; the
        // fleet's built-in pin is not this one (fix-149 tests it on its own).
        built_in_pin: None,
    })
    .start()
}

/// Events until nothing arrives for `quiet`.
async fn drain(evt_rx: &mut mpsc::Receiver<BackendEvent>, quiet: Duration) -> Vec<BackendEvent> {
    let mut out = Vec::new();
    while let Ok(Some(ev)) = tokio::time::timeout(quiet, evt_rx.recv()).await {
        out.push(ev);
    }
    out
}

/// covers: fix-66
///
/// Once the host reads an answer while the deploy that asked is still
/// running (host-questions-unanswerable, 2026-09-27), the reply to that
/// answer arrives BEFORE the deploy's own reply. The TUI tells replies apart
/// by order, not by id, so an `RpcDone` for the answer would close the
/// deploy's progress window as if the deploy had finished. The backend knows
/// which request was an answer and turns its reply into a log line.
#[tokio::test]
async fn fix_66_the_reply_to_an_answer_is_a_log_line_not_the_end_of_the_operation() {
    // fix-211: this test is about the answer/reply bookkeeping, not about
    // version skew — pinned to a host version at the unknown-field-check
    // capability so it never starts exercising that guard by accident
    // merely because this crate has not yet been bumped to that release.
    let mut host = fake_host(homelab_client::link::UNKNOWN_FIELD_CHECK_SINCE).await;
    let Channels { cmd_tx, mut evt_rx } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    cmd_tx
        .send(Command::Answer {
            id: 7,
            allow: true,
            boot: None,
        })
        .await
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(5), host.received.recv())
        .await
        .expect("the answer never reached the host")
        .unwrap();
    let req: RpcRequest = serde_json::from_str(&frame).expect("the host must be able to parse it");
    assert!(matches!(
        req.command,
        Command::Answer {
            id: 7,
            allow: true,
            ..
        }
    ));
    host.push
        .send(
            serde_json::to_string(&ServerMsg::RpcDone(RpcResponse {
                id: req.id,
                ok: true,
                message: "answer delivered to question 7".into(),
                deferred: None,
            }))
            .unwrap(),
        )
        .unwrap();

    let events = drain(&mut evt_rx, Duration::from_millis(800)).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BackendEvent::Server(ServerMsg::RpcDone(_)))),
        "the reply to an answer reached the TUI as an RpcDone, which closes the running \
         deploy's window as finished: {:?}",
        events
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            BackendEvent::Server(ServerMsg::Log { msg, .. }) if msg.contains("answer delivered")
        )),
        "the reply to an answer should still be visible, as a log line: {:?}",
        events
    );
}

/// covers: fix-67
///
/// The command line refuses to send a mutating command to a host older than
/// itself: serde drops a field the host does not know, and on 2026-08-31 a
/// deploy through a host one release behind lost `data_mounts` and sent 73
/// torrents to `missingFiles`. The TUI can deploy too, and its connection had
/// none of that guard (tui-connection-skips-guards, 2026-09-27).
#[tokio::test]
async fn fix_67_the_tui_refuses_a_mutating_command_to_an_older_host() {
    let mut host = fake_host("1.0.0").await;
    let Channels { cmd_tx, mut evt_rx } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    cmd_tx.send(Command::PatchFleet).await.unwrap();
    let reached = tokio::time::timeout(Duration::from_millis(1000), host.received.recv()).await;
    assert!(
        reached.is_err(),
        "a mutating command reached a host older than this client: {:?}",
        reached
    );
    let events = drain(&mut evt_rx, Duration::from_millis(500)).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            BackendEvent::Server(ServerMsg::RpcDone(r)) if !r.ok && r.message.contains("release-update")
        )),
        "the refusal must reach the screen and name the remedy: {:?}",
        events
    );
}

/// covers: fix-67
///
/// A machine with no pin of its own takes the fingerprint the repository
/// names instead of trusting whatever answers first. The command line did;
/// the TUI trusted the first certificate and saved it.
#[tokio::test]
async fn fix_67_the_tui_holds_the_host_to_the_repository_pin() {
    let host = fake_host(env!("CARGO_PKG_VERSION")).await;
    // A fresh machine: nothing pinned yet.
    let _ = std::fs::remove_file(pin_file());
    let other = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
    let Channels {
        cmd_tx: _cmd,
        mut evt_rx,
    } = start_with_repo_pin(&host.addr, Some(other));
    let events = drain(&mut evt_rx, Duration::from_millis(1500)).await;
    // Since fix-149 a refused host leaves no pin behind; removed anyway so the
    // next test can never inherit one.
    let _ = std::fs::remove_file(pin_file());
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BackendEvent::Connected { .. })),
        "the TUI connected to a host whose certificate is not the one the repository names: {:?}",
        events
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, BackendEvent::Disconnected(_))),
        "the refusal must be visible: {:?}",
        events
    );
}

/// covers: fix-67
///
/// fix-30 raised the link's message ceiling past tungstenite's 16 MiB frame
/// default, because a larger event from the host closed the link in the
/// middle of an operation. The TUI's connection kept the default.
#[tokio::test]
async fn fix_67_the_tui_reads_a_message_larger_than_sixteen_mib() {
    let host = fake_host(env!("CARGO_PKG_VERSION")).await;
    let Channels {
        cmd_tx: _cmd,
        mut evt_rx,
    } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    let big = "x".repeat(20 * 1024 * 1024);
    host.push
        .send(
            serde_json::to_string(&ServerMsg::Log {
                req: None,
                ts: None,
                by: None,
                step: None,
                plan: None,
                level: homelab_proto::LogLevel::Info,
                source: "HOST".into(),
                msg: big,
            })
            .unwrap(),
        )
        .unwrap();
    let events = drain(&mut evt_rx, Duration::from_millis(3000)).await;
    let seen: Vec<String> = events
        .iter()
        .map(|e| match e {
            BackendEvent::Server(ServerMsg::Log { msg, .. }) => {
                format!("Log({} bytes)", msg.len())
            }
            other => format!("{:?}", other),
        })
        .collect();
    assert!(
        seen.contains(&format!("Log({} bytes)", 20 * 1024 * 1024)),
        "a 20 MiB message from the host did not arrive: {:?}",
        seen
    );
}

/// covers: fix-105
///
/// older-client-no-warning (expert panel, 2026-09-27): the version gate
/// refused only a client NEWER than the host. A stale client may drop a
/// field the host now reads as "no longer declared, remove" (ask-8), the
/// mirror image of the 2026-08-31 data_mounts incident. The TUI refuses a
/// mutating command to a newer host and names `homelab self-install`.
#[tokio::test]
async fn fix_199_the_tui_no_longer_refuses_a_mutating_command_to_a_newer_host() {
    let mut host = fake_host("999.0.0").await;
    let Channels { cmd_tx, mut evt_rx } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    cmd_tx.send(Command::PatchFleet).await.unwrap();
    let reached = tokio::time::timeout(Duration::from_millis(1000), host.received.recv()).await;
    assert!(
        reached.is_ok(),
        "an older client's mutating command must now reach the host — the host's own \
         field-keeping rule makes it safe, so nothing here is refused: {:?}",
        reached
    );
    let events = drain(&mut evt_rx, Duration::from_millis(500)).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BackendEvent::Server(ServerMsg::RpcDone(r)) if !r.ok)),
        "nothing is refused on the version alone: {:?}",
        events
    );
}

/// covers: fix-149
///
/// first-connect-pin (Kenny, 2026-09-27: "Pin in de client"): a machine
/// with no pin trusted the first certificate it saw and sent the bearer
/// token to it. With a pin built into the client, a certificate that is not
/// that one is refused on the very first connection, and nothing is saved.
#[tokio::test]
async fn fix_149_a_first_connection_refuses_any_certificate_but_the_built_in_one() {
    let mut host = fake_host(env!("CARGO_PKG_VERSION")).await;
    let _ = std::fs::remove_file(pin_file());
    let other = "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99";
    let Channels {
        cmd_tx: _cmd,
        mut evt_rx,
    } = Box::new(RemoteBackend {
        host: host.addr.clone(),
        token: "0123456789abcdef0123".into(),
        repo_pin: None,
        built_in_pin: Some(other.into()),
    })
    .start();
    let events = drain(&mut evt_rx, Duration::from_millis(1500)).await;
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, BackendEvent::Connected { .. })),
        "a first connection trusted a certificate the client was not built for: {:?}",
        events
    );
    let got = tokio::time::timeout(Duration::from_millis(300), host.received.recv()).await;
    assert!(
        !matches!(got, Ok(Some(_))),
        "a frame reached a host whose certificate was refused: {:?}",
        got
    );
    assert!(
        !pin_file().exists(),
        "the refused certificate must not be pinned"
    );
}

/// covers: fix-199
///
/// Replaces fix-105's refusal: an older client no longer refuses a mutating command — the host's own
/// field-keeping rule (`core::manifest::client_knows`) makes sending safe —
/// it only ever prints a notice, for any command, mutating or not.
#[test]
fn fix_199_an_older_client_is_only_ever_told_never_refused() {
    use homelab_client::link::older_client_notice;
    let notice = older_client_notice("999.0.0").expect("a notice");
    assert!(notice.contains("self-install"), "{}", notice);
    assert!(notice.contains("kept by the host, not"), "{}", notice);
    assert!(older_client_notice(env!("CARGO_PKG_VERSION")).is_none());
    assert!(older_client_notice("0.1.0").is_none());
}

/// covers: fix-211
///
/// (3) a host that predates `UNKNOWN_FIELD_CHECK_SINCE` cannot yet tell
/// an unknown field from one its own build simply has no room for, so the
/// client still refuses a mutating command on its behalf — the same
/// guard as fix-67, now explaining why rather than only comparing
/// version numbers.
#[tokio::test]
async fn fix_211_the_tui_still_refuses_a_mutating_command_to_a_host_before_the_check() {
    let mut host = fake_host("3.70.3").await;
    let Channels { cmd_tx, mut evt_rx } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    cmd_tx.send(Command::PatchFleet).await.unwrap();
    let reached = tokio::time::timeout(Duration::from_millis(1000), host.received.recv()).await;
    assert!(
        reached.is_err(),
        "a mutating command reached a host that cannot check unknown fields itself: {:?}",
        reached
    );
    let events = drain(&mut evt_rx, Duration::from_millis(500)).await;
    assert!(
        events.iter().any(|e| matches!(
            e,
            BackendEvent::Server(ServerMsg::RpcDone(r))
                if !r.ok && r.message.contains("cannot yet tell")
        )),
        "the refusal must say WHY this host is refused, not just name a version: {:?}",
        events
    );
}

/// covers: fix-211
///
/// (2) a host AT `UNKNOWN_FIELD_CHECK_SINCE` is trusted to police its own
/// unknown fields, so the client sends the command through and relays
/// whatever the host answers.
#[tokio::test]
async fn fix_211_the_tui_sends_a_mutating_command_to_a_host_at_the_check_version() {
    let mut host = fake_host(homelab_client::link::UNKNOWN_FIELD_CHECK_SINCE).await;
    let Channels { cmd_tx, mut evt_rx } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    cmd_tx.send(Command::PatchFleet).await.unwrap();
    let reached = tokio::time::timeout(Duration::from_millis(1000), host.received.recv()).await;
    assert!(
        reached.is_ok(),
        "a mutating command with only known fields must reach a host that can police them \
         itself: {:?}",
        reached
    );
}

/// covers: fix-211
///
/// `refuse_older_host` is a capability check against
/// `UNKNOWN_FIELD_CHECK_SINCE`, never a comparison against this client's
/// own version: a client far newer than that constant still sends
/// everything to a host that has reached it, and a read-only command is
/// never refused either way.
#[test]
fn fix_211_refuse_older_host_is_a_capability_check_not_a_version_gate() {
    use homelab_client::link::{UNKNOWN_FIELD_CHECK_SINCE, refuse_older_host};
    let mutating = Command::PatchFleet;
    assert!(
        refuse_older_host(&mutating, "3.70.3").is_some(),
        "older than the capability version must still be refused"
    );
    assert!(
        refuse_older_host(&mutating, UNKNOWN_FIELD_CHECK_SINCE).is_none(),
        "at the capability version must go through"
    );
    assert!(
        refuse_older_host(&mutating, "999.0.0").is_none(),
        "well past the capability version must go through"
    );
    assert!(
        refuse_older_host(&Command::Status, "0.1.0").is_none(),
        "a read-only command is never refused"
    );
}
