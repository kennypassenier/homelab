//! The TUI's real connection (`RemoteBackend`) against a fake host on a
//! loopback port: real TLS, real WebSocket, a certificate made for the test.
//!
//! The pin lives under `$HOME`, so this file points HOME at a directory of
//! its own before anything connects. Every test here presents the same
//! certificate, so whichever test pins it first pins it for all of them.

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
        std::env::set_var("HOME", &home);
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
}

/// A host that says `Hello { version }` and then relays frames both ways.
async fn fake_host(version: &str) -> FakeHost {
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
    }
}

fn start(addr: &str) -> Channels {
    Box::new(RemoteBackend {
        host: addr.to_string(),
        token: "0123456789abcdef0123".into(),
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
    let mut host = fake_host(env!("CARGO_PKG_VERSION")).await;
    let Channels { cmd_tx, mut evt_rx } = start(&host.addr);
    let _ = drain(&mut evt_rx, Duration::from_millis(500)).await;

    cmd_tx
        .send(Command::Answer { id: 7, allow: true })
        .await
        .unwrap();
    let frame = tokio::time::timeout(Duration::from_secs(5), host.received.recv())
        .await
        .expect("the answer never reached the host")
        .unwrap();
    let req: RpcRequest = serde_json::from_str(&frame).expect("the host must be able to parse it");
    assert!(matches!(
        req.command,
        Command::Answer { id: 7, allow: true }
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
