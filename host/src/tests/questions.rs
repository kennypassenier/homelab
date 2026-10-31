//! Questions asked mid-operation and answered from the same or another session.

use super::support::*;
use crate::*;

/// covers: fix-66
///
/// An operation started over a connection asks a question, and the
/// operator answers over the SAME connection, which is what the TUI does.
/// Until 2026-09-27 the session read the next frame only after the
/// running request returned, so the answer sat unread behind the deploy
/// that was waiting for it, and every question asked of a TUI-started
/// operation ended as Unattended whatever was pressed
/// (host-questions-unanswerable). Real socket, real session loop.
#[tokio::test]
pub(crate) async fn fix_66_an_answer_on_the_same_connection_reaches_the_waiting_operation() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(asking_handler).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
    tx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();
    let verdict = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                ServerMsg::Ask { id, .. } => {
                    tx.send(frame(RpcRequest {
                        id: 2,
                        command: Rpc::Answer {
                            boot: None,
                            id,
                            allow: true,
                        },
                    }))
                    .await
                    .unwrap();
                }
                ServerMsg::RpcDone(r) if r.id == 1 => return r.message,
                _ => {}
            }
        }
        "the connection closed".to_string()
    })
    .await
    .expect("the operation never finished");
    assert_eq!(
        verdict, "Allow",
        "the operator allowed over the same connection, but the operation heard: {}",
        verdict
    );
}

/// covers: fix-240
///
/// A headless run (`homelab deploy … < /dev/null`) cannot answer the
/// question its own operation raised. A SECOND session answers it by the
/// operation's subject (`homelab answer gateway allow`), the waiting
/// operation hears Allow, the asking session learns the question is
/// settled, and the question carried the host's own wait. Real socket,
/// two real sessions on one host state.
#[tokio::test]
pub(crate) async fn fix_240_another_session_answers_a_running_operations_question() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(asking_handler).await;
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
    let (headless, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect the headless session");
    let (mut htx, mut hrx) = headless.split();
    htx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();
    let (outcome, wait_s, settled) = tokio::time::timeout(Duration::from_secs(20), async {
        let mut wait_s = None;
        let mut settled = false;
        // The reply and the broadcast travel separate channels, so
        // either may arrive first: wait for both.
        let mut outcome: Option<String> = None;
        while let Some(Ok(WsMsg::Text(t))) = hrx.next().await {
            match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                // The headless session does not answer; another one does.
                ServerMsg::Ask { wait_s: w, .. } => {
                    wait_s = w;
                    let (other, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
                        .await
                        .expect("connect the answering session");
                    let (mut otx, mut orx) = other.split();
                    otx.send(frame(RpcRequest {
                        id: 7,
                        command: Rpc::AnswerOpen {
                            op: Some("gateway".into()),
                            allow: true,
                        },
                    }))
                    .await
                    .unwrap();
                    while let Some(Ok(WsMsg::Text(t))) = orx.next().await {
                        if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
                            && r.id == 7
                        {
                            assert!(r.ok, "the answer was not taken: {}", r.message);
                            break;
                        }
                    }
                }
                ServerMsg::AskSettled { .. } => settled = true,
                ServerMsg::RpcDone(r) if r.id == 1 => outcome = Some(r.message),
                _ => {}
            }
            if let (Some(o), true) = (&outcome, settled) {
                return (o.clone(), wait_s, settled);
            }
        }
        (
            outcome.unwrap_or_else(|| "the connection closed".to_string()),
            wait_s,
            settled,
        )
    })
    .await
    .expect("the operation never finished");
    assert_eq!(outcome, "Allow", "the operation heard: {}", outcome);
    assert_eq!(wait_s, Some(5), "the question must carry the host's wait");
    assert!(
        settled,
        "the asking session was not told the question is settled"
    );
}

/// covers: fix-240
///
/// Nobody answers: the question still fails its operation (silence is
/// never permission), and on the way it was raised as an urgent notice
/// in the host's notices that names the question and how to answer it.
#[tokio::test]
pub(crate) async fn fix_240_an_unanswered_question_is_a_notice_and_still_fails() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let dir = std::env::temp_dir().join(format!("fix240-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    ));
    let addr = serve_state_on_loopback(test_state(config), notifying_asking_handler).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .unwrap();
    let (mut tx, mut rx) = ws.split();
    tx.send(WsMsg::Text(
        serde_json::to_string(&RpcRequest {
            id: 1,
            command: Rpc::Ping,
        })
        .unwrap()
        .into(),
    ))
    .await
    .unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
                && r.id == 1
            {
                return r;
            }
        }
        panic!("the connection closed");
    })
    .await
    .expect("the operation never finished");
    assert!(
        !outcome.ok,
        "an unanswered question let the operation go on"
    );
    assert!(
        outcome.message.contains("Unattended"),
        "{}",
        outcome.message
    );
    let notices = std::fs::read_to_string(dir.join("notices.jsonl")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    let all = homelab_core::notify::parse_notices(&notices);
    let n = all
        .iter()
        .find(|n| n.op.contains("deploy-gateway"))
        .unwrap_or_else(|| panic!("no notice for the question: {notices}"));
    assert!(n.urgent, "{:?}", n);
    assert!(n.what.contains("routes went from 29 to 28"), "{:?}", n);
    assert!(
        n.remedy.contains("homelab answer deploy-gateway"),
        "{:?}",
        n
    );
}

/// fix-127 (expert panel, websocket-edge-cases, 2026-09-27): the session
/// read `while let Some(Ok(Message::Text(..)))`, so the first Ping,
/// Pong or Binary frame ended it. A keepalive ping from a client or a
/// proxy closed the line without a word.
#[tokio::test]
pub(crate) async fn fix_127_a_ping_or_binary_frame_does_not_end_the_session() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(|st, req| async move { handle_rpc(&st, req).await }).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    tx.send(WsMsg::Ping(vec![1, 2, 3].into())).await.unwrap();
    tx.send(WsMsg::Binary(vec![0xff; 8].into())).await.unwrap();
    let req = RpcRequest {
        id: 7,
        command: Rpc::Ping,
    };
    tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
        .await
        .unwrap();
    let answered = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(frame)) = rx.next().await {
            if let WsMsg::Text(t) = frame
                && let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
            {
                return r.id == 7 && r.ok;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    assert!(
        answered,
        "the request after a ping and a binary frame is answered"
    );
}

/// arch-host-link: an answer stamped with an earlier start of the host
/// is refused; one stamped with this start, or unstamped (CLI, TUI), is
/// delivered.
#[tokio::test]
pub(crate) async fn arch_host_link_a_stale_answer_does_not_answer_a_new_question() {
    let state = test_state(config_from_text("token = \"0123456789abcdef0123\"\n"));
    let pending = |state: &AppState| {
        let (tx, rx) = tokio::sync::oneshot::channel();
        state.pending_asks.lock().unwrap().insert(
            1,
            PendingAsk {
                reply: tx,
                ask: ServerMsg::Log {
                    level: homelab_proto::LogLevel::Info,
                    source: "HOST".into(),
                    msg: "question".into(),
                    req: None,
                    ts: None,
                    by: None,
                    step: None,
                    plan: None,
                },
            },
        );
        rx
    };
    let _rx = pending(&state);
    let stale = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::Answer {
                id: 1,
                allow: true,
                boot: Some("0-1".into()),
            },
        },
    )
    .await;
    assert!(
        !stale.ok && stale.message.contains("earlier start"),
        "{}",
        stale.message
    );
    assert!(
        state.pending_asks.lock().unwrap().contains_key(&1),
        "the question still waits"
    );
    let now = handle_rpc(
        &state,
        RpcRequest {
            id: 2,
            command: Rpc::Answer {
                id: 1,
                allow: true,
                boot: Some(state.boot_id.clone()),
            },
        },
    )
    .await;
    assert!(now.ok, "{}", now.message);
    let _rx = pending(&state);
    let plain = handle_rpc(
        &state,
        RpcRequest {
            id: 3,
            command: Rpc::Answer {
                id: 1,
                allow: false,
                boot: None,
            },
        },
    )
    .await;
    assert!(plain.ok, "{}", plain.message);
}

// ── arch-tokens and arch-host-link (homelab-admin, 2026-09-28) ──────

/// arch-host-link: without the option, replies keep the order of the
/// requests (the TUI depends on it); with it, a read overtakes a slow
/// command and is told apart by its id.
#[tokio::test]
pub(crate) async fn arch_host_link_reads_skip_the_queue_only_for_a_session_that_asks() {
    let (state, _dir) = scoped_state("beside");
    let addr = serve_state_on_loopback(state, naming_handler).await;
    let plain = session_replies(
        addr,
        "all-token-cccccccccccccccccc",
        vec![(1, Rpc::BackupDevices), (2, Rpc::GetState)],
    )
    .await;
    assert_eq!(
        plain.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![1, 2],
        "{:?}",
        plain
    );
    let asked = session_replies(
        addr,
        "all-token-cccccccccccccccccc",
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
    assert_eq!(
        asked.iter().map(|r| r.0).collect::<Vec<_>>(),
        vec![1, 3, 2],
        "{:?}",
        asked
    );
}

/// fix-127: a client that reads slower than the host writes lags the
/// broadcast channel, and the forwarder skipped what it missed without
/// a word, questions included, so the operation waited for an answer to
/// a question the operator never saw and ended Unattended. Now the
/// client is told how much it missed and every open question is sent
/// again.
#[tokio::test]
pub(crate) async fn fix_127_a_lagging_client_still_gets_the_open_question() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(flooding_asker).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());
    tx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();
    let mut told_of_the_gap = false;
    let verdict = tokio::time::timeout(Duration::from_secs(20), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            match serde_json::from_str::<ServerMsg>(&t).unwrap() {
                ServerMsg::Ask { id, .. } => {
                    tx.send(frame(RpcRequest {
                        id: 2,
                        command: Rpc::Answer {
                            boot: None,
                            id,
                            allow: true,
                        },
                    }))
                    .await
                    .unwrap();
                }
                ServerMsg::Log { msg, .. } if msg.contains("dropped") => told_of_the_gap = true,
                ServerMsg::RpcDone(r) if r.id == 1 => return r.message,
                _ => {}
            }
        }
        "the connection closed".to_string()
    })
    .await
    .expect("the operation never finished");
    assert_eq!(verdict, "Allow", "the question reached the operator");
    assert!(
        told_of_the_gap,
        "and the client was told it missed messages"
    );
}
