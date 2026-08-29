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
