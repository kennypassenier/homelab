//! Status, history, reveals and reads that skip the queue.

use super::support::*;
use crate::*;

/// covers fix-180: before this fix, `Rpc::BackupCalendar` read restic
/// straight on the request path, so a slow repository (restic over
/// rclone, measured live on CT 120 taking past 180 s) ran inside the
/// one serial worker (fix-66) and blocked every request queued behind
/// it — a deploy, a Live view step, anything. With the cache, a miss
/// answers "not read yet" at once and the slow read moves to a detached
/// background task, so a request sent right after it on the SAME
/// connection (and so queued behind it on the very same `work_tx` the
/// serial worker drains) finishes long before the simulated 2 s restic
/// read would have.
#[tokio::test]
pub(crate) async fn fix_180_a_cache_miss_backup_calendar_does_not_block_the_queue_behind_it() {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let addr = serve_on_loopback(cache_answering_handler).await;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let frame = |req: RpcRequest| WsMsg::Text(serde_json::to_string(&req).unwrap().into());

    let started = std::time::Instant::now();
    // Neither request opts into `reads_beside_queue` (the TUI/CLI
    // never do, fix-66), so both go through the one serial `work_tx`.
    tx.send(frame(RpcRequest {
        id: 1,
        command: Rpc::BackupCalendar {
            stacks: Vec::new(),
            force: false,
        },
    }))
    .await
    .unwrap();
    tx.send(frame(RpcRequest {
        id: 2,
        command: Rpc::Ping,
    }))
    .await
    .unwrap();

    let mut done = std::collections::HashSet::new();
    tokio::time::timeout(Duration::from_secs(1), async {
        while done.len() < 2 {
            if let ServerMsg::RpcDone(r) = serde_json::from_str::<ServerMsg>(
                &rx.next().await.unwrap().unwrap().into_text().unwrap(),
            )
            .unwrap()
            {
                done.insert(r.id);
            }
        }
    })
    .await
    .expect(
        "both the cache-miss calendar read and the request queued behind it \
         must finish in well under the simulated 2 s restic read — the queue \
         must not be blocked by it",
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "took {:?}, which is not meaningfully faster than the 2 s background read \
         it should never have waited for",
        started.elapsed()
    );
}

/// fix-217 (Kenny, Dutch, 2026-10-02: "bij kalender bv blijft die op 12
/// steken, welke doet die dan niet?"): a freshly deployed stack with a
/// real manifest, a real (non-`no_backup`) mount and NO snapshots at
/// all yet (`restic snapshots` on a never-initialised repository exits
/// 10, which `list_snapshots` already reads as `Ok(vec![])`, not an
/// error) must still reach a terminal, counted-as-read answer — "no
/// snapshots yet" — once its cache entry has been filled, the same as
/// any other stack. The first call (cold cache) answers "not read yet"
/// (fix-180's documented shape); the kicked background refresh (here,
/// against a `restic` binary this test environment does not have,
/// which fails fast rather than hanging) still produces a cache entry,
/// so the very next call already resolves with a real `measured_at`
/// and an empty `stacks["inbox"]` — never stuck pending forever.
#[tokio::test]
pub(crate) async fn fix_217_a_never_backed_up_stack_still_resolves_to_a_read_empty_answer() {
    let dir = std::env::temp_dir().join(format!("homelab-fix217-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));

    let manifest: homelab_core::manifest::StackManifest = serde_json::from_value(serde_json::json!({
        "stack_name": "inbox",
        "vmid": 150,
        "hostname": "150-app-inbox",
        "network": {"ip": "10.10.10.50/24", "gateway": "10.10.10.1", "bridge": "vmbr0"},
        "resources": {"cores": 1, "memory_mb": 512, "swap_mb": 0, "disk_gb": 4, "storage": "local-zfs"},
        "lxc": {"timezone": "host", "template": "t", "unprivileged": true, "features": "", "protection": false, "gpu": false, "vpn": false},
        "boot": {"onboot": true},
        // A real, backed-up mount — not `no_data`, not `no_backup` —
        // so this stack is exactly the "will get a real repository,
        // just never read yet" case fix-202 left untouched.
        "storage": [{"host_path": "/mnt/inbox/data", "mount_point": "/data"}],
        "apps": ["inbox"],
    }))
    .unwrap();

    let mut st = homelab_core::state::HostState::default();
    st.stacks.insert(
        "inbox".into(),
        homelab_core::state::StackState {
            applied_source: None,
            vmid: 150,
            hostname: "150-app-inbox".into(),
            apps: vec!["inbox".into()],
            applied_at: 0,
            last_backup: 0,
            applied_hash: String::new(),
            manifest: Some(manifest),
            enabled: true,
            natives: Vec::new(),
            incomplete_step: None,
            route_file: None,
            extra_route_files: Vec::new(),
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
        },
    );
    homelab_core::state::StateStore::new(&RealExecutor, &dir.to_string_lossy())
        .save(st)
        .await
        .unwrap();

    let ask = || RpcRequest {
        id: 1,
        command: Rpc::BackupCalendar {
            stacks: Vec::new(),
            force: false,
        },
    };

    let first = handle_rpc(&state, ask()).await;
    assert!(first.ok);
    let body: serde_json::Value = serde_json::from_str(&first.message).unwrap();
    assert!(
        body["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str() == Some("inbox: not read yet")),
        "a cold cache must say so, not nothing: {body}"
    );
    assert!(
        body["measured_at"]["inbox"].is_null(),
        "the first, cold-cache answer must not claim a reading yet: {body}"
    );

    // The kick fired by that first call is a detached background task
    // (`kick_snapshot_refresh`, real executor); give it a moment to run
    // and store its answer — it fails fast here (no `restic` binary in
    // this test environment), which is itself still a cache entry
    // (`SnapshotCache::refresh` caches the error too), not a hang.
    // fix-262: a fixed 300 ms was not enough under a loaded gate run (the
    // release gate of 3.70.7 failed here once); poll until the entry is
    // stored, up to 10 s, instead of guessing how long the kick takes.
    let unread = |b: &serde_json::Value| {
        b["skipped"]
            .as_array()
            .is_some_and(|a| a.iter().any(|s| s.as_str() == Some("inbox: not read yet")))
    };
    let mut second = handle_rpc(&state, ask()).await;
    for _ in 0..100 {
        let b: serde_json::Value = serde_json::from_str(&second.message).unwrap();
        if !unread(&b) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        second = handle_rpc(&state, ask()).await;
    }
    assert!(second.ok);
    let body: serde_json::Value = serde_json::from_str(&second.message).unwrap();
    assert!(
        !body["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s.as_str() == Some("inbox: not read yet")),
        "the second call must no longer call this stack unread, once its \
         cache entry exists: {body}"
    );
    assert!(
        body["measured_at"]["inbox"].is_number(),
        "a stack whose cache entry now exists must answer a real \
         measured_at, never stay null: {body}"
    );
    assert_eq!(
        body["stacks"]["inbox"],
        serde_json::json!([]),
        "zero snapshots ever is still a read, empty answer, not an absent one: {body}"
    );
}

/// feat-platform-2: an app's status from a reading, and what stands in
/// for it before the first reading.
#[test]
pub(crate) fn feat_platform_2_app_status_comes_from_the_reading() {
    use homelab_core::ops::livestatus::{AppStatus, LiveStatus};
    assert_eq!(
        live_app(None, 106, "jellyfin"),
        (true, 0),
        "no reading: the old answer"
    );
    let mut r = LiveStatus::default();
    r.apps.entry(106).or_default().insert(
        "jellyfin".into(),
        AppStatus {
            running: false,
            containers: 1,
            restarts: 4,
            health: None,
        },
    );
    assert_eq!(live_app(Some(&r), 106, "jellyfin"), (false, 4));
    assert_eq!(
        live_app(Some(&r), 106, "bazarr"),
        (false, 0),
        "probed and absent: not running"
    );
    assert_eq!(
        live_app(Some(&r), 107, "kuma"),
        (true, 0),
        "guest not probed: unknown"
    );
}

/// redesign-stackhub-2: an app's own health check goes out from the
/// reading as it was read; nothing read is None, never "healthy".
#[test]
pub(crate) fn redesign_stackhub_2_app_health_comes_from_the_reading() {
    use homelab_core::ops::livestatus::{AppStatus, LiveStatus};
    assert_eq!(live_health(None, 106, "jellyfin"), None);
    let mut r = LiveStatus::default();
    r.apps.entry(106).or_default().insert(
        "jellyfin".into(),
        AppStatus {
            running: true,
            containers: 1,
            restarts: 0,
            health: Some("unhealthy".into()),
        },
    );
    assert_eq!(
        live_health(Some(&r), 106, "jellyfin").as_deref(),
        Some("unhealthy")
    );
    assert_eq!(live_health(Some(&r), 106, "bazarr"), None);
}

/// feat-platform-3: every line an operation prints carries its request
/// and the time; step starts and ends carry a structured mark; the ring
/// keeps only the newest lines.
#[test]
pub(crate) fn feat_platform_3_lines_carry_request_time_and_step_and_the_ring_is_capped() {
    use homelab_core::sink::{Level, PipelineEvent};
    let (tx, mut rx) = broadcast::channel(16);
    let recent = Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let sink = BroadcastSink {
        log_tx: tx,
        req: Some(7),
        recent: recent.clone(),
        recent_cap: 2,
        timings: std::sync::Mutex::new(Vec::new()),
        subject: std::sync::Mutex::new(None),
        by: None,
    };
    sink.emit(PipelineEvent::StepStarted {
        op: "deploy".into(),
        step: "pull".into(),
    });
    sink.emit(PipelineEvent::Line {
        level: Level::Info,
        source: "HOST".into(),
        msg: "pulling".into(),
    });
    sink.emit(PipelineEvent::StepFinished {
        op: "deploy".into(),
        step: "pull".into(),
        changed: true,
    });
    let first = rx.try_recv().unwrap();
    match first {
        ServerMsg::Log {
            req,
            ts,
            step: Some(mark),
            ..
        } => {
            assert_eq!(req, Some(7));
            assert!(ts.unwrap_or(0) > 1_700_000_000);
            assert_eq!((mark.step.as_str(), mark.finished), ("pull", false));
        }
        other => panic!("{:?}", other),
    }
    match rx.try_recv().unwrap() {
        ServerMsg::Log { req, step, msg, .. } => {
            assert_eq!((req, step, msg.as_str()), (Some(7), None, "pulling"));
        }
        other => panic!("{:?}", other),
    }
    let ring = recent.lock().unwrap();
    assert_eq!(ring.len(), 2, "capped at recent_cap");
    assert!(matches!(&ring[1], ServerMsg::Log { step: Some(m), .. } if m.finished && m.changed));
}

/// feat-platform-3: CurrentOp answers the holder and the kept lines.
#[tokio::test]
pub(crate) async fn feat_platform_3_current_op_answers_the_holder_and_the_newest_lines() {
    let state = test_state(config_from_text("token = \"0123456789abcdef0123\"\n"));
    *state.busy.lock().unwrap() = Some(homelab_core::oplock::Holder {
        what: "deploy media".into(),
        started_unix: 1_800_000_000,
        done: 0,
        total: 0,
        stack: Some("media".into()),
    });
    state.recent.lock().unwrap().push_back(ServerMsg::Log {
        level: homelab_proto::LogLevel::Info,
        source: "HOST".into(),
        msg: "step 3".into(),
        req: Some(9),
        ts: Some(1_800_000_010),
        step: None,
        plan: None,
        by: None,
    });
    let resp = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::CurrentOp,
        },
    )
    .await;
    assert!(resp.ok);
    let view: homelab_proto::CurrentOpView = serde_json::from_str(&resp.message).unwrap();
    assert_eq!(view.holder.as_deref(), Some("deploy media"));
    assert_eq!(view.lines.len(), 1);
}

/// feat-platform-1: asked for JSON, the incident list and the manual
/// checks answer JSON; asked for text, the text stays as it was.
#[tokio::test]
pub(crate) async fn feat_platform_1_reports_answer_json_when_asked() {
    let dir = std::env::temp_dir().join(format!("homelab-json-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("incidents/1800000000-deploy-media")).unwrap();
    let state = test_state(config_from_text(&format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
        dir.display()
    )));
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 1,
            command: Rpc::Incidents { json: true },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    assert_eq!(v["incidents"][0], "1800000000-deploy-media");
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 2,
            command: Rpc::Incidents { json: false },
        },
    )
    .await;
    assert!(r.message.starts_with("incidents:"), "{}", r.message);
    let r = handle_rpc(
        &state,
        RpcRequest {
            id: 3,
            command: Rpc::ListManualChecks { json: true },
        },
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&r.message).unwrap();
    assert!(v["checks"].is_array() && v["now"].is_u64(), "{}", r.message);
}

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
