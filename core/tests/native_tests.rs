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
        metrics_targets_dir: None,
        grafana_dashboards_dir: None,
        homepage_services_file: None,
        kuma_monitors_file: None,
        backup: Default::default(),
        registry_cache: None,
    }
}

/// CT 109 as it really is — the first adoption target.
fn kyu_manifest() -> NativeServiceManifest {
    NativeServiceManifest {
        stack_name: "kyu".into(),
        vmid: 109,
        hostname: "109-app-kyu".into(),
        unit: "kyu".into(),
        binary: "/usr/local/bin/kyu".into(),
        env_file: Some("/etc/kyu/kyu.env".into()),
        data_dirs: vec!["/var/lib/kyu".into()],
        update_cmd: Some("kyu update".into()),
        stateless: false,
        release_repo: None,
        release_asset: None,
    }
}

fn adopt_mocks(exec: &MockExecutor) {
    exec.respond_always("pct config 109", CmdOutput::ok("hostname: 109-app-kyu\n"));
    exec.respond_always("systemctl is-active", CmdOutput::ok("active\n"));
    exec.respond_always(
        "systemctl show",
        CmdOutput::ok(
            "ExecStart={ path=/usr/local/bin/kyu ; argv[]=/usr/local/bin/kyu }\n\
             EnvironmentFiles=/etc/kyu/kyu.env (ignore_errors=no)\n",
        ),
    );
    exec.respond_always("test -x", CmdOutput::ok(""));
}

#[test]
fn c7_validation_catches_the_lies() {
    assert!(validate_native(&kyu_manifest()).is_ok());

    let mut wrong_host = kyu_manifest();
    wrong_host.hostname = "kyu".into();
    assert!(validate_native(&wrong_host).is_err(), "hostname convention");

    let mut rel_path = kyu_manifest();
    rel_path.binary = "usr/local/bin/kyu".into();
    assert!(validate_native(&rel_path).is_err(), "relative path");

    let mut no_data = kyu_manifest();
    no_data.data_dirs.clear();
    assert!(validate_native(&no_data).is_err(), "undeclared state");
}

#[tokio::test]
async fn c7_adopt_records_state_without_touching_the_service() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
    assert!(report.ok, "{:?}", report.error);

    let state = exec.file("/var/lib/homelab/state.json").unwrap();
    assert!(state.contains("\"kyu\""), "{}", state);
    assert!(state.contains("/var/lib/kyu"), "data dir recorded");

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
    let report = adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
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
    let report = adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
    assert!(!report.ok, "manifest must match reality");
}

#[tokio::test]
async fn c7_adopt_refuses_no_touch_and_wrong_hostname() {
    // No-touch vmid.
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    let j = NullJournal;
    let mut m = kyu_manifest();
    m.vmid = 104;
    m.hostname = "104-app-kyu".into();
    let report = adopt(&ctx(&exec, &sink, &j), &m).await;
    assert!(!report.ok, "no-touch");
    assert!(exec.calls_containing("systemctl").is_empty());

    // Live hostname differs from the manifest.
    let exec = MockExecutor::new();
    exec.respond_always("pct config 109", CmdOutput::ok("hostname: 109-app-other\n"));
    let report = adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
    assert!(!report.ok, "A2 hostname guard");
}

#[tokio::test]
async fn c7_adopt_refuses_repointing_an_existing_stack() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.seed_file(
        "/var/lib/homelab/state.json",
        r#"{"schema_version":1,"stacks":{"kyu":{"vmid":140,"hostname":"140-app-kyu","apps":["kyu"],"applied_at":1}}}"#,
    );
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
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
        &kyu_manifest(),
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
    assert!(p.contains("'/var/lib/kyu'"), "data dir quoted: {}", p);
    assert!(
        p.contains("restic backup --stdin --stdin-filename kyu-data.tar"),
        "{}",
        p
    );
    assert!(
        p.contains("kyu-config"),
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
    let report = update_native(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
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
    let report = update_native(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
    assert!(report.ok, "{:?}", report.error);
    assert_eq!(
        exec.calls_containing("cp -p '/usr/local/bin/kyu' '/usr/local/bin/kyu.homelab-prev'")
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
    exec.respond_always("cp -p '/usr/local/bin/kyu.homelab-prev'", CmdOutput::ok(""));
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = update_native(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
    assert!(!report.ok, "a rolled-back update is still a FAILED update");
    assert_eq!(
        exec.calls_containing("cp -p '/usr/local/bin/kyu.homelab-prev' '/usr/local/bin/kyu'")
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

/// T5: several native services share one container — the layout puts kyu,
/// kyu-runner and http-switchboard on CT 109. They cannot be three separate
/// stacks: `validate_native` forces the hostname `<vmid>-app-<stack>` and
/// `guard_target` re-checks it against the live container, so three stacks on
/// one vmid would need three hostnames on one machine. So one stack holds a
/// list, and adoption adds to it.
#[tokio::test]
async fn t5_a_stack_holds_several_native_services() {
    use homelab_core::ops::native::adopt;
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    // Specific rules first: respond_always takes the FIRST match, so the
    // generic "systemctl show" from adopt_mocks would otherwise answer for
    // every unit and hand the runner kyu's ExecStart.
    exec.respond_always(
        "systemctl show kyu-runner.service",
        CmdOutput::ok(
            "ExecStart={ path=/usr/local/bin/kyu-runner ; argv[]=/usr/local/bin/kyu-runner }\n\
             EnvironmentFiles=/etc/kyu-runner/token.env (ignore_errors=no)\n",
        ),
    );
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;

    let hub = kyu_manifest();
    let r = adopt(&ctx(&exec, &sink, &j), &hub).await;
    assert!(r.ok, "{:?}", r.error);

    let runner = NativeServiceManifest {
        unit: "kyu-runner".into(),
        binary: "/usr/local/bin/kyu-runner".into(),
        env_file: Some("/etc/kyu-runner/token.env".into()),
        data_dirs: vec![],
        stateless: true,
        release_repo: None,
        release_asset: None,
        update_cmd: None,
        ..kyu_manifest()
    };
    let r = adopt(&ctx(&exec, &sink, &j), &runner).await;
    assert!(r.ok, "{:?}", r.error);

    let state = StateStore::new(&exec, "/var/lib/homelab")
        .load()
        .await
        .expect("state loads");
    let st = state.stacks.get("kyu").expect("the stack is recorded");
    assert_eq!(st.natives.len(), 2, "both services must be recorded");
    let units: Vec<&str> = st.natives.iter().map(|n| n.unit.as_str()).collect();
    assert!(
        units.contains(&"kyu") && units.contains(&"kyu-runner"),
        "{:?}",
        units
    );
    assert_eq!(st.apps, vec!["kyu".to_string(), "kyu-runner".to_string()]);
    assert!(st.is_native());

    // Re-adopting one service corrects that entry and leaves the other alone —
    // which is how the mailbox→kyu manifest correction lands.
    let corrected = NativeServiceManifest {
        unit: "kyu-runner".into(),
        binary: "/usr/local/bin/kyu-runner".into(),
        env_file: Some("/etc/kyu-runner/token.env".into()),
        data_dirs: vec![],
        stateless: true,
        release_repo: None,
        release_asset: None,
        update_cmd: Some("kyu-runner update".into()),
        ..kyu_manifest()
    };
    let r = adopt(&ctx(&exec, &sink, &j), &corrected).await;
    assert!(r.ok, "{:?}", r.error);
    let state = StateStore::new(&exec, "/var/lib/homelab")
        .load()
        .await
        .unwrap();
    let st = state.stacks.get("kyu").unwrap();
    assert_eq!(
        st.natives.len(),
        2,
        "a re-adopt must not duplicate the unit"
    );
    let rn = st.natives.iter().find(|n| n.unit == "kyu-runner").unwrap();
    assert_eq!(rn.update_cmd.as_deref(), Some("kyu-runner update"));
}

/// T5 migration: state written before native services became a list still
/// loads, and its single service moves into the list. Anything else would
/// have made the running host forget the two containers it already manages.
#[tokio::test]
async fn t5_pre_list_state_migrates_on_load() {
    use homelab_core::state::StateStore;
    let exec = MockExecutor::new();
    let legacy = r#"{
      "schema_version": 1,
      "stacks": {
        "almanac": {
          "vmid": 112, "hostname": "112-app-almanac", "apps": ["almanac"],
          "applied_at": 1, "last_backup": 2, "applied_hash": "",
          "manifest": null, "enabled": true,
          "native": {
            "stack_name": "almanac", "vmid": 112, "hostname": "112-app-almanac",
            "unit": "almanac", "binary": "/usr/local/bin/almanac",
            "env_file": null, "data_dirs": ["/var/lib/almanac"], "update_cmd": null
          }
        }
      }
    }"#;
    exec.seed_file("/var/lib/homelab/state.json", legacy);
    let state = StateStore::new(&exec, "/var/lib/homelab")
        .load()
        .await
        .expect("legacy state must still load");
    let st = state.stacks.get("almanac").expect("stack survives");
    assert_eq!(
        st.natives.len(),
        1,
        "the single service moved into the list"
    );
    assert_eq!(st.natives[0].unit, "almanac");
    assert!(
        st.native.is_none(),
        "the legacy field is cleared after the move"
    );
    assert_eq!(st.last_backup, 2, "unrelated bookkeeping is untouched");
}

/// D25 for native services: the repository is named after the SERVICE, not
/// the stack. With T5 putting kyu, kyu-runner and http-switchboard on one
/// container, a per-stack repository would have folded three services into
/// one — and moving any of them elsewhere would have left its history behind,
/// which is the exact thing D25 exists to prevent.
///
/// For the two services live today the name is unchanged, because their unit
/// and their stack happen to share a name. That is why this needed a test
/// rather than an eye: nothing about the current fleet would have shown it.
#[tokio::test]
async fn d25_native_backup_uses_the_service_name_for_its_repo() {
    use homelab_core::ops::backup::BackupCfg;
    use homelab_core::ops::native::backup_native;
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("snapshots --json", CmdOutput::ok("[]"));
    let sink = VecSink::new();
    let j = NullJournal;
    let runner = NativeServiceManifest {
        unit: "kyu-runner".into(),
        binary: "/usr/local/bin/kyu-runner".into(),
        env_file: None,
        data_dirs: vec!["/etc/kyu-runner".into()],
        stateless: false,
        release_repo: None,
        release_asset: None,
        update_cmd: None,
        ..kyu_manifest()
    };
    let r = backup_native(&ctx(&exec, &sink, &j), &runner, &BackupCfg::default()).await;
    assert!(r.ok, "{:?}", r.error);
    let calls = exec.calls_containing("restic backup --stdin").join(" ");
    assert!(
        calls.contains("homelab-backups/kyu-runner-config"),
        "the repo must be named after the service: {}",
        calls
    );
    assert!(
        calls.contains("kyu-runner-data.tar"),
        "and so must the archive inside it: {}",
        calls
    );
}

/// The `homelab` tag is what makes "managed" visible in the Proxmox list, and
/// it was only ever applied where a container is created. The two adopted
/// containers therefore carried no tag: a filter on it silently missed them,
/// so the tag meant "built by the orchestrator" while it read as "managed by
/// the orchestrator". Kenny asked the question that found it.
/// covers: F117
#[tokio::test]
async fn c7_adoption_tags_the_container_as_managed() {
    let exec = MockExecutor::new();
    // First matching always-rule wins, so the override precedes the defaults.
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\narch: amd64\n"),
    );
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await;
    assert!(report.ok, "{:?}", report.error);
    let set = exec.calls_containing("--tags");
    assert_eq!(set.len(), 1, "exactly one tag write: {:?}", set);
    assert!(set[0].contains("homelab"), "{}", set[0]);
}

/// A container that already carries the tag is not written to again, and a
/// container that carries somebody else's tags keeps them.
/// covers: F117
#[tokio::test]
async fn c7_adoption_never_clobbers_existing_tags() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\ntags: homelab\n"),
    );
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    assert!(adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await.ok);
    assert!(
        exec.calls_containing("--tags").is_empty(),
        "already tagged means nothing to write"
    );

    let exec = MockExecutor::new();
    exec.respond_always(
        "pct config",
        CmdOutput::ok("hostname: 109-app-kyu\ntags: production;critical\n"),
    );
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    assert!(adopt(&ctx(&exec, &sink, &j), &kyu_manifest()).await.ok);
    let set = exec.calls_containing("--tags");
    assert_eq!(set.len(), 1);
    for t in ["production", "critical", "homelab"] {
        assert!(set[0].contains(t), "{} must survive: {}", t, set[0]);
    }
}

// ── T11: installing a native service into a container the deploy made ──
//
// The half of C7 that never existed. `stacks/kyu/lxc-compose.yml` has said
// since it was written that a rebuild ends with "the three binaries are
// installed the way C7 installs them" — and that was a sentence, not a verb.

const UNIT_FILE: &str = "[Unit]\nDescription=kyu\n\n[Service]\n\
                         ExecStart=/usr/local/bin/kyu\n\n[Install]\n\
                         WantedBy=multi-user.target\n";

fn install_manifest() -> NativeServiceManifest {
    NativeServiceManifest {
        release_repo: Some("kennypassenier/kyu".into()),
        ..kyu_manifest()
    }
}

/// A unit file that runs a different program than the manifest declares is
/// refused BEFORE anything is written. Adoption already refuses this
/// mismatch when it finds it live; catching it here means the container
/// never reaches that state at all.
#[tokio::test]
async fn an_install_refuses_a_unit_file_that_runs_a_different_binary() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let wrong = "[Service]\nExecStart=/usr/local/bin/something-else\n";
    let report = homelab_core::ops::native::install_native(
        &ctx(&exec, &sink, &j),
        &install_manifest(),
        "YmluYXJ5",
        wrong,
    )
    .await;
    assert!(
        !report.ok,
        "a unit that execs another binary must be refused"
    );
    let why = format!("{:?}", report.error);
    assert!(why.contains("does not exec"), "{}", why);
    // Nothing may have been written: the check has to come before the push.
    assert!(
        !exec.file_paths().iter().any(|p| p.contains(".service")),
        "no unit file may be written when it was refused: {:?}",
        exec.file_paths()
    );
}

/// An empty unit file is its own refusal. A binary installed with nothing to
/// run it produces a container that holds the program and no service — which
/// looks, from every reading, exactly like a service that is down.
#[tokio::test]
async fn an_install_without_a_unit_file_is_refused() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    let sink = VecSink::new();
    let j = NullJournal;
    let report = homelab_core::ops::native::install_native(
        &ctx(&exec, &sink, &j),
        &install_manifest(),
        "YmluYXJ5",
        "   \n",
    )
    .await;
    assert!(!report.ok);
    assert!(format!("{:?}", report.error).contains("no unit file"));
}

/// The happy path: stage beside the target, install the unit, activate, and
/// end up recorded in state exactly as adoption would record it — a service
/// built this way and one taken over by hand must be indistinguishable
/// afterwards.
#[tokio::test]
async fn an_install_stages_beside_the_target_before_it_replaces_anything() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("test -f", CmdOutput::ok("no\n"));
    exec.respond_always("base64 -d", CmdOutput::ok(""));
    exec.respond_always("systemctl daemon-reload", CmdOutput::ok(""));
    exec.respond_always("systemctl stop", CmdOutput::ok(""));
    exec.respond_always("cat /var/lib/homelab/state.json", CmdOutput::failed(1, ""));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = homelab_core::ops::native::install_native(
        &ctx(&exec, &sink, &j),
        &install_manifest(),
        "YmluYXJ5",
        UNIT_FILE,
    )
    .await;
    assert!(report.ok, "{:?}", report.error);

    let cmds = exec.calls();
    let all = cmds.join(" ;; ");
    // Staged beside, never over: a transfer that dies half way must leave
    // the running service on its own binary.
    assert!(
        all.contains("/usr/local/bin/kyu.homelab-new"),
        "the binary must be staged beside the target: {}",
        all
    );
    // The move happens only after the unit file is in place and systemd has
    // reloaded — otherwise `start` would run against a unit that is not there.
    let staged = cmds
        .iter()
        .position(|c| c.contains("base64 -d"))
        .expect("decode step");
    let reload = cmds
        .iter()
        .position(|c| c.contains("daemon-reload"))
        .expect("daemon-reload step");
    let moved = cmds
        .iter()
        .position(|c| c.contains("mv -f"))
        .expect("move-into-place step");
    assert!(
        staged < reload && reload < moved,
        "order must be decode → daemon-reload → move: {:?}",
        cmds
    );
    // The unit file travels by `pct push`, so it shows up as a command
    // rather than a written host file.
    assert!(
        all.contains("/etc/systemd/system/kyu.service"),
        "the unit file must land in /etc/systemd/system: {}",
        all
    );
}

/// A FIRST install that does not come up has nothing to roll back to, and
/// the message has to say so. Claiming a rollback that did not happen is
/// worse than the failure: it tells the reader the service is on its old
/// version when there is no old version.
#[tokio::test]
async fn a_first_install_that_fails_says_there_is_nothing_to_roll_back_to() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("test -f", CmdOutput::ok("no\n"));
    exec.respond_always("base64 -d", CmdOutput::ok(""));
    exec.respond_always("systemctl daemon-reload", CmdOutput::ok(""));
    // The activation script fails: the service never reaches `active`.
    // `respond_first`, because the shared harness models a healthy container
    // and this test is about an unhealthy one — and the activation script
    // contains `systemctl is-active`, which that harness already answers.
    exec.respond_first("journalctl", CmdOutput::ok("kyu: address in use\n"));
    exec.respond_first("mv -f", CmdOutput::failed(1, "did not start"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = homelab_core::ops::native::install_native(
        &ctx(&exec, &sink, &j),
        &install_manifest(),
        "YmluYXJ5",
        UNIT_FILE,
    )
    .await;
    assert!(!report.ok);
    let why = format!("{:?}", report.error);
    assert!(
        why.contains("no previous binary") && why.contains("FIRST install"),
        "the failure must not claim a rollback that cannot exist: {}",
        why
    );
    assert!(
        why.contains("address in use"),
        "the reason the service refused to start belongs in the message: {}",
        why
    );
    assert!(
        !why.contains("rolled back"),
        "nothing was rolled back: {}",
        why
    );
}

/// A RE-install that does not come up returns to the binary that was
/// running. The difference between this and the test above is a fact about
/// the container, not a flag the caller passes.
#[tokio::test]
async fn a_reinstall_that_fails_returns_to_the_binary_that_was_running() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("test -f", CmdOutput::ok("yes\n"));
    exec.respond_always("cp -p", CmdOutput::ok(""));
    exec.respond_always("base64 -d", CmdOutput::ok(""));
    exec.respond_always("systemctl daemon-reload", CmdOutput::ok(""));
    exec.respond_first("cp -p", CmdOutput::ok(""));
    exec.respond_first("mv -f", CmdOutput::failed(1, "did not start"));
    let sink = VecSink::new();
    let j = NullJournal;
    let report = homelab_core::ops::native::install_native(
        &ctx(&exec, &sink, &j),
        &install_manifest(),
        "YmluYXJ5",
        UNIT_FILE,
    )
    .await;
    assert!(!report.ok);
    let why = format!("{:?}", report.error);
    assert!(why.contains("rolled back"), "{}", why);
    let all = exec.calls().join(" ;; ");
    assert!(
        all.contains("kyu.homelab-prev"),
        "the previous binary must have been preserved and restored: {}",
        all
    );
}

/// The release source is part of the stack file or it is nowhere. A
/// `release_asset` with no repository names a file in a place nobody
/// declared, and `gh` would report "release not found" — which reads as a
/// missing release rather than a wrong stack file.
#[test]
fn a_release_source_that_is_half_declared_is_refused() {
    let mut half = kyu_manifest();
    half.release_asset = Some("kyu".into());
    let problems = validate_native(&half).expect_err("asset without repo must be refused");
    assert!(problems.iter().any(|p| p.contains("nowhere to fetch")));

    let mut bad_repo = install_manifest();
    bad_repo.release_repo = Some("kyu".into());
    let problems = validate_native(&bad_repo).expect_err("a repo must be owner/name");
    assert!(problems.iter().any(|p| p.contains("owner/name")));

    // The default asset name is the unit name — which is what all four
    // native services happen to use.
    assert_eq!(install_manifest().asset_name(), "kyu");
    let mut named = install_manifest();
    named.release_asset = Some("kyu-linux-amd64".into());
    assert_eq!(named.asset_name(), "kyu-linux-amd64");
}

/// F171: the native backup builds its restic environment by hand, and the
/// hand-built copy had drifted from the one every compose stack gets.
///
/// Without a cache directory restic finds neither `$XDG_CACHE_HOME` nor
/// `$HOME` in the host service's environment. It says so on every run — the
/// T12 drill on 2026-09-02 has the line in the output of every native backup
/// — and then re-fetches from Google Drive metadata it should have had on
/// disk. A warning printed nightly that nobody reads is the same as no
/// warning at all.
///
/// covers: F171
#[tokio::test]
async fn the_native_backup_uses_the_same_restic_cache_as_every_other() {
    let exec = MockExecutor::new();
    adopt_mocks(&exec);
    exec.respond_always("snapshots --json", CmdOutput::ok("[]"));
    let sink = VecSink::new();
    let j = NullJournal;
    let cfg = homelab_core::ops::backup::BackupCfg::default();
    let _ = homelab_core::ops::native::backup_native(&ctx(&exec, &sink, &j), &kyu_manifest(), &cfg)
        .await;

    let snapshot = exec
        .calls()
        .into_iter()
        .find(|c| c.contains("restic backup --stdin"))
        .expect("the snapshot pipeline must have run");
    assert!(
        snapshot.contains(&format!(
            "RESTIC_CACHE_DIR={}",
            homelab_core::ops::backup::RESTIC_CACHE_DIR
        )),
        "the streamed backup must use the same cache as every other: {}",
        snapshot
    );
    // And it must still carry what it already carried, or this "fix" would
    // have traded one missing field for another.
    assert!(snapshot.contains("RESTIC_REPOSITORY="), "{}", snapshot);
    assert!(snapshot.contains("RESTIC_PASSWORD_FILE="), "{}", snapshot);
    assert!(
        snapshot.contains("set -o pipefail"),
        "pipefail is load-bearing — without it a dead tar yields a successful \
         empty snapshot: {}",
        snapshot
    );
}
