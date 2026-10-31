//! The deploy's fixed step plan.

use crate::manifest::DeploySpec;

/// fix-171 round 2: deploy's own step plan — fixed, EVERY deploy, regardless
/// of how many apps the manifest declares, whether it has a firewall, gives
/// a gateway route, or needs a log shipper. Every step here is already
/// invoked unconditionally and no-ops when its precondition does not hold
/// (`baseline` when the container does not exist yet, `native units` when
/// the manifest declares none); the six that used to be invoked only
/// conditionally (`firewall`, `firewall after create`,
/// `refresh the tile watcher's firewall`, `gateway route`,
/// `retire gateway route`, `log shipper`) now take an explicit `skip` mark
/// in their `else` branch instead of silently not appearing — so `n`
/// reaches this list's length exactly, every time, and the dashboard's
/// total is this constant rather than a guess from a past run's count
/// (fix-171's first attempt, rejected for exactly that reason). Shared with
/// the dashboard (`admin::shell::actions::execute`) so Apply and a batch of
/// several deploys can announce the combined total before the first one
/// starts.
pub const STEPS: &[&str] = &[
    "validate",
    "safety gates",
    "baseline",
    "registry cache",
    "hardware readiness",
    "host storage",
    "auto-restore check",
    "metrics discovery",
    "firewall",
    "provision container",
    "firewall after create",
    "refresh the tile watcher's firewall",
    "wait for systemd",
    "bootstrap docker",
    "runaway guards",
    "log rotation",
    "retire dropped",
    "commit intent",
    "push files",
    "start apps",
    "storage ownership",
    "verify health",
    "gateway route",
    "retire gateway route",
    "orphan files",
    "garbage collect",
    "log shipper",
    "native units",
    "record state",
    "reconcile",
    "app settings",
    "service checks",
];

/// redesign-stacks-6: the deploy's whole plan for `spec` — [`STEPS`], with
/// the nested `backup` op's names right after "safety gates" when the spec
/// asks for a backup first (`DeploySpec::backup_first`). Still fixed per
/// spec: a stack with nothing to back up skip-marks those names rather than
/// leaving them out. The single source `deploy` plans from and the
/// dashboard's batch total sums.
pub fn plan_names(spec: &DeploySpec) -> Vec<String> {
    let mut v: Vec<String> = Vec::with_capacity(STEPS.len() + 8);
    for s in STEPS {
        v.push(s.to_string());
        if *s == "safety gates" && spec.backup_first {
            v.extend(crate::ops::backup::backup_plan_names(
                &spec.manifest.stack_name,
            ));
        }
    }
    v
}
