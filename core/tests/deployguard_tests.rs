//! arch-deploy-guard: a deploy may not silently undo another writer's deploy.

use homelab_core::ops::deployguard::{applied_commit, decide, Ancestry};

#[test]
fn arch_deploy_guard_reads_the_commit_from_the_summary() {
    assert_eq!(applied_commit("a1b2c3d4e5f6"), Some("a1b2c3d4e5f6"));
    assert_eq!(
        applied_commit("a1b2c3d4e5f6 + 2 uncommitted file(s)"),
        Some("a1b2c3d4e5f6")
    );
    assert_eq!(applied_commit("unknown"), None);
    assert_eq!(applied_commit(""), None);
}

#[test]
fn arch_deploy_guard_refuses_a_deploy_that_would_undo_another() {
    let s = Some("a1b2c3d4e5f6");
    assert!(decide("kp-soft", s, Ancestry::Contained, false).is_ok());
    let e = decide("kp-soft", s, Ancestry::Diverged, false).unwrap_err();
    assert!(
        e.contains("a1b2c3d4e5f6") && e.contains("git pull") && e.contains("--force"),
        "{e}"
    );
    let e = decide("kp-soft", s, Ancestry::Unknown, false).unwrap_err();
    assert!(e.contains("does not have commit"), "{e}");
    assert!(
        decide("kp-soft", s, Ancestry::Diverged, true).is_ok(),
        "--force deploys anyway"
    );
    assert!(
        decide("kp-soft", None, Ancestry::Unknown, false).is_ok(),
        "never deployed with a source"
    );
}
