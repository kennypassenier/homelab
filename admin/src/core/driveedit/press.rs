//! A press in an edit form: the checks every press passes, then the dialog
//! on top, or the step's own button.

use super::*;

/// Hold a step: the errors go on the form, the first one is the refusal.
pub(super) fn hold(form: &mut OpenForm, what: &str, errors: BTreeMap<String, String>) -> Applied {
    let r = held_for(form, what, &errors);
    form.errors = errors;
    Applied {
        effect: Effect::None,
        held: Some(r),
    }
}

/// `hold`, boxed for the `Result`s that carry it.
pub(super) fn hold_b(
    form: &mut OpenForm,
    what: &str,
    errors: BTreeMap<String, String>,
) -> Box<Applied> {
    Box::new(hold(form, what, errors))
}

/// The shell runs this call next.
pub(super) fn call(call: EditCall) -> Applied {
    Applied {
        effect: Effect::Edit(call),
        held: None,
    }
}

/// The edit goes to the plan page: remember its body and ask for the plan.
pub(super) fn to_plan(form: &mut OpenForm, body: &Value, action: Option<&str>) {
    form.errors.clear();
    form.step_index = 1;
    let e = edit(form);
    e.body = Some(body.clone());
    if let Some(a) = action {
        e.action = Some(a.into());
    }
    e.plan = None;
}

/// Whether the kind ends in a plan page and a commit page.
fn has_plan(kind: EditKind) -> bool {
    kind == EditKind::Import || kind.preset_edit() || kind.stack_edit()
}

fn checked_press(
    form: &OpenForm,
    step: &UiStep,
    kind: EditKind,
    button: &str,
    cx: &Ctx,
) -> Result<(), Refusal> {
    if form.sent() {
        return Err(refused(
            step,
            "this form was sent already; one press runs once",
            "homelab ui state shows what it did; close the dialog to start over",
        ));
    }
    if !form.buttons.iter().any(|b| b == button) {
        return Err(refused(
            step,
            format!("{button} is not on screen on the step {}", form.step),
            format!("the buttons on screen are: {}", form.buttons.join(", ")),
        ));
    }
    if kind.scope() > cx.scope {
        return Err(refused(
            step,
            format!("{} needs scope {:?}", kind.slug(), kind.scope()),
            "leave this press to Kenny",
        ));
    }
    Ok(())
}

/// A press in an edit form.
pub fn press(
    form: &mut OpenForm,
    step: &UiStep,
    button: &str,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    let Family::Edit(kind) = form.desc.family else {
        return Err(refused(step, "not an edit form", "homelab ui state"));
    };
    checked_press(form, step, kind, button, cx)?;
    // A dialog on top first.
    if let Some(sub) = form.edit.as_ref().and_then(|e| e.sub.clone()) {
        return press_dialog(form, &sub, button);
    }
    if button == "back" {
        form.step_index = form.step_index.saturating_sub(1);
        form.errors.clear();
        form.run_error = None;
        return Ok(plain());
    }
    press_step(form, step, kind, button, cx)
}

fn press_step(
    form: &mut OpenForm,
    step: &UiStep,
    kind: EditKind,
    button: &str,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    let step_id = form.step.clone();
    match (kind, step_id.as_str(), button) {
        (k, "plan", "next") if has_plan(k) => {
            form.step_index += 1;
            Ok(plain())
        }
        (k, "commit", "confirm") if has_plan(k) => Ok(confirm_commit(form, kind)),
        (EditKind::Import, "bundle", "next") => Ok(import_bundle(form)),
        (EditKind::Preset | EditKind::NewPreset, "meta", "next") => {
            Ok(preset_meta_next(form, kind))
        }
        (
            EditKind::Preset,
            "meta",
            b @ ("save-file" | "rename-file" | "delete-file" | "remove-preset"),
        ) => Ok(preset_file_press(form, b)),
        (EditKind::Native, _, "remove") => Ok(native_remove(form)),
        (k, _, "next") if k.stack_edit() => Ok(plan_press(form, k)),
        (EditKind::NewStack, "plan", "confirm") => Ok(new_stack_confirm(form)),
        (EditKind::NewStack, _, "next") => Ok(new_stack_next(form)),
        (EditKind::HostSettings, "keys", "next") => {
            form.step_index = 1;
            Ok(plain())
        }
        (EditKind::HostSettings, "review", "confirm") => Ok(host_write(form)),
        (EditKind::Batch, _, "confirm") => Ok(batch_confirm(form)),
        (EditKind::Rollback, _, "next") => rollback_next(form, step, cx),
        _ => Err(refused(
            step,
            format!("{button} does nothing on the step {step_id}"),
            format!("the buttons on screen are: {}", form.buttons.join(", ")),
        )),
    }
}
