//! Daemon supervision: a dead scheduler ends the daemon, SIGTERM waits for the running operation.

use crate::*;

/// fix-52 (see REGISTER.md): a dead scheduler must end the process with
/// a failure, so systemd restarts it with a live one.
#[tokio::test]
pub(crate) async fn fix_52_a_dead_scheduler_ends_the_daemon_with_a_failure() {
    let scheduler = tokio::spawn(async { panic!("scheduler bug") });
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        crate::supervise(
            std::future::pending::<std::io::Result<()>>(),
            scheduler,
            std::future::pending::<()>(),
            Arc::new(Mutex::new(())),
            Duration::from_secs(1),
        ),
    )
    .await
    .expect("supervision ends when the scheduler dies");
    assert_ne!(code, 0, "and it ends as a failure");
}

/// fix-52 and adoption norm N1: SIGTERM lets the running operation finish
/// (up to a bound) and then exits 0, instead of killing a step mid-way.
#[tokio::test]
pub(crate) async fn fix_52_sigterm_waits_for_the_running_operation_then_exits_zero() {
    let op_lock = Arc::new(Mutex::new(()));
    let held = op_lock.clone().lock_owned().await;
    let finished = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let f2 = finished.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        f2.store(true, std::sync::atomic::Ordering::SeqCst);
        drop(held);
    });
    let scheduler = tokio::spawn(std::future::pending::<()>());
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        crate::supervise(
            std::future::pending::<std::io::Result<()>>(),
            scheduler,
            std::future::ready(()),
            op_lock.clone(),
            Duration::from_secs(3),
        ),
    )
    .await
    .expect("supervision ends on SIGTERM");
    assert_eq!(code, 0);
    assert!(
        finished.load(std::sync::atomic::Ordering::SeqCst),
        "it waited for the operation holding the lock"
    );
    // Having waited, it keeps the lock: nothing queued after the signal
    // starts before the process exits.
    assert!(op_lock.try_lock().is_err(), "the lock stays held");
    // A hung operation does not hold the exit forever.
    let op_lock = Arc::new(Mutex::new(()));
    let _stuck = op_lock.clone().lock_owned().await;
    let scheduler = tokio::spawn(std::future::pending::<()>());
    let code = tokio::time::timeout(
        Duration::from_secs(5),
        crate::supervise(
            std::future::pending::<std::io::Result<()>>(),
            scheduler,
            std::future::ready(()),
            op_lock,
            Duration::from_millis(100),
        ),
    )
    .await
    .expect("the drain wait is bounded");
    assert_eq!(code, 0);
}

/// fix-52 residual (2026-10-01): the 10 s ping only ever proved that its
/// own loop was still scheduled, not that the scheduler was making any
/// progress — a nightly round wedged inside one `await` (not a panic,
/// which the test above already covers) kept being fed forever. Now the
/// ping is withheld once the scheduler's heartbeat is older than
/// `SCHEDULER_WATCHDOG_STALE_S`, so systemd's `WatchdogSec` eventually
/// restarts a daemon whose scheduler stopped making progress.
#[test]
pub(crate) fn fix_52_residual_a_stale_scheduler_heartbeat_withholds_the_watchdog_ping() {
    let stale_after = crate::SCHEDULER_WATCHDOG_STALE_S;
    // A heartbeat from a few seconds ago: still alive.
    assert!(crate::scheduler_is_alive(1_000_000, 999_990, stale_after));
    // Exactly at the edge still counts.
    assert!(crate::scheduler_is_alive(
        1_000_000,
        1_000_000 - stale_after,
        stale_after
    ));
    // One second past the edge: the scheduler has made no progress in
    // longer than the tolerance, so the ping must stop.
    assert!(!crate::scheduler_is_alive(
        1_000_000,
        1_000_000 - stale_after - 1,
        stale_after
    ));
    // A heartbeat from "the future" (clock skew on a fresh AppState,
    // or `now` ticking backwards) never reads as stale.
    assert!(crate::scheduler_is_alive(1_000_000, 1_000_100, stale_after));
}
