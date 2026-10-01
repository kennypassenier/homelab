//! fix-114 (native-rollback-copies-deleted, 2026-09-27): a healthy native
//! update deleted every local rollback copy after a ten-second health window,
//! so a release that misbehaved an hour later had no N-1 binary on disk and
//! no verb to go back to one.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::native::NativeServiceManifest;
use homelab_core::ops::native::{
    drop_stale_rollback_script, keep_one_previous_script, rollback_native, update_native,
};
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;
use homelab_core::state::HostState;

const NOW: u64 = 1_790_000_000;
const BIN: &str = "/opt/kyu/bin/kyu";
const PREV: &str = "/opt/kyu/bin/kyu.homelab-prev";

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

fn kyu() -> NativeServiceManifest {
    NativeServiceManifest {
        restore_note: None,
        stack_name: "kyu".into(),
        vmid: 109,
        hostname: "109-app-kyu".into(),
        unit: "kyu".into(),
        binary: BIN.into(),
        env_file: None,
        data_dirs: vec!["/appdata/kyu/kyu-config".into()],
        update_cmd: Some("kyu update".into()),
        stateless: false,
        release_repo: None,
        release_asset: None,
        backup_from_newest: None,
        backup_pause: false,
        update_policy: Default::default(),
        metrics: None,
    }
}

fn harness() -> MockExecutor {
    let exec = MockExecutor::new();
    exec.respond_always("pct config 109", CmdOutput::ok("hostname: 109-app-kyu\n"));
    exec.respond_always("systemctl restart", CmdOutput::ok(""));
    exec
}

fn ends_with(exec: &MockExecutor, script: &str) -> bool {
    exec.calls().iter().any(|c| c.ends_with(script))
}

#[test]
fn exactly_one_previous_binary_is_kept() {
    let s = keep_one_previous_script(BIN, PREV);
    // The binary changed: the copy taken before the update is the previous
    // one now, and the one before that goes.
    assert!(s.contains(&format!("rm -f '{}.old'", PREV)), "{}", s);
    // It did not change: the previous binary from before this run comes back.
    assert!(
        s.contains(&format!("mv -f '{}.old' '{}'", PREV, PREV)),
        "{}",
        s
    );
    assert!(s.contains(&format!("cmp -s '{}' '{}'", BIN, PREV)), "{}", s);
}

#[tokio::test]
async fn a_healthy_update_keeps_the_binary_it_replaced() {
    let exec = harness();
    exec.enqueue("sha256sum", CmdOutput::ok("aaaa\n"));
    exec.respond_always("sha256sum", CmdOutput::ok("bbbb\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update_native(&ctx(&exec, &sink, &j), &kyu(), None).await;
    assert!(r.ok, "{:?}", r.error);
    assert!(
        !ends_with(&exec, &drop_stale_rollback_script(PREV)),
        "the N-1 binary must stay on disk: {:?}",
        exec.calls()
    );
    assert!(
        ends_with(&exec, &keep_one_previous_script(BIN, PREV)),
        "{:?}",
        exec.calls()
    );
    // fix-10 stands: the kit's own copy is the same version a second time.
    assert!(ends_with(
        &exec,
        &drop_stale_rollback_script(&format!("{}.prev", BIN))
    ));
}

#[tokio::test]
async fn the_previous_binary_is_set_aside_not_overwritten_before_the_update() {
    let exec = harness();
    exec.respond_always("sha256sum", CmdOutput::ok("aaaa\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = update_native(&ctx(&exec, &sink, &j), &kyu(), None).await;
    assert!(r.ok, "{:?}", r.error);
    let preserve = exec.calls_containing(&format!("cp -p '{}' '{}'", BIN, PREV));
    assert_eq!(preserve.len(), 1, "{:?}", exec.calls());
    assert!(
        preserve[0].contains(&format!("mv -f '{}' '{}.old'", PREV, PREV)),
        "an unchanged night must not replace N-1 with N: {}",
        preserve[0]
    );
    assert!(
        ends_with(&exec, &keep_one_previous_script(BIN, PREV)),
        "and the unchanged run puts N-1 back: {:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn rollback_native_returns_to_the_kept_binary_and_parks_the_updates() {
    let exec = harness();
    exec.respond_always("cmp -s", CmdOutput::ok("yes\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = rollback_native(&ctx(&exec, &sink, &j), &kyu()).await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls();
    let swap = calls
        .iter()
        .position(|c| c.contains(&format!("cp -p '{}' '{}'", PREV, BIN)))
        .expect("the kept binary is copied back");
    let stop = calls
        .iter()
        .position(|c| c.contains("systemctl stop kyu.service"))
        .expect("the unit is stopped first");
    assert!(stop <= swap, "{:?}", calls);
    assert!(
        calls
            .iter()
            .any(|c| c.contains(&format!("mv -f '{}.homelab-rollback' '{}'", BIN, PREV))),
        "the version rolled back from becomes the previous one, so a second rollback \
         returns: {:?}",
        calls
    );
    let st: HostState =
        serde_json::from_str(&exec.file("/var/lib/homelab/state.json").unwrap()).unwrap();
    assert_eq!(
        st.updates_parked.get("kyu"),
        Some(&NOW),
        "or tonight's update reinstalls the release that was just rolled back"
    );
}

#[tokio::test]
async fn rollback_native_without_a_kept_binary_stops_nothing() {
    let exec = harness();
    exec.respond_always("cmp -s", CmdOutput::ok("no\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let r = rollback_native(&ctx(&exec, &sink, &j), &kyu()).await;
    assert!(!r.ok);
    assert!(exec.calls_containing("systemctl stop").is_empty());
}

#[test]
fn the_unit_to_roll_back_is_named_or_the_only_one() {
    use homelab_core::ops::native::select_unit;
    let one = vec![kyu()];
    assert_eq!(select_unit(&one, None).unwrap().unit, "kyu");
    let runner = NativeServiceManifest {
        unit: "kyu-runner".into(),
        ..kyu()
    };
    let two = vec![kyu(), runner];
    assert!(select_unit(&two, None).is_err(), "several units: name one");
    assert_eq!(
        select_unit(&two, Some("kyu-runner")).unwrap().unit,
        "kyu-runner"
    );
    assert!(select_unit(&two, Some("almanac")).is_err());
}
