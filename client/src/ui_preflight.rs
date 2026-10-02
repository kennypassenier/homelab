//! fix-185: before `homelab ui` presses confirm on an action that reads the
//! repository (deploy, deploy-commit, apply, …), refuse here rather than let
//! the dashboard find out. Two failures on 2026-10-02 showed why this has to
//! run on the machine that is actually driving, before the dashboard is
//! asked at all:
//!
//! * the dashboard reads the repository through ITS OWN working copy, which
//!   fetches origin — a `confirm` was once sent while local HEAD carried
//!   commits nobody had pushed yet, so the dashboard quietly deployed the
//!   files from before them;
//! * a stack file that does not validate was only found when the dashboard's
//!   own guard refused the press, after the countdown and the dialog had
//!   already run.
//!
//! Pure here: formatting the refusal from facts the caller already
//! gathered (git's own answer, `homelab_core::manifest::validate`'s own
//! errors) — the git call and the spec build stay in `main`, which has the
//! filesystem and the process table.

/// Refuse the press when local HEAD carries commits the upstream does not
/// have: `unpushed` is `git rev-list --oneline @{u}..HEAD`'s lines, newest
/// first, already read by the caller. `None` when there is nothing to
/// refuse (the list is empty — including when it could not be read, which
/// the caller treats as "nothing to report" rather than an unpushed commit).
pub fn unpushed_refusal(unpushed: &[String]) -> Option<String> {
    if unpushed.is_empty() {
        return None;
    }
    Some(format!(
        "local HEAD has {} commit(s) not on its upstream, and the dashboard reads the \
         repository through its OWN working copy (it fetches origin, never this machine), so it \
         would validate and deploy the files from before them: {} :: push first (Kenny's go)",
        unpushed.len(),
        unpushed.join("; ")
    ))
}

/// Refuse the press when a stack involved fails the same validation
/// `homelab apply --plan` runs. `errors`: `(stack, why)`, one per stack that
/// failed; empty when every stack involved validated.
pub fn validation_refusal(errors: &[(String, String)]) -> Option<String> {
    if errors.is_empty() {
        return None;
    }
    Some(format!(
        "{} :: fix the stack file(s) first — this is the same check `homelab apply --plan` runs, \
         run here before the dashboard rather than only after its own guard refuses",
        errors
            .iter()
            .map(|(s, e)| format!("{s}: {e}"))
            .collect::<Vec<_>>()
            .join("; ")
    ))
}

/// The stacks a driven final press acts on: `apply` reads every declared
/// stack (`declared`, as `homelab apply` itself scans them), any other
/// repo-reading form the one comma list the dashboard's `state.form.stack`
/// already names (a single stack, or a batch's list).
pub fn stacks_involved(action: &str, form_stack: &str, declared: &[String]) -> Vec<String> {
    if action == "apply" {
        return declared.to_vec();
    }
    form_stack
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn follow_no_unpushed_commits_and_no_errors_refuse_nothing() {
        assert_eq!(unpushed_refusal(&[]), None);
        assert_eq!(validation_refusal(&[]), None);
    }

    #[test]
    fn follow_unpushed_commits_name_them_and_say_push_first() {
        let why =
            unpushed_refusal(&["abc1234 fix the thing".into(), "def5678 another".into()]).unwrap();
        assert!(why.contains("2 commit(s)"), "{why}");
        assert!(why.contains("abc1234 fix the thing"), "{why}");
        assert!(why.contains("push first (Kenny's go)"), "{why}");
        assert!(why.contains("its OWN working copy"), "{why}");
    }

    #[test]
    fn follow_a_validation_error_names_the_stack_and_points_at_apply_plan() {
        let why = validation_refusal(&[("media".into(), "vmid 0 is not valid".into())]).unwrap();
        assert!(why.contains("media: vmid 0 is not valid"), "{why}");
        assert!(why.contains("homelab apply --plan"), "{why}");
    }

    #[test]
    fn follow_stacks_involved_is_every_declared_stack_for_apply_and_the_form_list_otherwise() {
        let declared = vec!["almanac".into(), "media".into(), "uptime".into()];
        assert_eq!(stacks_involved("apply", "_host", &declared), declared);
        assert_eq!(
            stacks_involved("deploy", "media", &declared),
            vec!["media".to_string()]
        );
        assert_eq!(
            stacks_involved("deploy", "media, uptime", &declared),
            vec!["media".to_string(), "uptime".to_string()]
        );
        assert_eq!(
            stacks_involved("deploy-commit", "", &declared),
            Vec::<String>::new()
        );
    }
}
