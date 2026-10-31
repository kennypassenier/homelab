//! Running commands: concurrent writes, timeouts, stalls, the home address and the restore source.

use crate::*;

// restore-two-sources-1: "local" points the settings at the mounted
// local set (and drops its own local_dataset, so a safety copy does not
// copy the repository onto itself); none or "remote" leaves them alone;
// anything else, or "local" without a local set, is refused.
#[tokio::test]
pub(crate) async fn a_restore_source_picks_the_repository_set() {
    use homelab_core::executor::{CmdOutput, MockExecutor};
    use homelab_core::ops::backup::BackupCfg;
    let exec = MockExecutor::new();
    exec.respond_always(
        "zfs list -H -o mounted,mountpoint HDD4TB/restic",
        CmdOutput::ok("yes\t/HDD4TB/restic\n"),
    );
    let base = BackupCfg {
        local_dataset: Some("HDD4TB/restic".into()),
        ..BackupCfg::default()
    };
    let local = crate::backup_cfg_for_source(&exec, base.clone(), Some("local"))
        .await
        .unwrap();
    assert_eq!(local.restic_base, "/HDD4TB/restic");
    assert!(local.local_dataset.is_none());
    let remote = crate::backup_cfg_for_source(&exec, base.clone(), None)
        .await
        .unwrap();
    assert_eq!(remote.restic_base, base.restic_base);
    assert!(
        crate::backup_cfg_for_source(&exec, base, Some("usb"))
            .await
            .is_err()
    );
    assert!(
        crate::backup_cfg_for_source(&exec, BackupCfg::default(), Some("local"))
            .await
            .is_err()
    );
}

// backup-stall (correction 2026-10-22): a command whose status lines
// keep coming while nothing moves is stopped once the watch's limit
// passes; one whose progress keeps moving runs to its end.
#[tokio::test]
pub(crate) async fn a_stalled_streaming_command_is_stopped_and_a_moving_one_is_not() {
    use homelab_core::executor::{Cmd, Executor, Stall};
    let ticking = Cmd::new(
        "sh",
        &["-c", "while true; do echo tick; sleep 0.2; done"],
        60,
    )
    .stall(Stall::new(2));
    let started = std::time::Instant::now();
    let err = crate::RealExecutor
        .run_streaming(&ticking, &|_l: &str| false)
        .await
        .unwrap_err();
    assert!(
        matches!(
            err,
            homelab_core::error::CoreError::Stalled { seconds: 2, .. }
        ),
        "{err:?}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(10));

    let stall = Stall::new(2);
    let moving = Cmd::new(
        "sh",
        &["-c", "for i in 1 2 3 4 5 6; do echo $i; sleep 0.7; done"],
        60,
    )
    .stall(stall.clone());
    let out = crate::RealExecutor
        .run_streaming(&moving, &|_l: &str| {
            stall.progressed();
            false
        })
        .await
        .unwrap();
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout.lines().count(), 6);
}

/// covers: fix-94, fix-156
///
/// The home address is read again after every gateway deploy that
/// finished; another stack's deploy and a failed gateway deploy leave it
/// alone. (Every night and at the host's start it is read regardless.)
#[test]
pub(crate) fn fix_94_the_home_address_follows_gateway_deploys() {
    use crate::home_address_after_deploy;
    assert!(home_address_after_deploy(104, true, 104));
    assert!(!home_address_after_deploy(104, false, 104));
    assert!(!home_address_after_deploy(115, true, 104));
}

/// fix-51 (see REGISTER.md): concurrent writes of one path must all
/// succeed and leave one whole version behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
pub(crate) async fn fix_51_concurrent_writes_of_one_path_all_succeed_and_stay_whole() {
    use homelab_core::executor::Executor;
    let dir = std::env::temp_dir().join(format!("homelab-fix51-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json").to_string_lossy().into_owned();
    for round in 0..20 {
        let writes = (0..8).map(|i| {
            let path = path.clone();
            tokio::spawn(async move {
                let body = format!("{}-{};", round, i).repeat(20_000);
                crate::RealExecutor.write_file(&path, &body, 0o644).await
            })
        });
        for w in writes.collect::<Vec<_>>() {
            w.await
                .unwrap()
                .expect("every concurrent write of one path succeeds");
        }
        let got = std::fs::read_to_string(&path).unwrap();
        let piece = &got[..=got.find(';').expect("content present")];
        assert_eq!(got, piece.repeat(20_000), "one writer's content, whole");
    }
    let leftovers: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n != "state.json")
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(leftovers.is_empty(), "no temp files left: {:?}", leftovers);
}

/// fix-53 (expert panel, timeout-leaves-container-work-running,
/// 2026-09-27): a timeout killed only the direct child (`pct`), not what
/// it had started (`lxc-attach` and the script), so a "timed out" step
/// went on changing the container. Everything the command started must
/// be gone once the timeout is reported.
#[tokio::test]
pub(crate) async fn fix_53_a_timeout_kills_everything_the_command_started() {
    use homelab_core::executor::Executor;
    let pidfile = std::env::temp_dir().join(format!("homelab-fix53-{}.pid", std::process::id()));
    let _ = std::fs::remove_file(&pidfile);
    let script = format!("sleep 30 & echo $! > '{}'; wait", pidfile.display());
    let got = crate::RealExecutor
        .run(&Cmd::new("sh", &["-c", &script], 1))
        .await;
    assert!(
        matches!(got, Err(CoreError::Timeout { .. })),
        "the wait ends as a timeout: {:?}",
        got
    );
    let pid = std::fs::read_to_string(&pidfile)
        .expect("the script recorded its child")
        .trim()
        .to_string();
    let _ = std::fs::remove_file(&pidfile);
    // An exit is not instant; a zombie awaiting its reaper counts as gone.
    let mut alive = true;
    for _ in 0..20 {
        let stat = std::fs::read_to_string(format!("/proc/{}/stat", pid)).unwrap_or_default();
        alive = !stat.is_empty() && !stat.contains(") Z ");
        if !alive {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if alive {
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid])
            .status();
    }
    assert!(
        !alive,
        "the command's own child {} outlived the timeout",
        pid
    );
}
