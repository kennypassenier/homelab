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
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: StackManifest {
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: false,
            syslog_receivers: vec![],
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
        checks: Default::default(),
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
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
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
        host_units_drift: None,
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
        host_units_drift: None,
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

/// gap-27: with the Drive token dead the doctor said "local backups still
/// run". Every repository lives behind rclone on Google Drive, so none runs.
///
/// covers: gap-27
#[test]
fn gap_27_the_dead_drive_token_remedy_does_not_promise_local_backups() {
    let p = Probes {
        host_disk_free_pct: Some(57),
        state_parses: true,
        managed_stacks: vec![],
        offsite_configured: true,
        offsite_token_valid: false,
        mirror_behind: Some(0),
        interrupted_ops: vec![],
        host_units_drift: None,
    };
    let checks = doctor::diagnose(&p);
    let remedy = checks
        .iter()
        .find(|c| c.name.contains("offsite"))
        .and_then(|c| c.remedy.clone())
        .expect("a remedy");
    assert!(!remedy.contains("local backups still run"), "{remedy}");
    assert!(remedy.contains("no backup"), "{remedy}");
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

/// fix-57 (expert panel, error-detail-unmasked-to-phone, 2026-09-27): a
/// failed step's "why" is the command's raw output (`docker compose logs
/// --tail 20`, `journalctl -n 20`, full stderr). It reached the journal, the
/// incident report, Home Assistant's event log and the phone unmasked, and
/// uncapped: a crash-looping app printing its DSN put the password on the
/// phone.
#[test]
fn fix_57_a_failure_reason_is_masked_everywhere_and_capped_on_the_phone() {
    use homelab_core::error::{CoreError, OperatorError};
    let detail = format!(
        "rc=1 :: app-1 | DATABASE_URL=postgres://paperless:hunter22@db/paperless\n\
         app-1 | PAPERLESS_SECRET_KEY=abcd1234\n{}",
        "app-1 | still crashing\n".repeat(400)
    );
    let err = OperatorError::from_core(
        "health gate",
        &CoreError::Command {
            rendered: "pct exec 110 -- sh -c docker compose logs".into(),
            detail,
        },
    );
    assert!(!err.why.contains("hunter22"), "{}", &err.why[..300]);
    assert!(!err.why.contains("abcd1234"), "{}", &err.why[..300]);
    assert!(err.why.contains("still crashing"), "the rest is kept");

    let payload = homelab_core::notify::op_payload(
        "deploy-paperless",
        "deploy",
        false,
        Some(&format!("{} :: {}", err.what, err.why)),
        "3.59.6",
    );
    let v: serde_json::Value = serde_json::from_str(&payload).unwrap();
    let sent = v["error"].as_str().unwrap();
    assert!(sent.len() <= 1200, "{} bytes to the phone", sent.len());
    assert!(
        sent.contains("incident bundle"),
        "says where the rest is: {sent}"
    );
    // And a raw secret handed straight to the payload is masked there too.
    let payload =
        homelab_core::notify::op_payload("x", "deploy", false, Some("KYU_TOKEN=abc123"), "1");
    assert!(!payload.contains("abc123"), "{payload}");
}

/// fix-53 (expert panel, timeout-leaves-container-work-running, 2026-09-27):
/// the host's timeout ended the wait for `pct exec`, not the script inside
/// the container. A slow `docker compose pull` went on running next to the
/// prune and the second pull of the registry-cache fallback. The limit now
/// holds inside the container as well.
#[tokio::test]
async fn fix_53_a_container_script_carries_its_own_time_limit() {
    use homelab_core::executor::{pct_sh, pct_sh_secret};
    let exec = MockExecutor::new();
    pct_sh(&exec, 110, "docker compose pull", 600)
        .await
        .unwrap();
    pct_sh_secret(&exec, 110, "cat /opt/x/.env", 30)
        .await
        .unwrap();
    let calls = exec.calls();
    assert_eq!(
        calls[0],
        "pct exec 110 -- timeout -k 10 600 sh -c docker compose pull"
    );
    assert_eq!(
        calls[1],
        "pct exec 110 -- timeout -k 10 30 sh -c cat /opt/x/.env"
    );
}

// fix-38: commands.sh joined arguments with single spaces, so an argument
// holding a whole shell script came back as separate words. Replayed, the
// line `pct exec 110 -- sh -c cd '/opt/x' && docker compose up -d` runs
// `cd` inside the container and `docker compose up -d` ON THE HOST.
#[tokio::test]
async fn fix_38_a_replayed_command_keeps_its_script_argument_whole() {
    use homelab_core::executor::{Cmd, Executor, TracingExecutor};
    let exec = MockExecutor::new();
    exec.respond_always("pct exec", CmdOutput::ok(""));
    let sink = VecSink::new();
    let traced = TracingExecutor::new(&exec, &sink);
    let script = "cd '/opt/x' && docker compose up -d";
    traced
        .run(&Cmd::new(
            "pct",
            &["exec", "110", "--", "sh", "-c", script],
            30,
        ))
        .await
        .unwrap();
    let replay = commands_script(&sink.events());
    let line = replay
        .lines()
        .find(|l| l.starts_with("pct "))
        .expect("the command is in the script");
    assert_eq!(
        line,
        r#"pct exec 110 -- sh -c 'cd '\''/opt/x'\'' && docker compose up -d'"#
    );
    // And the shell reads it back as exactly those seven words.
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("set -- {}; printf '%s\\n' \"$#\" \"$7\"", line))
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("7\n{}\n", script)
    );
}
