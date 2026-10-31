//! The fixed step plans of backup and restore.

/// E1: snapshot a stack's /appdata paths, quiescing paused containers.
/// fix-171 round 3: `backup`'s own fixed step plan — unconditional, like
/// `deploy::STEPS` and `destroy::STEPS`: every precondition failure here
/// (an owner conflict, a busy app, a stale declared-empty path) is a
/// command that FAILS the step, and a failed step already aborts the whole
/// operation without needing the rest of the plan marked — the same pattern
/// every other op in this file already follows.
pub const BACKUP_STEPS: &[&str] = &[
    "safety gates",
    "owner conflict",
    "in use?",
    "declared-empty paths",
    "declared paths exist",
    "init repos",
    "clear stale locks",
    "quiesce",
    "snapshot",
    "resume",
    "upload",
    "retention",
];

/// fix-step-plan-nested: `backup`'s own step names, qualified the way they
/// are marked when `backup` runs nested inside another op (`destroy`'s
/// "backup before destroy") — the single source that composer and
/// `admin`'s batch-total both use.
pub fn backup_plan_names(stack_name: &str) -> Vec<String> {
    let prefix = format!("backup-{}", stack_name);
    BACKUP_STEPS
        .iter()
        .map(|s| format!("{prefix} :: {s}"))
        .collect()
}

/// one-process-batch-2: `restore_app`'s step plan, known from `safety_copy`
/// alone (a plain argument of the command). "restore data" is always in the
/// plan, even on the branch where the safety copy fails and it never runs —
/// that branch marks it skipped. The one list the op plans from and the
/// dashboard counts.
pub fn restore_plan_names(safety_copy: bool) -> Vec<String> {
    let mut steps: Vec<&str> = vec!["safety gates", "native units", "validate snapshot"];
    if safety_copy {
        steps.push("room for a safety copy");
    }
    steps.push("quiesce stack");
    if safety_copy {
        steps.push("safety copy");
    }
    steps.push("restore data");
    steps.push("resume stack");
    steps.push("verify health");
    steps.into_iter().map(String::from).collect()
}
