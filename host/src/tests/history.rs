//! The history file and the audit of a secret reveal.

use super::support::*;
use crate::*;

/// arch-history: the sink times each step and remembers the subject.
#[test]
pub(crate) fn arch_history_the_sink_times_steps_and_names_the_subject() {
    use homelab_core::sink::PipelineEvent;
    let (tx, _rx) = broadcast::channel(16);
    let sink = BroadcastSink {
        log_tx: tx,
        req: None,
        recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
        recent_cap: 10,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: None,
    };
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy media".into(),
        step: "pull".into(),
    });
    sink.emit(PipelineEvent::StepFinished {
        op: "deploy media".into(),
        step: "pull".into(),
        changed: true,
    });
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy media".into(),
        step: "up".into(),
    });
    let t = sink.timings.lock().unwrap().clone();
    assert_eq!(t.len(), 2);
    assert!(t[0].end >= t[0].start && t[0].end > 0 && t[0].changed);
    assert_eq!(t[1].end, 0, "a step still running has no end");
    assert_eq!(
        sink.subject.lock().unwrap().as_deref(),
        Some("deploy media")
    );
}

/// arch-history: History answers what history.jsonl holds since a moment.
#[tokio::test]
pub(crate) async fn arch_history_the_history_command_reads_since_a_moment() {
    let dir = std::env::temp_dir().join(format!("homelab-hist-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));
    for (start, name) in [(100u64, "old"), (200, "backup")] {
        record_history(
            &state,
            &homelab_core::history::HistoryEntry::Phase {
                start,
                end: start + 5,
                name: name.into(),
                count: 3,
            },
        );
    }
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::History {
                since: 150,
                until: None,
                limit: 10,
            },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    assert_eq!(v["entries"].as_array().unwrap().len(), 1, "{}", r.message);
    assert_eq!(v["entries"][0]["name"], "backup");
}

/// covers: feat-secrets-6
///
/// Kenny, 2026-10-03, "Alle drie": a
/// reveal and a copy each leave one history line — what Activity reads —
/// naming who and which secret, and neither history.jsonl nor audit.log
/// ever holds the value itself. A reveal from an old dashboard (no
/// audit) is still recorded, as a reveal.
#[tokio::test]
pub(crate) async fn redesign_371_a_reveal_is_audited_in_history_without_the_value() {
    let dir = std::env::temp_dir().join(format!("homelab-reveal-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("secrets/gateway")).unwrap();
    let value = "TUNNEL_TOKEN=never-in-a-log-7f3a";
    std::fs::write(dir.join("secrets/gateway/traefik.env"), value).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));
    let secret = homelab_proto::SecretRef::Env {
        app: "traefik".into(),
    };
    for (id, audit) in [
        (
            3,
            Some(homelab_proto::RevealAudit {
                purpose: homelab_proto::RevealPurpose::Reveal,
                by: "Kenny".into(),
            }),
        ),
        (
            4,
            Some(homelab_proto::RevealAudit {
                purpose: homelab_proto::RevealPurpose::Copy,
                by: "Claude (Live view)".into(),
            }),
        ),
        (5, None),
    ] {
        let r = handle_rpc(
            &state,
            RpcRequest {
                id,
                command: Rpc::RevealSecret {
                    stack: "gateway".into(),
                    secret: secret.clone(),
                    audit,
                },
            },
        )
        .await;
        assert!(r.ok, "{}", r.message);
        assert_eq!(r.message, value, "the reveal itself still answers");
    }
    let history = std::fs::read_to_string(dir.join("history.jsonl")).unwrap_or_default();
    let audit = std::fs::read_to_string(dir.join("audit.log")).unwrap_or_default();
    assert!(!history.contains("never-in-a-log"), "{history}");
    assert!(!audit.contains("never-in-a-log"), "{audit}");
    let entries = homelab_core::history::parse(&history);
    let said: Vec<(String, Option<String>, Option<String>)> = entries
        .iter()
        .filter_map(|e| match e {
            homelab_core::history::HistoryEntry::Op {
                label, subject, by, ..
            } => Some((label.clone(), subject.clone(), by.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        said,
        vec![
            (
                "reveal-secret".into(),
                Some("revealed gateway/traefik/.env".into()),
                Some("Kenny".into())
            ),
            (
                "copy-secret".into(),
                Some("copied gateway/traefik/.env".into()),
                Some("Claude (Live view)".into())
            ),
            (
                "reveal-secret".into(),
                Some("revealed gateway/traefik/.env".into()),
                None
            ),
        ],
        "{history}"
    );
    assert!(audit.contains("copy-secret stack=gateway"), "{audit}");
    let _ = std::fs::remove_dir_all(&dir);
}
