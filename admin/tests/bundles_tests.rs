//! bundle-release-rollout, bundle-hosttoml-deploys, bundle-secret-restart
//! (Kenny, 2026-10-24, release form: "Nu in 4.1.1"): three jobs of several
//! parts. What the catalog and the request check promise; the parts run on
//! the real host (a GitHub release and a latch stage cannot be faked here).

use homelab_admin::core::actions::{ActionArgs, ActionKind, catalog, validate};
use homelab_proto::Scope;

#[test]
fn bundle_release_rollout_is_a_host_action_with_a_tag() {
    let e = catalog()
        .into_iter()
        .find(|e| e.action == ActionKind::RollOutRelease)
        .expect("Roll out a release is in the catalog");
    assert_eq!(e.target, "host");
    assert_eq!(ActionKind::RollOutRelease.scope(), Scope::All);
    let ok = validate(
        "_host",
        "roll-out-release",
        ActionArgs {
            tag: Some("v4.1.1".into()),
            ..Default::default()
        },
    );
    assert!(ok.is_ok(), "{ok:?}");
    assert!(
        validate("media", "roll-out-release", ActionArgs::default()).is_err(),
        "host-wide only"
    );
}

#[test]
fn bundle_hosttoml_deploys_restart_host_takes_then_apply() {
    let r = validate(
        "_host",
        "restart-host",
        ActionArgs {
            then_apply: true,
            ..Default::default()
        },
    )
    .expect("restart-host accepts then_apply");
    assert!(r.args.then_apply);
    assert!(
        validate(
            "media",
            "deploy",
            ActionArgs {
                then_apply: true,
                ..Default::default()
            }
        )
        .is_err(),
        "only restart-host takes then_apply"
    );
}

#[test]
fn bundle_secret_restart_change_secret_takes_then_deploy() {
    assert!(
        ActionKind::ChangeSecret
            .args()
            .contains(&homelab_admin::core::actions::Arg::ThenDeploy)
    );
    assert!(
        validate(
            "media",
            "backup",
            ActionArgs {
                then_deploy: true,
                ..Default::default()
            }
        )
        .is_err(),
        "only change-secret takes then_deploy"
    );
}
