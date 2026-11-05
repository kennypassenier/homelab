//! test-round defect: `docker-prune.service` failed with 203/EXEC on every
//! run on native-only CTs (kyu CT 109, almanac CT 112), which have no docker.
//! The guard already skipped installing it there, but a unit installed by an
//! older release stayed and kept failing. Guards on a container without
//! docker now remove the unit and the timer, so a deploy clears the failure.

use homelab_core::executor::MockExecutor;
use homelab_core::sink::VecSink;

#[tokio::test]
async fn a_native_only_ct_gets_no_prune_unit_and_loses_an_existing_one() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    homelab_core::ops::guards::apply(&exec, &sink, 109, false, None)
        .await
        .unwrap();
    // Nothing is written for the prune service or timer.
    assert!(
        exec.calls_containing("systemd/system/docker-prune.service")
            .iter()
            .all(|c| !c.contains("cat >") && !c.contains("pct push")),
        "{:?}",
        exec.calls_containing("docker-prune")
    );
    // One step stops the timer, removes both unit files, clears the failed state.
    let retire = exec.calls_containing("rm -f /etc/systemd/system/docker-prune.service");
    assert_eq!(
        retire.len(),
        1,
        "{:?}",
        exec.calls_containing("docker-prune")
    );
    let cmd = &retire[0];
    assert!(cmd.contains("docker-prune.timer"), "{cmd}");
    assert!(cmd.contains("disable --now docker-prune.timer"), "{cmd}");
    assert!(cmd.contains("reset-failed docker-prune.service"), "{cmd}");
}

#[tokio::test]
async fn a_docker_ct_keeps_the_prune_timer_and_is_not_retired() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    homelab_core::ops::guards::apply(&exec, &sink, 106, true, None)
        .await
        .unwrap();
    assert!(
        exec.calls_containing("rm -f /etc/systemd/system/docker-prune.service")
            .is_empty()
    );
    assert!(
        !exec
            .calls_containing("enable --now docker-prune.timer")
            .is_empty()
    );
}
