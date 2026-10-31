//! The UI relay and the token name that reaches every operation.

use super::support::*;
use crate::*;

/// milestone act: the token name reaches the handler of a queued
/// request and of a read beside the queue alike; outside a session
/// there is none.
#[tokio::test]
pub(crate) async fn act_the_token_name_reaches_the_operation_queued_or_beside() {
    let (state, _dir) = scoped_state("by");
    let addr = serve_state_on_loopback(state, by_handler).await;
    let replies = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![
            (
                1,
                Rpc::SessionOptions {
                    reads_beside_queue: true,
                },
            ),
            (2, Rpc::BackupDevices),
            (3, Rpc::GetState),
        ],
    )
    .await;
    for id in [2, 3] {
        let r = replies.iter().find(|r| r.0 == id).expect("a reply");
        assert_eq!(r.2, "dash-operate", "{:?}", replies);
    }
    assert_eq!(requested_by(), None);
}

/// feat-platform-10 (milestone follow).
///
/// Over real sockets: the dashboard's session attaches; a `homelab ui`
/// step from another token reaches it with that token's name and scope,
/// and the dashboard's answer is the CLI's reply. A read token may read
/// the state but not drive; a step never waits behind the queue.
#[tokio::test]
pub(crate) async fn follow_ui_steps_are_relayed_to_the_attached_dashboard_by_scope() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    let (state, _dir) = scoped_state("ui");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    // The dashboard: attaches, then answers every step with what it saw.
    let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        "Bearer all-token-cccccccccccccccccc".parse().unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    let (mut tx, mut rx) = ws.split();
    let attach = RpcRequest {
        id: 1,
        command: Rpc::UiAttach,
    };
    tx.send(WsMsg::Text(serde_json::to_string(&attach).unwrap().into()))
        .await
        .unwrap();
    let (attached_tx, attached_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let mut attached_tx = Some(attached_tx);
        let mut next = 100;
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            match serde_json::from_str::<ServerMsg>(&t) {
                Ok(ServerMsg::RpcDone(r)) if r.id == 1 => {
                    if let Some(a) = attached_tx.take() {
                        let _ = a.send(());
                    }
                }
                Ok(ServerMsg::Ui {
                    relay,
                    by,
                    scope,
                    step,
                    ..
                }) => {
                    next += 1;
                    let reply = RpcRequest {
                        id: next,
                        command: Rpc::UiReply {
                            relay,
                            ok: true,
                            message: format!("{} {:?} {}", by, scope, step.verb()),
                        },
                    };
                    tx.send(WsMsg::Text(serde_json::to_string(&reply).unwrap().into()))
                        .await
                        .unwrap();
                }
                _ => {}
            }
        }
    });
    attached_rx.await.unwrap();
    let replies = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![
            // A deploy-like command first: the step must not wait for it.
            (1, Rpc::BackupDevices),
            (
                2,
                Rpc::Ui {
                    step: homelab_proto::UiStep::Close,
                    client_version: "3.70.0".into(),
                },
            ),
        ],
    )
    .await;
    assert_eq!(replies.first().map(|r| r.0), Some(2), "{:?}", replies);
    assert_eq!(replies[0].2, "dash-operate Operate close");
    let read = session_replies(
        addr,
        "read-token-aaaaaaaaaaaaaaaa",
        vec![
            (
                1,
                Rpc::Ui {
                    step: homelab_proto::UiStep::State,
                    client_version: "3.70.0".into(),
                },
            ),
            (
                2,
                Rpc::Ui {
                    step: homelab_proto::UiStep::Close,
                    client_version: "3.70.0".into(),
                },
            ),
        ],
    )
    .await;
    let state = read.iter().find(|r| r.0 == 1).unwrap();
    assert!(state.1 && state.2 == "dash-read Read state", "{:?}", read);
    let close = read.iter().find(|r| r.0 == 2).unwrap();
    assert!(
        !close.1 && close.2.contains("needs scope Operate"),
        "{:?}",
        read
    );
    // A session that is not the dashboard cannot answer for it.
    let forged = session_replies(
        addr,
        "operate-token-bbbbbbbbbbbbbb",
        vec![(
            1,
            Rpc::UiReply {
                relay: 1,
                ok: true,
                message: "forged".into(),
            },
        )],
    )
    .await;
    assert!(!forged[0].1, "{:?}", forged);
}

/// milestone act: every line of an operation carries who asked for it.
#[test]
pub(crate) fn act_the_sink_stamps_the_token_name_on_every_line() {
    use homelab_core::sink::PipelineEvent;
    let (tx, mut rx) = broadcast::channel(16);
    let sink = BroadcastSink {
        log_tx: tx,
        req: Some(3),
        recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        recent_cap: 10,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: Some("admin".into()),
    };
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy-media".into(),
        step: "pull".into(),
    });
    sink.emit(PipelineEvent::Line {
        level: homelab_core::sink::Level::Info,
        source: "HOST".into(),
        msg: "pulled".into(),
    });
    for _ in 0..2 {
        match rx.try_recv().unwrap() {
            ServerMsg::Log { by, req, .. } => {
                assert_eq!((by.as_deref(), req), (Some("admin"), Some(3)))
            }
            other => panic!("{:?}", other),
        }
    }
}
