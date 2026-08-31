//! M1 failure-model suite (AR13, AR14, AR16, F6). Every scenario maps to a
//! FEATURES.md / ARCHITECTURE_DECISIONS.md test scenario.

use homelab_core::doctor::{self, Health, Probes, StackProbe};
use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::incidents::{self, commands_script, interrupted_ops, RecordingSink};
use homelab_core::manifest::*;
use homelab_core::ops::{deploy::deploy, OpCtx};
use homelab_core::runner::NullJournal;
use homelab_core::sink::{NullSink, PipelineEvent, Sink, VecSink};

fn spec(vmid: u16, stack: &str) -> DeploySpec {
    DeploySpec {
        manifest: StackManifest {
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: false,
            natives: Vec::new(),
            stack_name: stack.into(),
            vmid,
            hostname: format!("{}-app-{}", vmid, stack),
            network: NetworkSpec {
                ip: format!("10.10.10.{}/24", vmid - 100),
                gateway: "10.10.10.1".into(),
                bridge: "vmbr0".into(),
                vlan: Some(10),
            },
            resources: ResourceSpec {
                cores: 1,
                memory_mb: 512,
                swap_mb: 256,
                disk_gb: 4,
                storage: "local-lvm".into(),
            },
            lxc: LxcSpec {
                template: "local:vztmpl/debian-12.tar.zst".into(),
                unprivileged: true,
                features: "nesting=1".into(),
                protection: false,
                gpu: false,
                vpn: false,
            },
            boot: BootSpec {
                onboot: true,
                order: Some(50),
            },
            storage: vec![],
            apps: vec!["app".into()],
        },
        files: vec![FileBlob {
            path: "app/docker-compose.yml".into(),
            content: "services: {}\n".into(),
            mode: None,
        }],
        env: std::collections::BTreeMap::new(),
        gateway_route: None,
    }
}

// ── AR14: a failed deploy produces a bundle whose commands.sh replays ───────

#[tokio::test]
async fn ar14_failed_deploy_writes_replayable_bundle() {
    let exec = MockExecutor::new();
    exec.respond_always("qm status", CmdOutput::failed(2, ""));
    exec.enqueue("pct config", CmdOutput::failed(2, "does not exist"));
    exec.enqueue("pct status", CmdOutput::ok("status: stopped"));
    exec.respond_always("is-system-running", CmdOutput::ok("running"));
    // Force failure at compose up.
    exec.respond_always("compose up -d", CmdOutput::failed(1, "image not found"));

    let broadcast = NullSink;
    let sink = RecordingSink::new(&broadcast);
    let journal = NullJournal;
    let ctx = OpCtx {
        exec: &exec,
        sink: &sink,
        journal: &journal,
        safety: Default::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_760_000_000,
        kea: None,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        backup: Default::default(),
        registry_cache: None,
    };
    let report = deploy(&ctx, &spec(110, "syncthing")).await;
    assert!(!report.ok);

    let dir = incidents::write_bundle(
        &exec,
        "/var/lib/homelab",
        1_760_000_000,
        &report,
        &sink.events(),
        "host=2.0.0\n",
    )
    .await
    .expect("bundle written");

    // The bundle carries report, events, versions and a replay script.
    assert!(exec.file(&format!("{}/report.json", dir)).is_some());
    assert!(exec.file(&format!("{}/events.jsonl", dir)).is_some());
    assert!(exec.file(&format!("{}/versions.txt", dir)).is_some());
    let script = exec.file(&format!("{}/commands.sh", dir)).expect("script");
    assert!(script.starts_with("#!/bin/sh"));
    // Replay contains the real commands that ran (e.g. pct create).
    assert!(script.contains("pct create 110"), "script:\n{}", script);
}

#[test]
fn ar16_commands_script_extracts_only_run_lines() {
    let events = vec![
        PipelineEvent::Line {
            level: homelab_core::sink::Level::Debug,
            source: "HOST".into(),
            msg: "[run ] pct start 110".into(),
        },
        PipelineEvent::Line {
            level: homelab_core::sink::Level::Info,
            source: "HOST".into(),
            msg: "some narrative line".into(),
        },
        PipelineEvent::Line {
            level: homelab_core::sink::Level::Debug,
            source: "HOST".into(),
            msg: "  inner output".into(),
        },
    ];
    let script = commands_script(&events);
    assert!(script.contains("pct start 110"));
    assert!(!script.contains("narrative"));
    assert!(!script.contains("inner output"));
}

// ── AR13: interrupted operations detected from the journal ──────────────────

#[test]
fn ar13_interrupted_op_detected_from_journal() {
    // deploy-media got to "compose up" then the daemon died (no done/failed).
    let journal = concat!(
        r#"{"ts":1,"op":"deploy-media","step":"validate","status":"running"}"#,
        "\n",
        r#"{"ts":2,"op":"deploy-media","step":"validate","status":"done"}"#,
        "\n",
        r#"{"ts":3,"op":"deploy-media","step":"compose up","status":"running"}"#,
        "\n",
    );
    let interrupted = interrupted_ops(journal);
    assert_eq!(
        interrupted,
        vec![("deploy-media".into(), "compose up".into())]
    );
}

#[test]
fn ar13_completed_op_is_not_flagged() {
    let journal = concat!(
        r#"{"ts":1,"op":"deploy-x","step":"validate","status":"running"}"#,
        "\n",
        r#"{"ts":2,"op":"deploy-x","step":"validate","status":"done"}"#,
        "\n",
        r#"{"ts":3,"op":"deploy-x","step":"-","status":"complete"}"#,
        "\n",
    );
    assert!(interrupted_ops(journal).is_empty());
}

// ── F6: doctor verdicts over injected probes ────────────────────────────────

#[test]
fn f6_doctor_healthy_system_is_ok() {
    let p = Probes {
        host_disk_free_pct: Some(57),
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "syncthing".into(),
            backup_age_h: Some(3),
            container_present: true,
            env_sealed: true,
        }],
        offsite_configured: true,
        offsite_token_valid: true,
        mirror_behind: Some(0),
        interrupted_ops: vec![],
    };
    let checks = doctor::diagnose(&p);
    assert_eq!(doctor::overall(&checks), Health::Ok);
}

#[test]
fn f6_doctor_flags_each_problem_with_remedy() {
    let p = Probes {
        host_disk_free_pct: Some(5), // fail
        state_parses: false,         // fail
        managed_stacks: vec![StackProbe {
            name: "media".into(),
            backup_age_h: Some(72), // warn
            container_present: true,
            env_sealed: false, // fail
        }],
        offsite_configured: true,
        offsite_token_valid: false,                   // fail
        mirror_behind: Some(2),                       // warn
        interrupted_ops: vec!["deploy-media".into()], // warn
    };
    let checks = doctor::diagnose(&p);
    assert_eq!(doctor::overall(&checks), Health::Fail);
    // Every failing/warning check carries an actionable remedy (AR7 spirit).
    for c in &checks {
        if c.health != Health::Ok {
            assert!(c.remedy.is_some(), "check '{}' has no remedy", c.name);
        }
    }
    assert!(checks
        .iter()
        .any(|c| c.name.contains("offsite") && c.health == Health::Fail));
    assert!(checks
        .iter()
        .any(|c| c.name.contains("interrupted") && c.health == Health::Warn));
}

// ── RecordingSink tees to inner sink AND records ────────────────────────────

#[tokio::test]
async fn recording_sink_tees_and_records() {
    let inner = VecSink::new();
    let rec = RecordingSink::new(&inner);
    rec.emit(PipelineEvent::Line {
        level: homelab_core::sink::Level::Info,
        source: "HOST".into(),
        msg: "hello".into(),
    });
    assert_eq!(inner.lines(), vec!["hello".to_string()]);
    assert_eq!(rec.events().len(), 1);
}
