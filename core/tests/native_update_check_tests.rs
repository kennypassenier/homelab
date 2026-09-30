//! fix-116 (native-update-copies-binary-nightly, 2026-09-27): `update_native`
//! copied every native binary aside every night (40 MB for kyu on CT 109's
//! small rootfs) and ran the service's own update, even when the release was
//! the one already installed. It asks the release first now, as
//! `release_update` does.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::native::update_native;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const RELEASE: &str = r#"{"tag_name":"v4.0.1","assets":[
  {"name":"kyu","browser_download_url":"https://github.com/kennypassenier/kyu/releases/download/v4.0.1/kyu"},
  {"name":"SHA256SUMS","browser_download_url":"https://github.com/kennypassenier/kyu/releases/download/v4.0.1/SHA256SUMS"}]}"#;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_790_000_000,
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        tile_watch_source: None,
    }
}

fn kyu() -> NativeServiceManifest {
    NativeServiceManifest {
        restore_note: None,
        stack_name: "kyu".into(),
        vmid: 109,
        hostname: "109-app-kyu".into(),
        unit: "kyu".into(),
        binary: "/opt/kyu/bin/kyu".into(),
        env_file: None,
        data_dirs: vec!["/appdata/kyu/kyu-config".into()],
        update_cmd: Some("kyu update".into()),
        stateless: false,
        release_repo: Some("kennypassenier/kyu".into()),
        release_asset: Some("kyu".into()),
        backup_from_newest: None,
        backup_pause: false,
        update_policy: Default::default(),
        metrics: None,
    }
}

fn harness(listed: &str) -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always("pct config 109", CmdOutput::ok("hostname: 109-app-kyu\n"));
    exec.respond_always("api.github.com", CmdOutput::ok(RELEASE));
    exec.respond_always(
        "releases/download/v4.0.1/SHA256SUMS",
        CmdOutput::ok(&format!("{}  kyu\n", listed)),
    );
    exec.respond_always("sha256sum", CmdOutput::ok("aaaa\n"));
    exec
}

#[tokio::test]
async fn an_installed_release_is_neither_copied_nor_updated() {
    let exec = harness("aaaa");
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update_native(&ctx(&exec, &sink, &j), &kyu(), None).await;
    assert!(r.ok, "{:?}", r.error);
    assert!(
        exec.calls_containing("cp -p").is_empty(),
        "no nightly copy of a binary that is current: {:?}",
        exec.calls()
    );
    assert!(exec.calls_containing("kyu update").is_empty());
}

#[tokio::test]
async fn a_newer_release_is_preserved_and_updated_as_before() {
    let exec = harness("bbbb");
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update_native(&ctx(&exec, &sink, &j), &kyu(), None).await;
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(
        exec.calls_containing("cp -p").len(),
        1,
        "{:?}",
        exec.calls()
    );
    assert_eq!(exec.calls_containing("kyu update").len(), 1);
}

#[tokio::test]
async fn an_unreachable_github_falls_back_to_the_supervised_update() {
    let exec = harness("aaaa");
    exec.respond_first(
        "api.github.com",
        CmdOutput::failed(6, "could not resolve host"),
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update_native(&ctx(&exec, &sink, &j), &kyu(), None).await;
    assert!(r.ok, "{:?}", r.error);
    assert_eq!(
        exec.calls_containing("kyu update").len(),
        1,
        "the service's own update decides when the release cannot be read: {:?}",
        exec.calls()
    );
}
