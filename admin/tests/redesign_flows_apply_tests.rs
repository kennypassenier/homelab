//! redesign-flows-5 (senior review of the 3.71.0 Deploy all changes, item 2,
//! with the coordinator's destroy constraint of 2026-10-03): the dashboard's
//! Apply takes a chosen subset — the ticked deploys, with the stacks left
//! out and the ones that cannot be planned left alone — and a destroy is its
//! own, separately confirmed step: never beside a deploy, never without its
//! own tick, never without the backup and restore check the host takes
//! first, and only for the stack the host records under that CT number.

mod act_support;

use act_support::spec;
use homelab_admin::core::actions::{self, ActionArgs, HOST_TARGET, Material, commands, validate};
use homelab_admin::core::applyview::{ApplyView, choose};
use homelab_proto::Command;

fn view() -> ApplyView {
    ApplyView {
        deploy: vec!["media".into(), "notes".into(), "web".into()],
        new: vec![],
        unchanged: vec!["home".into()],
        destroy: vec!["drill".into(), "old".into()],
        ephemeral: vec![],
        broken: vec![("bad".into(), "latch failed".into())],
        reasons: Default::default(),
    }
}

fn names(s: &[&str]) -> Vec<String> {
    s.iter().map(|x| x.to_string()).collect()
}

fn vmid(s: &str) -> Option<u16> {
    match s {
        "drill" => Some(903),
        "old" => Some(905),
        _ => None,
    }
}

/// The ticked deploys run; the unticked ones and the stacks that cannot be
/// planned are left alone instead of refusing the whole plan.
#[test]
fn redesign_flows_5_a_subset_deploys_only_the_ticked_stacks() {
    let c = choose(&view(), &names(&["notes", "bad"]), &[], &[], false, vmid).unwrap();
    assert_eq!(c.deploy, names(&["media", "web"]));
    assert!(c.destroy.is_empty());
}

/// Without `leave_out` naming it, a stack that cannot be planned still
/// refuses the whole run (the whole plan, as before).
#[test]
fn redesign_flows_5_a_broken_stack_must_be_left_out_by_name() {
    let e = choose(&view(), &[], &[], &[], false, vmid).unwrap_err();
    assert!(e.contains("bad does not build"), "{e}");
    let e = choose(&view(), &names(&["home"]), &[], &[], false, vmid).unwrap_err();
    assert!(e.contains("home"), "{e}");
}

/// A destroy rides with no deploy: the request that destroys leaves every
/// deploy out, has its own tick, and names each stack's CT number as the
/// host records it.
#[test]
fn redesign_flows_5_a_destroy_is_its_own_confirmed_step() {
    let all_out = names(&["media", "notes", "web", "bad"]);
    // With deploys beside it: refused.
    let e = choose(
        &view(),
        &names(&["bad"]),
        &names(&["drill"]),
        &[903],
        true,
        vmid,
    )
    .unwrap_err();
    assert!(e.contains("its own step"), "{e}");
    // Without the tick: refused.
    let e = choose(&view(), &all_out, &names(&["drill"]), &[903], false, vmid).unwrap_err();
    assert!(e.contains("confirmation"), "{e}");
    // A CT number that is not the one the host records: refused.
    let e = choose(&view(), &all_out, &names(&["drill"]), &[905], true, vmid).unwrap_err();
    assert!(e.contains("CT 903"), "{e}");
    let e = choose(&view(), &all_out, &names(&["drill"]), &[], true, vmid).unwrap_err();
    assert!(e.contains("CT number"), "{e}");
    // A stack the files still declare is never destroyed.
    let e = choose(&view(), &all_out, &names(&["media"]), &[1], true, vmid).unwrap_err();
    assert!(e.contains("not gone"), "{e}");
    // Armed, ticked, the right CT, nothing else: that destroy only.
    let c = choose(&view(), &all_out, &names(&["drill"]), &[903], true, vmid).unwrap();
    assert!(c.deploy.is_empty());
    assert_eq!(c.destroy, names(&["drill"]));
    // Nothing chosen at all: refused, never an empty run.
    let e = choose(&view(), &all_out, &[], &[], false, vmid).unwrap_err();
    assert!(e.contains("nothing"), "{e}");
}

/// The proof: the dashboard's apply never skips the backup and restore
/// check the host takes before a destroy — the form refuses the request,
/// and the command always asks the host for it.
#[test]
fn redesign_flows_5_a_destroy_never_runs_without_its_backup_proof() {
    let r = validate(
        HOST_TARGET,
        "apply",
        ActionArgs {
            destroy: Some("drill".into()),
            destroy_ids: Some("903".into()),
            destroy_ack: true,
            skip_backup: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(r.why.contains("skip_backup"), "{r}");
    // Typed names without the tick, or without their CT numbers: refused
    // before anything is read.
    let r = validate(
        HOST_TARGET,
        "apply",
        ActionArgs {
            destroy: Some("drill".into()),
            destroy_ids: Some("903".into()),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(r.why.contains("confirmation"), "{r}");
    let r = validate(
        HOST_TARGET,
        "apply",
        ActionArgs {
            destroy: Some("drill, old".into()),
            destroy_ids: Some("903".into()),
            destroy_ack: true,
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(r.why.contains("CT number"), "{r}");
    let req = validate(
        HOST_TARGET,
        "apply",
        ActionArgs {
            destroy: Some("drill".into()),
            destroy_ids: Some("903".into()),
            destroy_ack: true,
            leave_out: Some("media, bad".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let cmds = commands(
        &req,
        Material::Apply {
            deploy: vec![],
            destroy: vec!["drill".into()],
        },
    )
    .unwrap();
    assert!(matches!(
        cmds.as_slice(),
        [Command::DestroyRecorded { stack, confirm, skip_backup: false }] if stack == "drill" && confirm == "drill"
    ));
    // A plain subset deploy carries no destroy at all.
    let req = validate(
        HOST_TARGET,
        "apply",
        ActionArgs {
            leave_out: Some("notes".into()),
            ..Default::default()
        },
    )
    .unwrap();
    let cmds = commands(
        &req,
        Material::Apply {
            deploy: vec![spec("media")],
            destroy: vec![],
        },
    )
    .unwrap();
    assert!(
        cmds.iter()
            .all(|c| !matches!(c, Command::DestroyRecorded { .. }))
    );
    assert_eq!(
        actions::cli_override(&req, &Material::None).as_deref(),
        Some("homelab apply --yes")
    );
}
