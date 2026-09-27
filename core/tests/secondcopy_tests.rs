//! fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): every
//! repository lived only on Google Drive and nothing ever ran `restic check`.
//! Kenny chose a second repository set on the ZFS pool HDD4TB (which the
//! nightly ZFS replication also carries to HDD18TB), a rotating nightly
//! `restic check` of both copies, and a monthly `--read-data-subset`.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::manifest::*;
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::fleetcheck::Severity;
use homelab_core::ops::secondcopy::{
    check_repo, copy_all, data_subset, evaluate_copies, parse_dataset, pick, repo_policies,
    Dataset, RepoPolicy, DATA_SUBSETS,
};
use homelab_core::ops::OpCtx;
use homelab_core::retention::RetentionTier;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::{CopyRecord, HostState, IntegrityRecord, StackState};

const NOW: u64 = 1_790_000_000;
const DAY: u64 = 86_400;
const DS: &str = "HDD4TB/restic";

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    }
}

fn mount(path: &str, app: Option<&str>) -> MountSpec {
    MountSpec {
        host_path: path.into(),
        mount_point: path.into(),
        no_data: false,
        no_backup: None,
        host_owner_uid: None,
        app: app.map(str::to_string),
    }
}

fn manifest(
    stack: &str,
    storage: Vec<MountSpec>,
    retention: Option<Vec<RetentionTier>>,
) -> StackManifest {
    StackManifest {
        firewall: None,
        registry_login: None,
        retention,
        data_mounts: Vec::new(),
        native_only: false,
        syslog_receivers: vec![],
        natives: Vec::new(),
        stack_name: stack.into(),
        vmid: 110,
        hostname: format!("110-app-{}", stack),
        network: NetworkSpec {
            ip: "10.10.10.10/24".into(),
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
            template: "debian-12".into(),
            unprivileged: true,
            features: "nesting=1".into(),
            protection: true,
            gpu: false,
            vpn: false,
        },
        boot: BootSpec {
            onboot: true,
            order: Some(50),
        },
        storage,
        apps: vec!["app".into()],
    }
}

fn stack_state(
    manifest: Option<StackManifest>,
    natives: Vec<&str>,
    last_backup: u64,
) -> StackState {
    StackState {
        applied_source: None,
        extra_route_files: Vec::new(),
        vmid: 110,
        hostname: "110-app-x".into(),
        apps: vec![],
        applied_at: NOW,
        last_backup,
        applied_hash: String::new(),
        manifest,

        natives: natives
            .into_iter()
            .map(|u| NativeServiceManifest {
                stack_name: "kyu".into(),
                vmid: 109,
                hostname: "109-app-kyu".into(),
                unit: u.into(),
                binary: format!("/opt/{}/bin/{}", u, u),
                env_file: None,
                data_dirs: vec![format!("/appdata/kyu/{}-config", u)],
                update_cmd: None,
                stateless: false,
                release_repo: None,
                release_asset: None,
                backup_from_newest: None,
                backup_pause: false,
                update_policy: Default::default(),
                metrics: None,
            })
            .collect(),
        enabled: true,
        incomplete_step: None,
        route_file: None,
    }
}

fn fleet() -> Vec<RetentionTier> {
    homelab_core::retention::default_tiers()
}

fn own_tiers() -> Vec<RetentionTier> {
    vec![RetentionTier {
        every_days: 1,
        span_days: None,
    }]
}

fn state_json(exec: &MockExecutor) -> HostState {
    serde_json::from_str(
        &exec
            .file("/var/lib/homelab/state.json")
            .expect("state was written"),
    )
    .unwrap()
}

// ── which repositories, under which retention ──────────────────────────────

#[test]
fn every_repository_is_copied_under_the_retention_its_source_keeps() {
    let mut st = HostState {
        last_host_meta: NOW - DAY,
        ..Default::default()
    };
    st.stacks.insert(
        "paperwork".into(),
        stack_state(
            Some(manifest(
                "paperwork",
                vec![
                    mount("/appdata/paperwork/paperless-config", Some("paperless")),
                    mount("/appdata/paperwork/actual-config", Some("actual")),
                ],
                Some(own_tiers()),
            )),
            vec![],
            NOW - DAY,
        ),
    );
    st.stacks.insert(
        "kyu".into(),
        stack_state(None, vec!["kyu", "kyu-runner"], NOW - DAY),
    );
    // Never backed up: its repository does not exist yet, so there is
    // nothing to copy and a copy attempt would only report a false failure.
    st.stacks.insert(
        "fresh".into(),
        stack_state(
            Some(manifest(
                "fresh",
                vec![mount("/appdata/fresh/x", None)],
                None,
            )),
            vec![],
            0,
        ),
    );
    let p = repo_policies(&st, &["opnsense".to_string()], &fleet());
    let names: Vec<&str> = p.iter().map(|r| r.repo.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "actual",
            "host-meta",
            "kyu",
            "kyu-runner",
            "opnsense",
            "paperless"
        ]
    );
    let tiers_of = |n: &str| p.iter().find(|r| r.repo == n).unwrap().tiers.clone();
    assert_eq!(
        tiers_of("paperless"),
        Some(own_tiers()),
        "W2: the stack's own policy"
    );
    assert_eq!(tiers_of("kyu"), Some(fleet()));
    // fix-111: host-meta is pruned by the fleet tiers at source, so its copy is.
    assert_eq!(tiers_of("host-meta"), Some(fleet()));
    // A device configuration is never pruned at source, so the copy must not
    // be either: pruning only the copy would re-copy the same snapshots nightly.
    assert_eq!(tiers_of("opnsense"), None);
}

// ── the dataset ─────────────────────────────────────────────────────────────

#[test]
fn the_dataset_answer_is_read_exactly() {
    assert_eq!(
        parse_dataset("yes\t/HDD4TB/restic\n", 0),
        Dataset::Mounted("/HDD4TB/restic".into())
    );
    assert_eq!(
        parse_dataset("no\t/HDD4TB/restic\n", 0),
        Dataset::NotMounted
    );
    assert_eq!(parse_dataset("yes\tlegacy\n", 0), Dataset::NotMounted);
    assert_eq!(parse_dataset("", 1), Dataset::Missing);
}

#[tokio::test]
async fn an_unmounted_dataset_is_refused_before_anything_is_written() {
    // A repository written into the empty mountpoint of an unimported pool
    // lands on the NVMe it is meant to be independent of.
    let exec = MockExecutor::new();
    exec.respond_always("zfs list", CmdOutput::ok("no\t/HDD4TB/restic\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let repos = vec![RepoPolicy {
        repo: "kyu".into(),
        tiers: None,
    }];
    let r = copy_all(&ctx(&exec, &sink, &j), &BackupCfg::default(), DS, &repos).await;
    assert!(!r.ok);
    assert!(
        exec.calls_containing("restic init").is_empty()
            && exec.calls_containing("restic copy").is_empty(),
        "{:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn a_missing_dataset_is_created_as_the_configuration_declares() {
    let exec = MockExecutor::new();
    exec.enqueue("zfs list", CmdOutput::failed(1, "dataset does not exist"));
    exec.respond_always("zfs list", CmdOutput::ok("yes\t/HDD4TB/restic\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = copy_all(&ctx(&exec, &sink, &j), &BackupCfg::default(), DS, &[]).await;
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(exec.calls_containing("zfs create HDD4TB/restic").len(), 1);
}

// ── the copy ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn each_repository_is_copied_from_drive_into_its_twin_on_the_pool() {
    let exec = MockExecutor::new();
    exec.respond_always("zfs list", CmdOutput::ok("yes\t/HDD4TB/restic\n"));
    // Two snapshots on the same day: one of them is due to be forgotten.
    exec.respond_always(
        "snapshots --json",
        CmdOutput::ok(
            r#"[{"short_id":"aaaa1111","time":"2026-09-20T02:00:00Z"},
                {"short_id":"bbbb2222","time":"2026-09-20T03:00:00Z"}]"#,
        ),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let repos = vec![RepoPolicy {
        repo: "kyu".into(),
        tiers: Some(fleet()),
    }];
    let r = copy_all(&ctx(&exec, &sink, &j), &BackupCfg::default(), DS, &repos).await;
    assert!(r.ok, "{:?}", r.error);
    let init = exec.calls_containing("restic init");
    assert_eq!(init.len(), 1);
    assert!(
        init[0].contains("RESTIC_REPOSITORY=/HDD4TB/restic/kyu-config"),
        "{}",
        init[0]
    );
    assert!(
        init[0].contains("--from-repo rclone:gdrive:homelab-backups/kyu-config")
            && init[0].contains("--copy-chunker-params"),
        "the same chunker, so the copy deduplicates like its source: {}",
        init[0]
    );
    let copy = exec.calls_containing("restic copy");
    assert_eq!(copy.len(), 1);
    assert!(copy[0].contains("RESTIC_REPOSITORY=/HDD4TB/restic/kyu-config"));
    assert!(copy[0].contains("--from-repo rclone:gdrive:homelab-backups/kyu-config"));
    assert!(
        copy[0].contains("--from-password-file /var/lib/homelab/secrets/restic.pw"),
        "the same password file opens both: {}",
        copy[0]
    );
    let forget = exec.calls_containing("restic forget");
    assert_eq!(forget.len(), 1);
    assert!(forget[0].contains("RESTIC_REPOSITORY=/HDD4TB/restic/kyu-config"));
    assert!(forget[0].contains("aaaa1111") && !forget[0].contains("bbbb2222"));
    let st = state_json(&exec);
    assert_eq!(st.last_second_copy, NOW);
    assert_eq!(
        st.second_copies.get("kyu"),
        Some(&CopyRecord {
            last_attempt: NOW,
            last_ok: NOW,
            last_error: None
        })
    );
}

#[tokio::test]
async fn one_failing_repository_does_not_stop_the_others_and_is_remembered() {
    let exec = MockExecutor::new();
    exec.respond_always("zfs list", CmdOutput::ok("yes\t/HDD4TB/restic\n"));
    exec.respond_always(
        "restic copy --from-repo rclone:gdrive:homelab-backups/actual-config",
        CmdOutput::failed(1, "Fatal: unable to open repository"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let repos = vec![
        RepoPolicy {
            repo: "actual".into(),
            tiers: None,
        },
        RepoPolicy {
            repo: "kyu".into(),
            tiers: None,
        },
    ];
    let r = copy_all(&ctx(&exec, &sink, &j), &BackupCfg::default(), DS, &repos).await;
    assert!(
        !r.ok,
        "a failed copy fails the operation, so it is reported"
    );
    assert!(
        exec.calls_containing("RESTIC_REPOSITORY=/HDD4TB/restic/kyu-config")
            .iter()
            .any(|c| c.contains("restic copy")),
        "kyu is still copied after actual failed"
    );
    let st = state_json(&exec);
    assert!(st.second_copies["actual"]
        .last_error
        .as_deref()
        .unwrap()
        .contains("unable to open repository"));
    assert_eq!(st.second_copies["kyu"].last_ok, NOW);
}

// ── the check ───────────────────────────────────────────────────────────────

#[test]
fn the_repository_checked_longest_ago_goes_first() {
    let mut st = HostState::default();
    st.integrity.insert(
        "actual".into(),
        IntegrityRecord {
            last_check: NOW - DAY,
            ..Default::default()
        },
    );
    st.integrity.insert(
        "kyu".into(),
        IntegrityRecord {
            last_check: NOW - 5 * DAY,
            ..Default::default()
        },
    );
    let repos = vec!["actual".to_string(), "kyu".to_string(), "zzz".to_string()];
    assert_eq!(
        pick(&st, &repos).as_deref(),
        Some("zzz"),
        "never checked first"
    );
    assert_eq!(pick(&st, &repos[..2]).as_deref(), Some("kyu"));
    assert_eq!(pick(&st, &[]), None);
}

#[test]
fn data_is_read_once_a_month_per_repository_and_walks_every_slice() {
    let mut st = HostState::default();
    let month = 30 * DAY;
    assert_eq!(data_subset(&st, "kyu", NOW, month), Some((1, DATA_SUBSETS)));
    st.integrity.insert(
        "kyu".into(),
        IntegrityRecord {
            last_data_read: NOW - 10 * DAY,
            next_subset: 4,
            ..Default::default()
        },
    );
    assert_eq!(data_subset(&st, "kyu", NOW, month), None);
    assert_eq!(
        data_subset(&st, "kyu", NOW + 20 * DAY, month),
        Some((4, DATA_SUBSETS))
    );
}

#[tokio::test]
async fn a_check_covers_both_copies_and_a_failure_on_one_still_checks_the_other() {
    let exec = MockExecutor::new();
    exec.respond_always("zfs list", CmdOutput::ok("yes\t/HDD4TB/restic\n"));
    exec.respond_always(
        "RESTIC_REPOSITORY=rclone:gdrive:homelab-backups/kyu-config RESTIC_PASSWORD_FILE=/var/lib/homelab/secrets/restic.pw RESTIC_CACHE_DIR=/var/lib/homelab/restic-cache restic check",
        CmdOutput::failed(1, "Fatal: repository contains errors"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = check_repo(
        &ctx(&exec, &sink, &j),
        &BackupCfg::default(),
        "kyu",
        Some(DS),
        Some((3, DATA_SUBSETS)),
    )
    .await;
    assert!(!r.ok);
    let checks = exec.calls_containing("restic check");
    assert_eq!(checks.len(), 2, "{:?}", checks);
    assert!(checks.iter().all(|c| c.contains("--read-data-subset=3/10")));
    assert!(checks
        .iter()
        .any(|c| c.contains("RESTIC_REPOSITORY=/HDD4TB/restic/kyu-config")));
    let st = state_json(&exec);
    let rec = &st.integrity["kyu"];
    assert!(rec
        .drive_error
        .as_deref()
        .unwrap()
        .contains("contains errors"));
    assert_eq!(rec.local_error, None);
    assert_eq!(rec.last_check, NOW);
    assert_eq!(rec.last_data_read, NOW);
    assert_eq!(rec.next_subset, 4);
    assert_eq!(st.last_integrity_check, NOW);
}

// ── what the fleet check says ───────────────────────────────────────────────

#[test]
fn failures_and_silence_become_findings() {
    let mut st = HostState {
        last_second_copy: NOW - DAY,
        last_integrity_check: NOW - DAY,
        ..Default::default()
    };
    st.second_copies.insert(
        "actual".into(),
        CopyRecord {
            last_attempt: NOW - DAY,
            last_ok: NOW - 3 * DAY,
            last_error: Some("unable to open repository".into()),
        },
    );
    st.integrity.insert(
        "kyu".into(),
        IntegrityRecord {
            last_check: NOW - DAY,
            local_error: Some("pack abc is damaged".into()),
            ..Default::default()
        },
    );
    let f = evaluate_copies(&st, NOW, Some(DS), 20 * 3600);
    let broken: Vec<&str> = f
        .iter()
        .filter(|x| x.severity == Severity::Broken)
        .map(|x| x.subject.as_str())
        .collect();
    assert!(
        broken.iter().any(|s| s.contains("second copy · actual")),
        "{:?}",
        f
    );
    assert!(
        broken.iter().any(|s| s.contains("restic check · kyu")),
        "{:?}",
        f
    );
    assert!(
        broken.contains(&"second copy"),
        "no repository copied for three days: {:?}",
        f
    );
}

#[test]
fn a_healthy_pair_of_copies_is_silent_and_an_unconfigured_one_asks_nothing() {
    let mut st = HostState {
        last_second_copy: NOW - 3600,
        last_integrity_check: NOW - 3600,
        ..Default::default()
    };
    st.second_copies.insert(
        "kyu".into(),
        CopyRecord {
            last_attempt: NOW - 3600,
            last_ok: NOW - 3600,
            last_error: None,
        },
    );
    assert!(evaluate_copies(&st, NOW, Some(DS), 20 * 3600).is_empty());
    let never = HostState {
        last_integrity_check: NOW - 3600,
        ..Default::default()
    };
    assert!(evaluate_copies(&never, NOW, None, 20 * 3600).is_empty());
    let f = evaluate_copies(&HostState::default(), NOW, None, 20 * 3600);
    assert!(
        f.iter()
            .any(|x| x.subject == "restic check" && x.severity == Severity::Drift),
        "a Drive repository nobody ever checked is said: {:?}",
        f
    );
}
