//! feat-backup-2: `restore_native` — the gated, chosen-snapshot restore of
//! an adopted (native) service from the Backups page. Mirrors
//! `native_backup_tests.rs`'s harness.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::native::{BackupPause, NativeServiceManifest};
use homelab_core::ops::OpCtx;
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::native::restore_native;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

const NOW: u64 = 1_790_000_000;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: NOW,
        metrics_targets_dir: None,
        loki_url: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
        default_log_rotation: None,
        tile_watch_source: None,
        tile_watch_targets: Vec::new(),
        tile_watch_watcher: None,
    }
}

fn almanac(data_dirs: Vec<String>, stateless: bool) -> NativeServiceManifest {
    NativeServiceManifest {
        restore_note: None,
        stack_name: "almanac".into(),
        vmid: 112,
        hostname: "112-app-almanac".into(),
        unit: "almanac".into(),
        binary: "/opt/almanac/bin/almanac".into(),
        env_file: None,
        data_dirs,
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

fn harness() -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config 112",
        CmdOutput::ok("hostname: 112-app-almanac\n"),
    );
    exec.respond_always("systemctl stop", CmdOutput::ok(""));
    exec.respond_always("systemctl start", CmdOutput::ok(""));
    exec.respond_always("mkdir -p", CmdOutput::ok(""));
    exec.respond_always("restic dump", CmdOutput::ok(""));
    exec
}

fn position(calls: &[String], needle: &str) -> usize {
    calls
        .iter()
        .position(|c| c.contains(needle))
        .unwrap_or_else(|| panic!("no call containing {:?}: {:?}", needle, calls))
}

#[tokio::test]
async fn refuses_without_the_typed_name() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &almanac(vec!["/appdata/almanac/almanac-config".into()], false),
        &BackupCfg::default(),
        "latest",
        None,
    )
    .await;
    assert!(!r.ok);
    // The positive twin: the refusal names why, not just that it failed.
    assert!(
        r.error
            .as_ref()
            .is_some_and(|e| e.why.contains("no typed stack name")),
        "{:?}",
        r.error
    );
    // Nothing was stopped — the gate runs before any step that touches
    // the unit (fix-64's rule, mirrored for the native path).
    assert!(!exec.calls().iter().any(|c| c.contains("systemctl stop")));
}

#[tokio::test]
async fn refuses_when_the_typed_name_does_not_match() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &almanac(vec!["/appdata/almanac/almanac-config".into()], false),
        &BackupCfg::default(),
        "latest",
        Some("not-almanac"),
    )
    .await;
    assert!(!r.ok);
    // The positive twin: refused for naming the wrong stack, not some
    // other reason.
    assert!(
        r.error
            .as_ref()
            .is_some_and(|e| e.why.contains("does not match")),
        "{:?}",
        r.error
    );
}

#[tokio::test]
async fn stops_copies_unpacks_and_restarts_in_order() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &almanac(vec!["/appdata/almanac/almanac-config".into()], false),
        &BackupCfg::default(),
        "a1b2c3",
        Some("almanac"),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    let stop = position(&calls, "systemctl stop");
    let copy = position(&calls, "mkdir -p");
    let unpack = position(&calls, "restic dump");
    let start = position(&calls, "systemctl start");
    assert!(
        stop < copy && copy < unpack && unpack < start,
        "wrong order: {:?}",
        calls
    );
    // The snapshot id picked on the Backups page rides straight through to
    // the restic command, not silently replaced by "latest".
    assert!(calls[unpack].contains("a1b2c3"));
}

#[tokio::test]
async fn a_stateless_unit_is_told_there_is_nothing_to_restore() {
    let exec = harness();
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &almanac(vec![], true),
        &BackupCfg::default(),
        "latest",
        Some("almanac"),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    // The positive twin: it is told so, not merely silent about it.
    assert!(
        sink.lines().join("\n").contains("nothing to restore"),
        "{:?}",
        sink.lines()
    );
    assert!(!exec.calls().iter().any(|c| c.contains("systemctl stop")));
}

#[tokio::test]
async fn a_failed_unpack_still_leaves_the_unit_stopped_rather_than_half_restored() {
    let exec = harness();
    // respond_first: the harness already scripts a healthy "restic dump" as
    // its default, and `respond_always` only appends — the first-registered
    // rule still wins, so this override must jump the queue to actually
    // replace it.
    exec.respond_first("restic dump", CmdOutput::failed(1, "no such snapshot"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &almanac(vec!["/appdata/almanac/almanac-config".into()], false),
        &BackupCfg::default(),
        "missing",
        Some("almanac"),
    )
    .await;
    assert!(!r.ok);
    // The positive twin: the unit WAS stopped — "left stopped", not "never
    // touched" — before the unpack failed and the restart never ran.
    assert!(exec.calls().iter().any(|c| c.contains("systemctl stop")));
    assert!(!exec.calls().iter().any(|c| c.contains("systemctl start")));
}

// fix-146 (native-empty-rebuild follow-up, 2026-10-01): a hand-triggered
// restore from the Backups page is exactly as much "a restore" as the
// automatic empty-rebuild path, so it runs the same `after_restore` step.

#[tokio::test]
async fn after_restore_runs_between_the_unpack_and_the_start() {
    let exec = harness();
    exec.respond_always("mv -f", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = almanac(vec!["/appdata/almanac/almanac-config".into()], false);
    m.after_restore = Some("mv -f /a /b".into());
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &m,
        &BackupCfg::default(),
        "latest",
        Some("almanac"),
    )
    .await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    let unpack = position(&calls, "restic dump");
    let seed = position(&calls, "mv -f");
    let start = position(&calls, "systemctl start");
    assert!(unpack < seed && seed < start, "wrong order: {:?}", calls);
}

#[tokio::test]
async fn a_failed_after_restore_leaves_the_unit_stopped() {
    let exec = harness();
    exec.respond_always("mv -f", CmdOutput::failed(1, "no such file"));
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = almanac(vec!["/appdata/almanac/almanac-config".into()], false);
    m.after_restore = Some("mv -f /a /b".into());
    let r = restore_native(
        &ctx(&exec, &sink, &j),
        &m,
        &BackupCfg::default(),
        "latest",
        Some("almanac"),
    )
    .await;
    assert!(!r.ok);
    assert!(!exec.calls().iter().any(|c| c.contains("systemctl start")));
}
