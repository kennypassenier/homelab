//! The test double itself (expert panel 2026-09-27,
//! mock-executor-weak-assertions): a negative assertion must not pass merely
//! because a command is rendered differently, and a scripted answer that no
//! longer matches anything must be visible.

use homelab_core::executor::{pct_sh, Cmd, CmdOutput, Executor, MockExecutor};

/// `calls_containing("pct set").is_empty()` passes for `pct  set` in a
/// host-side `sh -c` script. `ran` reads the recorded program and arguments
/// (and the words of a script), so it sees all three forms.
#[tokio::test]
async fn ran_sees_a_verb_however_it_is_rendered() {
    let exec = MockExecutor::new();
    exec.run(&Cmd::new(
        "/usr/sbin/pct",
        &["set", "110", "-onboot", "1"],
        30,
    ))
    .await
    .unwrap();
    exec.run(&Cmd::new("sh", &["-c", "pct  set 110 -onboot 1"], 30))
        .await
        .unwrap();
    exec.run(&Cmd::new("pct", &["set", "110", "-cores", "2"], 30))
        .await
        .unwrap();
    assert_eq!(exec.calls_containing("pct set").len(), 2, "the weak form");
    assert_eq!(exec.ran("pct", &["set"]), 3);
    assert_eq!(exec.ran("pct", &["set", "110"]), 3);
    // The negative twin: another verb or another vmid is not counted.
    assert_eq!(exec.ran("pct", &["create"]), 0);
    assert_eq!(exec.ran("pct", &["set", "111"]), 0);
}

/// Inside a container script the words after `sh -c` are what ran.
#[tokio::test]
async fn ran_reads_the_script_a_container_was_given() {
    let exec = MockExecutor::new();
    pct_sh(&exec, 110, "cd '/opt/s/app' && docker compose up -d", 60)
        .await
        .unwrap();
    assert_eq!(exec.ran("docker", &["compose", "up"]), 1);
    assert_eq!(exec.ran("pct", &["exec", "110"]), 1);
    assert_eq!(exec.ran("docker", &["compose", "down"]), 0);
}

/// A rule nothing matched used to fall back to "success, empty output"
/// without a word. `unused_rules` names it.
#[tokio::test]
async fn a_rule_that_never_matched_is_reported() {
    let exec = MockExecutor::new();
    exec.respond_always("pct sett", CmdOutput::ok("typo"));
    exec.respond_always("pct set", CmdOutput::ok(""));
    exec.enqueue("pct destroy", CmdOutput::ok(""));
    exec.run(&Cmd::new("pct", &["set", "110"], 30))
        .await
        .unwrap();
    assert_eq!(
        exec.unused_rules(),
        vec!["pct sett".to_string(), "pct destroy".to_string()]
    );
}

/// A strict mock fails the test that leaves a rule unused.
#[tokio::test]
#[should_panic(expected = "never matched")]
async fn a_strict_mock_fails_on_a_rule_that_never_matched() {
    let exec = MockExecutor::strict();
    exec.respond_always("pct sett", CmdOutput::ok(""));
    exec.run(&Cmd::new("pct", &["set", "110"], 30))
        .await
        .unwrap();
}

/// The positive twin: a strict mock whose every rule matched is quiet.
#[tokio::test]
async fn a_strict_mock_whose_rules_all_matched_passes() {
    let exec = MockExecutor::strict();
    exec.respond_always("pct set", CmdOutput::ok(""));
    exec.run(&Cmd::new("pct", &["set", "110"], 30))
        .await
        .unwrap();
}

/// The compose model found the app directory as the text between the first
/// two single quotes, so it depended on how deploy.rs happened to quote. It
/// now reads the `cd` word, quoted or not.
#[tokio::test]
async fn the_compose_model_reads_the_directory_however_it_is_quoted() {
    let exec = MockExecutor::new();
    pct_sh(&exec, 110, "cd /opt/s/alpha && docker compose up -d", 60)
        .await
        .unwrap();
    pct_sh(&exec, 110, "cd '/opt/s/beta' && docker compose up -d", 60)
        .await
        .unwrap();
    let ps = exec
        .run(&Cmd::new("docker", &["ps", "--format", "{{.Names}}"], 30))
        .await
        .unwrap();
    assert_eq!(ps.stdout, "alpha\nbeta\n");
}
