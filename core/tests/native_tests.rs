//! C7: native-service adoption. The refusals carry the feature: adopting a
//! container that is not what the manifest claims would point every later
//! backup and update at the wrong thing.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::native::{validate_native, NativeServiceManifest};
use homelab_core::ops::native::adopt;
use homelab_core::ops::OpCtx;
use homelab_core::runner::NullJournal;
use homelab_core::safety::SafetyConfig;
use homelab_core::sink::VecSink;

fn ctx<'a>(exec: &'a MockExecutor, sink: &'a VecSink, journal: &'a NullJournal) -> OpCtx<'a> {
    OpCtx {
        exec,
        sink,
        journal,
        safety: SafetyConfig::default(),
        state_dir: "/var/lib/homelab".into(),
        now_unix: 1_788_000_000,
        kea: None,
    }
}

/// CT 109 as it really is — the first adoption target.
fn mailbox_manifest() -> NativeServiceManifest {
    NativeServiceManifest {
        stack_name: "mailbox".into(),
        vmid: 109,
        hostname: "109-app-mailbox".into(),
        unit: "mailbox".into(),
        binary: "/usr/local/bin/mailbox".into(),
        env_file: Some("/etc/mailbox/mailbox.env".into()),
        data_dirs: vec!["/var/lib/mailbox".into()],
        update_cmd: Some("mailbox update".into()),
    }
}

fn adopt_mocks(exec: &MockExecutor) {
    exec.respond_always(
        "pct config 109",
        CmdOutput::ok("hostname: 109-app-mailbox\n"),
    );
    exec.respond_always("systemctl is-active", CmdOutput::ok("active\n"));
    exec.respond_always(
        "systemctl show",
        CmdOutput::ok(
            "ExecStart={ path=/usr/local/bin/mailbox ; argv[]=/usr/local/bin/mailbox }\n\
             EnvironmentFiles=/etc/mailbox/mailbox.env (ignore_errors=no)\n",
        ),
    );
    exec.respond_always("test -x", CmdOutput::ok(""));
}

#[test]
fn c7_validation_catches_the_lies() {
    assert!(validate_native(&mailbox_manifest()).is_ok());

    let mut wrong_host = mailbox_manifest();
    wrong_host.hostname = "mailbox".into();
    assert!(validate_native(&wrong_host).is_err(), "hostname convention");

    let mut rel_path = mailbox_manifest();
    rel_path.binary = "usr/local/bin/mailbox".into();
    assert!(validate_native(&rel_path).is_err(), "relative path");

    let mut no_data = mailbox_manifest();
    no_data.data_dirs.clear();
    assert!(validate_native(&no_data).is_err(), "undeclared state");
}

#[tokio::test]
async fn c7_adopt_records_state_without_touching_the_service() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(report.ok, "{:?}", report.error);

    let state = exec.file("/var/lib/homelab/state.json").unwrap();
    assert!(state.contains("\"mailbox\""), "{}", state);
    assert!(state.contains("/var/lib/mailbox"), "data dir recorded");

    // The whole point: nothing may (re)start, stop or write in the CT.
    for forbidden in ["restart", "systemctl stop", "systemctl start", "pct push"] {
        assert!(
            exec.calls_containing(forbidden).is_empty(),
            "adoption must never run '{}': {:?}",
            forbidden,
            exec.calls_containing(forbidden)
        );
    }
}

#[tokio::test]
async fn c7_adopt_refuses_inactive_unit() {
    let exec = MockExecutor::new();
    // First matching always-rule wins, so the override precedes the defaults.
    exec.respond_always("systemctl is-active", CmdOutput::ok("inactive\n"));
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(!report.ok, "adoption never starts services");
    let state = exec.file("/var/lib/homelab/state.json");
    assert!(state.is_none(), "no state may be recorded on refusal");
}

#[tokio::test]
async fn c7_adopt_refuses_unit_running_a_different_binary() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "systemctl show",
        CmdOutput::ok("ExecStart={ path=/opt/other/thing }\nEnvironmentFiles=\n"),
    );
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(!report.ok, "manifest must match reality");
}

#[tokio::test]
async fn c7_adopt_refuses_no_touch_and_wrong_hostname() {
    // No-touch vmid.
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = mailbox_manifest();
    m.vmid = 104;
    m.hostname = "104-app-mailbox".into();
    let report = adopt(&ctx(&exec, &sink, &j), &m).await;
    assert!(!report.ok, "no-touch");
    assert!(exec.calls_containing("systemctl").is_empty());

    // Live hostname differs from the manifest.
    let exec = MockExecutor::new();
    exec.respond_always("pct config 109", CmdOutput::ok("hostname: 109-app-other\n"));
    let report = adopt(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(!report.ok, "A2 hostname guard");
}

#[tokio::test]
async fn c7_adopt_refuses_repointing_an_existing_stack() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"mailbox":{"vmid":140,"hostname":"140-app-mailbox","apps":["mailbox"],"applied_at":1}}}"#,
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(!report.ok, "a stack name points at one vmid, forever");
}

// ── C7: native backup + supervised self-update ──────────────────────────────

#[tokio::test]
async fn c7_native_backup_streams_tar_into_restic() {
    use homelab_core::ops::backup::BackupCfg;
    use homelab_core::ops::native::backup_native;
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("snapshots --json", CmdOutput::ok("[]"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = backup_native(
        &ctx(&exec, &sink, &j),
        &mailbox_manifest(),
        &BackupCfg::default(),
    )
    .await;
    assert!(report.ok, "{:?}", report.error);
    let pipelines = exec.calls_containing("pct exec 109 -- tar -cf -");
    assert_eq!(pipelines.len(), 1, "{:?}", exec.calls());
    let p = &pipelines[0];
    assert!(
        p.contains("set -o pipefail"),
        "without pipefail a dead tar yields a lying empty snapshot: {}",
        p
    );
    assert!(p.contains("'/var/lib/mailbox'"), "data dir quoted: {}", p);
    assert!(
        p.contains("restic backup --stdin --stdin-filename mailbox-data.tar"),
        "{}",
        p
    );
    assert!(
        p.contains("mailbox-config"),
        "same repo naming as compose stacks: {}",
        p
    );
}

#[tokio::test]
async fn c7_supervised_update_restarts_only_on_binary_change() {
    use homelab_core::ops::native::update_native;
    // Unchanged binary: the self-update said "already current" → no restart,
    // no nightly service blip.
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("sha256sum", CmdOutput::ok("aaaa\n"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update_native(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(report.ok, "{:?}", report.error);
    assert!(
        exec.calls_containing("systemctl restart").is_empty(),
        "no restart when the binary did not change"
    );

    // Changed binary: restart + health check, all good.
    let exec = MockExecutor::new();
    exec.enqueue("sha256sum", CmdOutput::ok("aaaa\n"));
    exec.respond_always("sha256sum", CmdOutput::ok("bbbb\n"));
    adopt_mocks(&exec);
    exec.respond_always("systemctl restart", CmdOutput::ok(""));
    let report = update_native(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing(
            "cp -p '/usr/local/bin/mailbox' '/usr/local/bin/mailbox.homelab-prev'"
        )
        .len(),
        1,
        "binary preserved before the update: {:?}",
        exec.calls()
    );
}

#[tokio::test]
async fn c7_supervised_update_rolls_back_when_new_version_stays_down() {
    use homelab_core::ops::native::update_native;
    let exec = MockExecutor::new();
    exec.enqueue("sha256sum", CmdOutput::ok("aaaa\n"));
    exec.respond_always("sha256sum", CmdOutput::ok("bbbb\n"));
    // Health loop after restart fails; the rollback script (cp back +
    // restart) succeeds.
    exec.respond_always(
        "for i in 1 2 3 4 5",
        CmdOutput::failed(1, "unit stays down"),
    );
    exec.respond_always(
        "cp -p '/usr/local/bin/mailbox.homelab-prev'",
        CmdOutput::ok(""),
    );
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update_native(&ctx(&exec, &sink, &j), &mailbox_manifest()).await;
    assert!(!report.ok, "a rolled-back update is still a FAILED update");
    assert_eq!(
        exec.calls_containing(
            "cp -p '/usr/local/bin/mailbox.homelab-prev' '/usr/local/bin/mailbox'"
        )
        .len(),
        1,
        "the armed rollback must restore the preserved binary: {:?}",
        exec.calls()
    );
    let err = report.error.expect("failure carries the story");
    assert!(
        err.why.contains("rolled back") || err.remedy.contains("rolled back"),
        "{:?}",
        err
    );
}
