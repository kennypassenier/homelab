//! What the shell's call answered, into the form.

use super::*;

/// The plan in short, for the tabs and `homelab ui state`.
pub fn plan_summary(p: &Value) -> Value {
    let files: Vec<Value> = p["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            json!({ "path": f["path"], "status": f["status"], "added": f["added"], "removed": f["removed"] })
        })
        .collect();
    let effects: Vec<Value> = p["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| e["what"].clone())
        .collect();
    json!({
        "valid": p["valid"],
        "unchanged": p["unchanged"],
        "problems": p["problems"],
        "subject": p["subject"],
        "follow_ups": p["follow_ups"],
        "files": files,
        "effects": effects,
        "restarts_dashboard": p["restarts_dashboard"],
    })
}

fn is_plan_call(call: &EditCall) -> bool {
    matches!(
        call,
        EditCall::Plan { .. }
            | EditCall::NewPlan { .. }
            | EditCall::ImportPlan { .. }
            | EditCall::PresetPlan { .. }
    )
}

/// A plan came back: the commit page (or the new-stack plan page) gets its
/// fields.
fn plan_arrived(form: &mut OpenForm, call: &EditCall, plan: Value) {
    let follow_ups = strings(&plan["follow_ups"]);
    let fields = commit_fields(&follow_ups, plan["subject"].as_str().unwrap_or(""));
    // A preset's own steps are the same `plan_steps()` shape a stack edit's
    // are (`[first, "plan", "commit"]`), so its plan lands on "commit" the
    // same way `EditCall::Plan`'s does.
    let at = if matches!(call, EditCall::NewPlan { .. }) {
        "plan"
    } else {
        "commit"
    };
    for f in &fields {
        form.values.insert(f.name.clone(), drive::start_value(f));
    }
    if let Some(s) = form.desc.steps.iter_mut().find(|s| s.id == at) {
        s.fields = fields;
    }
    form.run_error = None;
    edit(form).plan = Some(plan_summary(&plan));
}

/// The appdata answer: the data folders the wizard offers to leave out.
fn appdata_arrived(form: &mut OpenForm, outcome: Result<Value, Refusal>) {
    let paths = outcome.map(|v| strings(&v["appdata"])).unwrap_or_default();
    let fields = data_fields(&paths);
    form.values.retain(|k, _| {
        !k.starts_with("nodata:") || paths.iter().any(|p| k == &format!("nodata:{p}"))
    });
    for f in &fields {
        form.values
            .entry(f.name.clone())
            .or_insert(Value::Bool(false));
    }
    if let Some(s) = form.desc.steps.iter_mut().find(|s| s.id == "data") {
        s.fields = fields;
    }
}

/// What the shell's call answered, into the form.
pub fn done(form: &mut OpenForm, call: &EditCall, outcome: Result<Value, Refusal>) {
    match (call, outcome) {
        (c, Ok(plan)) if is_plan_call(c) => plan_arrived(form, c, plan),
        (c, Err(r)) if is_plan_call(c) => {
            edit(form).plan = None;
            form.run_error = Some(r);
        }
        (EditCall::Appdata { .. }, outcome) => appdata_arrived(form, outcome),
        (_, Ok(v)) => {
            form.run_error = None;
            edit(form).result = Some(v);
        }
        (_, Err(r)) => form.run_error = Some(r),
    }
    form.refresh();
}
