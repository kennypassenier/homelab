//! M1 failure-model suite (AR13, AR14, AR16, F6). Every scenario maps to a
//! FEATURES.md / ARCHITECTURE_DECISIONS.md test scenario.

use homelab_core::doctor::{self, Health, Probes, StackProbe};
use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::incidents::{self, RecordingSink, commands_script, interrupted_ops};
use homelab_core::manifest::*;
use homelab_core::ops::{OpCtx, deploy::deploy};
use homelab_core::runner::NullJournal;
use homelab_core::sink::{NullSink, PipelineEvent, Sink, VecSink};

fn spec(vmid: u16, stack: &str) -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        backup_first: false,
        client_schema: homelab_core::manifest::CURRENT_CLIENT_SCHEMA,
        source: None,
        native_binaries: Default::default(),
        native_manifests: Default::default(),
        manifest: StackManifest {
            home_address_whitelist: None,
            tiles: Default::default(),
            log_files: Vec::new(),
            registry_login: None,
            retention: None,
            data_mounts: Vec::new(),
            native_only: false,
            no_apps_yet: false,
            on_demand: false,
            syslog_receivers: vec![],
            firewall: None,
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
                timezone: "host".into(),
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
        extra_routes: Vec::new(),
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
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        default_log_rotation: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
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

/// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27): every
/// bundle file was written 0644, so any local account on pve could read the
/// transcripts and replay scripts of failed operations, which carried
/// secrets until they were masked on 2026-09-27. Files are 0600 (the replay
/// script 0700), and the bundle and incidents directories are made 0700.
#[tokio::test]
async fn fix_125_an_incident_bundle_is_readable_by_root_only() {
    let exec = MockExecutor::new();
    exec.seed_file("/var/lib/homelab/state.json", "{}");
    exec.seed_file("/var/lib/homelab/journal.jsonl", "{}\n");
    let report = homelab_core::runner::OperationReport {
        op: "deploy-x".into(),
        ok: false,
        steps: vec![],
        error: None,
        deferred: None,
    };
    let dir = incidents::write_bundle(&exec, "/var/lib/homelab", 1, &report, &[], "host=x\n")
        .await
        .expect("bundle written");
    for f in [
        "report.json",
        "events.jsonl",
        "state-at-failure.json",
        "journal-tail.jsonl",
        "versions.txt",
    ] {
        assert_eq!(
            exec.file_mode(&format!("{}/{}", dir, f)),
            Some(0o600),
            "{} must be 0600",
            f
        );
    }
    assert_eq!(exec.file_mode(&format!("{}/commands.sh", dir)), Some(0o700));
    let chmods = exec.calls_containing("chmod 700");
    assert!(
        chmods
            .iter()
            .any(|c| c.contains("/var/lib/homelab/incidents ") && c.contains(&dir)),
        "the incidents directory and the bundle are made 0700: {:?}",
        exec.calls()
    );
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
        // fix-62 (2026-10-01): asked unconditionally, unlike staging disk
        // (which is only a finding when configured) — a healthy system
        // still has to answer it, or doctor reads "not asked" as "could not
        // read" and warns.
        restore_scratch_disk_free_pct: Some(57),
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "syncthing".into(),
            snapshot_age_h: Some(3),
            snapshot_read: true,
            container_present: true,
            env_sealed: true,
            nothing_to_back_up: false,
            is_native: false,
        }],
        offsite_configured: true,
        offsite_token_valid: true,
        mirror_behind: Some(0),
        interrupted_ops: vec![],
        host_units_drift: None,
        ..Default::default()
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
            snapshot_age_h: Some(72), // warn
            snapshot_read: true,
            container_present: true,
            env_sealed: false, // fail
            nothing_to_back_up: false,
            is_native: false,
        }],
        offsite_configured: true,
        offsite_token_valid: false,                   // fail
        mirror_behind: Some(2),                       // warn
        interrupted_ops: vec!["deploy-media".into()], // warn
        host_units_drift: None,
        ..Default::default()
    };
    let checks = doctor::diagnose(&p);
    assert_eq!(doctor::overall(&checks), Health::Fail);
    // Every failing/warning check carries an actionable remedy (AR7 spirit).
    for c in &checks {
        if c.health != Health::Ok {
            assert!(c.remedy.is_some(), "check '{}' has no remedy", c.name);
        }
    }
    assert!(
        checks
            .iter()
            .any(|c| c.name.contains("offsite") && c.health == Health::Fail)
    );
    assert!(
        checks
            .iter()
            .any(|c| c.name.contains("interrupted") && c.health == Health::Warn)
    );
}

/// fix-62 (restore-drill-covers-almost-nothing, 2026-10-01): the drill's
/// scratch directory is now a disk the doctor watches, the same thresholds
/// as the host disk and the backup staging directory.
#[test]
fn fix_62_the_restore_drill_scratch_disk_is_a_doctor_check() {
    for (free, want) in [(5u64, Health::Fail), (15, Health::Warn), (50, Health::Ok)] {
        let p = Probes {
            host_disk_free_pct: Some(90),
            state_parses: true,
            restore_scratch_disk_free_pct: Some(free),
            ..Default::default()
        };
        let checks = doctor::diagnose(&p);
        let line = checks
            .iter()
            .find(|c| c.name == "restore drill scratch disk")
            .expect("a restore drill scratch disk line");
        assert_eq!(line.health, want, "{}% free", free);
        if want != Health::Ok {
            assert!(line.remedy.is_some());
        }
    }
    // Unreadable is a warning with a remedy, not silence.
    let p = Probes {
        host_disk_free_pct: Some(90),
        state_parses: true,
        restore_scratch_disk_free_pct: None,
        ..Default::default()
    };
    let checks = doctor::diagnose(&p);
    let line = checks
        .iter()
        .find(|c| c.name == "restore drill scratch disk")
        .expect("a restore drill scratch disk line even when unreadable");
    assert_eq!(line.health, Health::Warn);
    assert!(line.remedy.is_some());
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
        ..Default::default()
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

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): a refused token
/// used to leave no trace. Doctor says how many were refused and from where,
/// and says "none" when there were none, so the line is always read.
#[test]
fn fix_120_doctor_names_refused_connections() {
    let quiet = Probes {
        state_parses: true,
        failed_auth: Some(doctor::FailedAuth::default()),
        ..Default::default()
    };
    let line = doctor::diagnose(&quiet)
        .into_iter()
        .find(|c| c.name == "refused connections")
        .expect("a refused-connections line");
    assert_eq!(line.health, Health::Ok);

    let probed = Probes {
        state_parses: true,
        failed_auth: Some(doctor::FailedAuth {
            count: 3,
            last_peer: Some("10.10.10.23:51234".into()),
            last_at: 1_800_000_000,
        }),
        ..Default::default()
    };
    let line = doctor::diagnose(&probed)
        .into_iter()
        .find(|c| c.name == "refused connections")
        .expect("a refused-connections line");
    assert_eq!(line.health, Health::Warn);
    assert!(
        line.detail.contains('3') && line.detail.contains("10.10.10.23"),
        "{}",
        line.detail
    );
    assert!(line.remedy.is_some());
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): doctor
/// answered backups and the Drive token only, and said Ok for a stack that
/// backs up nothing. Each new probe gets a line: an Ok that says what it saw,
/// or a warning with a remedy.
#[test]
fn fix_130_doctor_reports_exposure_files_privilege_host_meta_drill_and_space() {
    use homelab_core::doctor::{DrillProbe, DriveSpace, Exposure, Freshness, Privileged};
    const GIB: u64 = 1 << 30;
    let p = Probes {
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "registry".into(),
            snapshot_age_h: Some(12),
            snapshot_read: true,
            container_present: true,
            env_sealed: true,
            nothing_to_back_up: true,
            is_native: false,
        }],
        exposure: Some(Exposure {
            listen: "0.0.0.0:8443".into(),
            exec_enabled: true,
        }),
        loose_files: Some(vec!["/etc/homelab/host.toml (644)".into()]),
        privileged: Some(Privileged {
            vmids: vec![105, 106, 108],
            outside_policy: vec![108],
        }),
        host_meta: Some(Freshness { age_h: None }),
        restore_drill: Some(DrillProbe {
            age_h: Some(100 * 24),
            interval_h: 90 * 24,
            failing: vec!["kyu-config: the restore itself failed".into()],
        }),
        password_file_ok: Some(false),
        drive: Some(DriveSpace {
            total: 100 * GIB,
            free: 2 * GIB,
            trashed: 30 * GIB,
        }),
        ..Default::default()
    };
    let checks = doctor::diagnose(&p);
    let line = |name: &str| {
        checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("no '{}' line in {:?}", name, checks))
            .clone()
    };
    let exposure = line("daemon exposure");
    assert_eq!(exposure.health, Health::Ok);
    assert!(exposure.detail.contains("0.0.0.0:8443") && exposure.detail.contains("exec on"));
    assert_eq!(line("file modes").health, Health::Warn);
    assert!(line("file modes").detail.contains("host.toml"));
    let privileged = line("privileged containers");
    assert_eq!(privileged.health, Health::Warn);
    assert!(privileged.detail.contains("108"), "{}", privileged.detail);
    assert_eq!(line("host-meta backup").health, Health::Warn);
    let drill = line("restore drill");
    assert_eq!(drill.health, Health::Warn);
    assert!(drill.detail.contains("kyu-config"), "{}", drill.detail);
    assert_eq!(line("restic password file").health, Health::Fail);
    let drive = line("Drive space");
    assert_eq!(drive.health, Health::Fail, "{}", drive.detail);
    assert!(drive.detail.contains("trash"), "{}", drive.detail);
    let registry = line("stack registry backup");
    assert_eq!(registry.health, Health::Ok);
    assert!(
        registry.detail.contains("nothing to back up (declared)"),
        "{}",
        registry.detail
    );
    for c in &checks {
        if c.health != Health::Ok {
            assert!(c.remedy.is_some(), "check '{}' has no remedy", c.name);
        }
    }
}

/// fix-130 (second half, 2026-10-01): doctor's "gateway route files" line —
/// Ok when every file on the gateway is declared by a stack, Warn naming
/// the ones that are not. `None` (the gateway could not be listed) prints
/// no line at all, same as every other probe here.
#[test]
fn fix_130_doctor_names_gateway_route_files_no_stack_declares() {
    let clean = Probes {
        state_parses: true,
        unowned_route_files: Some(vec![]),
        ..Default::default()
    };
    let line = doctor::diagnose(&clean)
        .into_iter()
        .find(|c| c.name == "gateway route files")
        .expect("a gateway route files line");
    assert_eq!(line.health, Health::Ok);

    let dirty = Probes {
        state_parses: true,
        unowned_route_files: Some(vec!["manual-homeassistant.yml".into()]),
        ..Default::default()
    };
    let line = doctor::diagnose(&dirty)
        .into_iter()
        .find(|c| c.name == "gateway route files")
        .expect("a gateway route files line");
    assert_eq!(line.health, Health::Warn);
    assert!(line.detail.contains("manual-homeassistant.yml"));
    assert!(line.remedy.unwrap().contains("never removes"));

    let unasked = Probes {
        state_parses: true,
        unowned_route_files: None,
        ..Default::default()
    };
    assert!(
        !doctor::diagnose(&unasked)
            .iter()
            .any(|c| c.name == "gateway route files")
    );
}

/// fix-130 (second half, 2026-10-01): the contract documented on
/// `homelab_core::doctor` — a host right after its daemon's first start,
/// nothing deployed yet, never backed up, never drilled — reports at worst
/// Warn from that emptiness. The one thing that still fails hard before a
/// single stack exists is the restic password file, because without it no
/// backup can ever be written once something IS deployed.
#[test]
fn fix_130_a_fresh_host_with_nothing_deployed_never_fails_from_emptiness_alone() {
    let fresh = Probes {
        state_parses: true,
        managed_stacks: vec![],
        offsite_configured: false,
        host_units_drift: None,
        failed_auth: None,
        exposure: None,
        loose_files: Some(vec![]),
        privileged: Some(doctor::Privileged::default()),
        host_meta: Some(doctor::Freshness { age_h: None }),
        restore_drill: Some(doctor::DrillProbe {
            age_h: None,
            interval_h: 90 * 24,
            failing: vec![],
        }),
        unowned_route_files: Some(vec![]),
        password_file_ok: Some(true),
        drive: None,
        ..Default::default()
    };
    let checks = doctor::diagnose(&fresh);
    assert_eq!(doctor::overall(&checks), Health::Warn, "{:?}", checks);
    assert!(
        !checks.iter().any(|c| c.health == Health::Fail),
        "nothing deployed yet must never fail: {:?}",
        checks
    );
    // No per-stack line at all — an unasked question is never a finding.
    assert!(!checks.iter().any(|c| c.name.starts_with("stack ")));

    // The one thing doctor still fails on before anything is deployed: a
    // restic password file that is missing or empty, because the first
    // deploy's first backup depends on it.
    let unprotected = Probes {
        password_file_ok: Some(false),
        ..fresh
    };
    assert_eq!(
        doctor::overall(&doctor::diagnose(&unprotected)),
        Health::Fail
    );
}

/// fix-131 (expert panel, orchestrator-logs-only-on-pve, 2026-09-27):
/// nothing pruned the incident bundles (90 on pve that day). Bundles older
/// than the age limit go, and past the count limit the oldest go; a name
/// that is not `<unix-ts>-<op>` is not the pruner's to judge and stays.
#[test]
fn fix_131_old_and_surplus_incident_bundles_are_pruned() {
    let day = 86_400u64;
    let now = 1_800_000_000u64;
    let names: Vec<String> = vec![
        format!("{}-deploy-media", now - 100 * day),
        format!("{}-backup-kyu", now - 10 * day),
        format!("{}-deploy-gateway", now - 9 * day),
        format!("{}-update-home", now - day),
        "notes-by-hand".into(),
    ];
    let gone = incidents::bundles_to_prune(&names, now, 90, 2);
    assert_eq!(
        gone,
        vec![names[0].clone(), names[1].clone()],
        "the 100-day-old one by age, then the oldest beyond two"
    );
    assert!(incidents::bundles_to_prune(&names[1..], now, 90, 200).is_empty());
}

/// fix-131: journal.jsonl grew for good and was read whole at every start.
/// Past the limit it keeps its newest lines, and the last record of every
/// operation still marked running, so an interrupted operation is still
/// reported after the compaction.
#[test]
fn fix_131_a_compacted_journal_keeps_its_tail_and_every_interrupted_op() {
    let mut journal = String::from(
        "{\"ts\":1,\"op\":\"deploy-media\",\"step\":\"pull images\",\"status\":\"running\"}\n",
    );
    for i in 0..2000 {
        journal.push_str(&format!(
            "{{\"ts\":{},\"op\":\"backup-kyu\",\"step\":\"snapshot\",\"status\":\"complete\"}}\n",
            10 + i
        ));
    }
    assert!(incidents::compact_journal(&journal, journal.len() + 1).is_none());
    let small = incidents::compact_journal(&journal, 40_000).expect("compacted");
    assert!(small.len() <= 40_000, "{} bytes", small.len());
    assert!(
        small.ends_with("\"status\":\"complete\"}\n"),
        "the newest lines stay"
    );
    assert!(
        small
            .lines()
            .all(|l| serde_json::from_str::<serde_json::Value>(l).is_ok()),
        "whole lines only"
    );
    assert_eq!(
        interrupted_ops(&small),
        vec![("deploy-media".to_string(), "pull images".to_string())]
    );
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
    // The positive twin: the name is kept, only the value is gone — this is
    // masking, not a payload that silently dropped the whole error.
    assert!(payload.contains("KYU_TOKEN"), "{payload}");
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

// ── fix-218: doctor's backup-age line trusts the real repository ───────────
//
// Measured live on host 3.70.4, 2026-10-02: `homelab doctor` said
// "[Ok] stack inbox backup — last backup 11h ago" while `homelab snapshots
// stacks/inbox` answered "no repositories (nothing has backed up this stack
// yet)". `StackState::last_backup` only records that a backup OPERATION
// returned ok — a native stack's nightly backup loop (and the on-demand
// `backup-native` RPC) both "succeed" vacuously over zero services, with no
// restic repository ever created. The fix reads the newest snapshot from the
// same `SnapshotCache` `homelab snapshots` uses (`StackProbe::snapshot_age_h`
// / `snapshot_read`) instead of the cached timestamp.

/// A stack whose cached `last_backup`-style timestamp would have read as
/// fresh must not be reported Ok when its repository's own newest snapshot
/// is stale — this is the exact live fault (doctor said "0h ago"/"Ok" with a
/// cold trail: the snapshot cache, once read, is the only thing allowed to
/// say "fresh").
#[test]
fn fix_218_a_recorded_night_with_a_stale_real_snapshot_is_not_ok() {
    let p = Probes {
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "inbox".into(),
            // The repository's own newest snapshot is 72h old.
            snapshot_age_h: Some(72),
            snapshot_read: true,
            container_present: true,
            env_sealed: true,
            nothing_to_back_up: false,
            is_native: true,
        }],
        ..Default::default()
    };
    let line = doctor::diagnose(&p)
        .into_iter()
        .find(|c| c.name == "stack inbox backup")
        .expect("a stack inbox backup line");
    assert_eq!(line.health, Health::Warn, "{:?}", line);
    assert!(line.detail.contains("72h ago"), "{}", line.detail);
}

/// fix-218: a deployed stack that has data to back up (not
/// `backs_up_nothing`) but whose repository holds no snapshot at all gets a
/// Warn finding naming the remedy command — this is the live inbox case
/// exactly: a cache read that came back with zero snapshots.
#[test]
fn fix_218_data_with_zero_snapshots_is_warn_has_data_but_no_backup_yet() {
    let p = Probes {
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "inbox".into(),
            snapshot_age_h: None,
            snapshot_read: true,
            container_present: true,
            env_sealed: true,
            nothing_to_back_up: false,
            is_native: true,
        }],
        ..Default::default()
    };
    let line = doctor::diagnose(&p)
        .into_iter()
        .find(|c| c.name == "stack inbox backup")
        .expect("a stack inbox backup line");
    assert_eq!(line.health, Health::Warn, "{:?}", line);
    assert!(
        line.detail.contains("has data but no backup yet"),
        "{}",
        line.detail
    );
    let remedy = line.remedy.expect("a remedy");
    assert!(
        remedy.contains("backup-native inbox"),
        "native remedy should name backup-native: {}",
        remedy
    );
}

/// fix-218: the same zero-snapshot reading for a COMPOSE stack names
/// `homelab backup stacks/<name>`, not `backup-native`.
#[test]
fn fix_218_compose_stack_zero_snapshots_remedy_names_backup_stacks() {
    let p = Probes {
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "gateway".into(),
            snapshot_age_h: None,
            snapshot_read: true,
            container_present: true,
            env_sealed: true,
            nothing_to_back_up: false,
            is_native: false,
        }],
        ..Default::default()
    };
    let line = doctor::diagnose(&p)
        .into_iter()
        .find(|c| c.name == "stack gateway backup")
        .expect("a stack gateway backup line");
    assert_eq!(line.health, Health::Warn, "{:?}", line);
    let remedy = line.remedy.expect("a remedy");
    assert!(
        remedy.contains("backup stacks/gateway"),
        "compose remedy should name `backup stacks/<name>`: {}",
        remedy
    );
}

/// fix-218: a stack that declares nothing to back up never gets the
/// no-backup-yet Warn, even with zero snapshots and a cold cache — there is
/// nothing it could ever have backed up.
#[test]
fn fix_218_no_data_stack_is_never_warned_about_missing_snapshots() {
    for snapshot_read in [true, false] {
        let p = Probes {
            state_parses: true,
            managed_stacks: vec![StackProbe {
                name: "registry".into(),
                snapshot_age_h: None,
                snapshot_read,
                container_present: true,
                env_sealed: true,
                nothing_to_back_up: true,
                is_native: false,
            }],
            ..Default::default()
        };
        let line = doctor::diagnose(&p)
            .into_iter()
            .find(|c| c.name == "stack registry backup")
            .expect("a stack registry backup line");
        assert_eq!(line.health, Health::Ok, "{:?}", line);
    }
}

/// fix-218: a cold cache (never read yet, e.g. right after a host restart)
/// must not be read as either "fresh" (the old bug) or "never backed up" (a
/// false alarm before the sweep has had its first pass) — it gets its own
/// Warn, distinct from both.
#[test]
fn fix_218_cold_snapshot_cache_is_its_own_warn_not_ok_or_never_backed_up() {
    let p = Probes {
        state_parses: true,
        managed_stacks: vec![StackProbe {
            name: "almanac".into(),
            snapshot_age_h: None,
            snapshot_read: false,
            container_present: true,
            env_sealed: true,
            nothing_to_back_up: false,
            is_native: false,
        }],
        ..Default::default()
    };
    let line = doctor::diagnose(&p)
        .into_iter()
        .find(|c| c.name == "stack almanac backup")
        .expect("a stack almanac backup line");
    assert_eq!(line.health, Health::Warn, "{:?}", line);
    assert!(
        !line.detail.contains("has data but no backup yet"),
        "a cold cache must read as unknown, not as a confirmed no-backup finding: {}",
        line.detail
    );
}
