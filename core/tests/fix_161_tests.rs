//! fix-161: the guard that installs unattended-upgrades ignored the exit
//! code of `apt-get install`. On CT 118 (inbox) the first deploy on
//! 2026-09-28 06:09 ran it against package lists from 2025-10-01, the
//! install failed, and the deploy said "security patching in place" while
//! the package was never installed (measured read-only 2026-09-29 15:17).

use homelab_core::executor::{CmdOutput, MockExecutor};
use homelab_core::sink::VecSink;

#[tokio::test]
async fn fix_161_the_package_lists_are_refreshed_before_unattended_upgrades_is_installed() {
    let exec = MockExecutor::new();
    let sink = VecSink::new();
    homelab_core::ops::guards::apply(&exec, &sink, 118, false, None)
        .await
        .unwrap();
    let install = exec.calls_containing("apt-get install -y -qq unattended-upgrades");
    assert_eq!(install.len(), 1, "{:?}", install);
    let s = &install[0];
    let update = s
        .find("apt-get update")
        .expect("apt-get update in the same step");
    assert!(update < s.find("apt-get install").unwrap(), "{}", s);
}

#[tokio::test]
async fn fix_161_a_failed_install_fails_the_guard_step() {
    let exec = MockExecutor::new();
    exec.respond_always(
        "unattended-upgrades)",
        CmdOutput::failed(100, "E: Failed to fetch … 404 Not Found"),
    );
    let sink = VecSink::new();
    let r = homelab_core::ops::guards::apply(&exec, &sink, 118, false, None).await;
    let e = r.expect_err("a failed install is not \"security patching in place\"");
    assert!(format!("{}", e).contains("unattended-upgrades"), "{}", e);
}

#[tokio::test]
async fn fix_161_every_package_the_guard_installs_is_checked() {
    for pkg in ["logrotate", "sqlite3", "unattended-upgrades"] {
        let exec = MockExecutor::new();
        exec.respond_always(
            &format!("apt-get install -y -qq {pkg})"),
            CmdOutput::failed(100, "E: 404 Not Found"),
        );
        let sink = VecSink::new();
        let r = homelab_core::ops::guards::apply(&exec, &sink, 118, false, None).await;
        let e = r.expect_err(pkg);
        assert!(format!("{}", e).contains(pkg), "{pkg}: {e}");
    }
}
