//! Route A (Kenny, form J1 via the OPNsense session, 2026-09-02): the
//! orchestrator asks a device it may not touch for its own configuration and
//! stores the answer in restic.

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::ops::backup::BackupCfg;
use homelab_core::ops::devicebackup::{backup_device, DeviceBackup};
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
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        asker: &homelab_core::ask::NOBODY,
        backup: Default::default(),
        registry_cache: None,
    }
}

fn dev(ca: Option<&str>) -> DeviceBackup {
    DeviceBackup {
        name: "opnsense".into(),
        url: "https://10.10.10.1/api/core/backup/download/this".into(),
        cred_file: "/var/lib/homelab/secrets/opnsense-backup.conf".into(),
        filename: "config.xml".into(),
        pin: None,
        ca_file: ca.map(String::from),
    }
}

fn pinned(pin: &str) -> DeviceBackup {
    DeviceBackup {
        pin: Some(pin.into()),
        ..dev(None)
    }
}

/// covers: F205
///
/// The credential must never reach argv: `/proc/<pid>/cmdline` is
/// world-readable, and this one can download a router's entire
/// configuration. `-K <file>` keeps it off the command line.
#[tokio::test]
async fn the_credential_never_reaches_the_command_line() {
    let exec = MockExecutor::new();
    exec.respond_always("restic stats", CmdOutput::ok("{\"total_size\":114688}"));
    exec.respond_always("curl", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let c = ctx(&exec, &sink, &j);

    let _ = backup_device(&c, &dev(None), &BackupCfg::default()).await;

    let calls = exec.calls();
    let curl = calls
        .iter()
        .find(|c| c.contains("curl"))
        .expect("no curl call was made");
    assert!(
        curl.contains("-K '/var/lib/homelab/secrets/opnsense-backup.conf'"),
        "the credential must arrive through curl's config file: {}",
        curl
    );
    assert!(
        !curl.contains("-u "),
        "-u puts the credential in argv, which /proc exposes: {}",
        curl
    );
}

/// covers: F205
///
/// A login page is a 200 with a body, and restic stores whatever it is
/// handed. The snapshot is therefore weighed after the fact, not trusted.
#[tokio::test]
async fn a_snapshot_too_small_to_be_a_configuration_fails() {
    let exec = MockExecutor::new();
    exec.respond_always("restic stats", CmdOutput::ok("{\"total_size\":812}"));
    exec.respond_always("curl", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let c = ctx(&exec, &sink, &j);

    let report = backup_device(&c, &dev(None), &BackupCfg::default()).await;
    assert!(
        !report.ok,
        "812 bytes is an error page, not a configuration"
    );
    let err = report.error.expect("a failed report carries an error");
    assert!(err.why.contains("812 bytes"), "{:?}", err);
    assert!(
        !err.remedy.is_empty(),
        "rule 11: every error message carries a remedy — {:?}",
        err
    );
}

/// covers: F205
///
/// Verification is configuration-dependent (rule 24), so the code cannot
/// enforce it — but it must not be silent about running without it.
#[tokio::test]
async fn an_unverified_certificate_is_used_but_never_quietly() {
    let exec = MockExecutor::new();
    exec.respond_always("restic stats", CmdOutput::ok("{\"total_size\":114688}"));
    exec.respond_always("curl", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let c = ctx(&exec, &sink, &j);

    let _ = backup_device(&c, &dev(None), &BackupCfg::default()).await;
    assert!(
        sink.lines().iter().any(|l| l.contains("NOT verified")),
        "running without certificate verification must be said out loud"
    );

    let exec2 = MockExecutor::new();
    exec2.respond_always("restic stats", CmdOutput::ok("{\"total_size\":114688}"));
    exec2.respond_always("curl", CmdOutput::ok(""));
    let sink2 = VecSink::new();
    let c2 = ctx(&exec2, &sink2, &j);
    let _ = backup_device(
        &c2,
        &dev(Some("/etc/ssl/opnsense.pem")),
        &BackupCfg::default(),
    )
    .await;
    assert!(
        exec2
            .calls()
            .iter()
            .any(|c| c.contains("--cacert '/etc/ssl/opnsense.pem'")),
        "a configured CA must actually be passed to curl"
    );
    assert!(
        !sink2.lines().iter().any(|l| l.contains("NOT verified")),
        "no warning when verification is on"
    );
}

/// covers: F205
///
/// The pin is what verifies this connection, and it must reach curl exactly.
/// Measured against the live router on 2026-09-02: the right pin returns
/// 200, a wrong one returns curl exit 90 and no connection — so a typo here
/// fails loudly rather than falling back to trusting anything.
#[tokio::test]
async fn a_pinned_public_key_is_what_verifies_the_connection() {
    let exec = MockExecutor::new();
    exec.respond_always("restic stats", CmdOutput::ok("{\"total_size\":114688}"));
    exec.respond_always("curl", CmdOutput::ok(""));
    let sink = VecSink::new();
    let j = NullJournal;
    let c = ctx(&exec, &sink, &j);

    let _ = backup_device(
        &c,
        &pinned("sha256//oZKgUOWR56fT3HYG68aGVn7s1saleArMf75StP1KaUE="),
        &BackupCfg::default(),
    )
    .await;

    let curl = exec
        .calls()
        .into_iter()
        .find(|c| c.contains("curl"))
        .expect("no curl call");
    assert!(
        curl.contains("--pinnedpubkey 'sha256//oZKgUOWR56fT3HYG68aGVn7s1saleArMf75StP1KaUE='"),
        "the pin must reach curl verbatim: {}",
        curl
    );
    assert!(
        !sink.lines().iter().any(|l| l.contains("NOT verified")),
        "a pinned connection is verified — the warning is for the unpinned case"
    );
}
