//! fix-115 (drill-includes-stateless-native, 2026-09-27): a native unit that
//! keeps nothing (`stateless: true`, the drill stack's `drillsvc`, kyu-runner
//! in the past) has no repository, yet the restore drill put it in the
//! rotation and its night failed on a repository that does not exist.

use homelab_core::executor::MockExecutor;
use homelab_core::native::{BackupPause, NativeServiceManifest};
use homelab_core::ops::OpCtx;
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::native::backup_native;
use homelab_core::ops::restoredrill::backed_up_units;
use homelab_core::ops::secondcopy::repo_policies;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::{HostState, StackState};

fn unit(name: &str, stateless: bool) -> NativeServiceManifest {
    NativeServiceManifest {
        restore_note: None,
        stack_name: "drill".into(),
        vmid: 119,
        hostname: "119-app-drill".into(),
        unit: name.into(),
        binary: format!("/opt/{}/bin/{}", name, name),
        env_file: None,
        data_dirs: if stateless {
            vec![]
        } else {
            vec![format!("/var/lib/{}", name)]
        },
        update_cmd: None,
        stateless,
        release_repo: None,
        release_asset: None,
        backup_from_newest: None,
        backup_pause: BackupPause::Off,
        update_policy: Default::default(),
        after_restore: None,
        metrics: None,
    }
}

#[test]
fn a_stateless_unit_has_no_repository_to_drill() {
    let units = vec![unit("drillsvc", true), unit("keeper", false)];
    assert_eq!(backed_up_units(&units), vec!["keeper".to_string()]);
}

#[test]
fn a_stateless_unit_has_no_repository_to_copy() {
    let mut st = HostState::default();
    st.stacks.insert(
        "drill".into(),
        StackState {
            pushed_file_hashes: std::collections::BTreeMap::new(),
            component_digests: Default::default(),
            applied_source: None,
            extra_route_files: Vec::new(),
            vmid: 119,
            hostname: "119-app-drill".into(),
            apps: vec![],
            applied_at: 1,
            last_backup: 1,
            applied_hash: String::new(),
            manifest: None,
            natives: vec![unit("drillsvc", true), unit("keeper", false)],
            enabled: true,
            incomplete_step: None,
            route_file: None,
        },
    );
    let names: Vec<String> = repo_policies(&st, &[], &[])
        .into_iter()
        .map(|p| p.repo)
        .collect();
    assert_eq!(names, vec!["keeper".to_string()]);
}

#[tokio::test]
async fn a_stateless_unit_is_not_backed_up_and_that_is_not_a_failure() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let ctx = OpCtx {
        exec: &exec,
        sink: &sink,
        journal: &j,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_790_000_000,
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
    let r = backup_native(&ctx, &unit("drillsvc", true), &BackupCfg::default()).await;
    assert!(r.ok, "{:?}", r.error);
    assert!(
        exec.calls_containing("restic").is_empty(),
        "no repository is created for a unit that keeps nothing: {:?}",
        exec.calls()
    );
}
