//! Presses of the new-stack wizard, host.toml, the batch and the roll back
//! dialog.

use super::*;

pub(super) fn new_stack_confirm(form: &mut OpenForm) -> Applied {
    let subject = text_of(form.values.get("subject")).trim().to_string();
    if subject.is_empty() {
        let errors = BTreeMap::from([("subject".to_string(), say("subject", &[]))]);
        return hold(form, "confirm", errors);
    }
    let follow = match form.values.get("follow") {
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => "none".into(),
    };
    let mut body = Map::new();
    body.insert("stack".into(), new_stack_body(&form.values));
    body.insert("subject".into(), json!(subject));
    let note = text_of(form.values.get("note")).trim().to_string();
    if !note.is_empty() {
        body.insert("note".into(), json!(note));
    }
    if follow != "none" {
        body.insert("follow".into(), json!(follow));
    }
    form.run_error = None;
    call(EditCall::NewCommit {
        body: Value::Object(body),
    })
}

pub(super) fn new_stack_next(form: &mut OpenForm) -> Applied {
    let at = form.desc.steps[form.step_index].clone();
    let names = strings(&edit(form).data["taken"]["names"]);
    let vmids = taken_vmids(&edit(form).data);
    let errors = check_new_step(&at, &form.values, &names, &vmids);
    if !errors.is_empty() {
        return hold(form, "next", errors);
    }
    form.errors.clear();
    form.step_index += 1;
    let next = form.desc.steps[form.step_index].id.clone();
    let effect = match next.as_str() {
        "data" => Effect::Edit(EditCall::Appdata {
            body: json!({
                "preset": text_of(form.values.get("preset")),
                "name": text_of(form.values.get("name")).trim(),
                "vmid": js_number(&text_of(form.values.get("vmid"))),
            }),
        }),
        "plan" => {
            edit(form).plan = None;
            Effect::Edit(EditCall::NewPlan {
                body: new_stack_body(&form.values),
            })
        }
        _ => Effect::None,
    };
    Applied { effect, held: None }
}

pub(super) fn host_write(form: &mut OpenForm) -> Applied {
    let e = edit(form);
    let fields = e.data["page"]["fields"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let body = host_settings_body(
        e.data["page"]["sha256"].as_str().unwrap_or(""),
        &fields,
        &e.staged,
        &e.confirmed,
    );
    form.run_error = None;
    call(EditCall::HostWrite { body })
}

/// The batch dialog's typed stack names that do not match.
fn confirm_errors(form: &OpenForm, fields: &[Field]) -> BTreeMap<String, String> {
    let mut errors = BTreeMap::new();
    for f in fields.iter().filter(|f| f.name.starts_with("confirm:")) {
        let s = &f.name["confirm:".len()..];
        if text_of(form.values.get(&f.name)).trim() != s {
            errors.insert(f.name.clone(), say("batch_confirm", &[("stack", s)]));
        }
    }
    errors
}

/// The action's arguments and the typed confirmations, from the values.
fn batch_args(form: &OpenForm, fields: &[Field]) -> (Map<String, Value>, Map<String, Value>) {
    let mut args = Map::new();
    let mut confirms = Map::new();
    for f in fields {
        let v = form.values.get(&f.name);
        let t = text_of(v).trim().to_string();
        if let Some(s) = f.name.strip_prefix("confirm:") {
            if !t.is_empty() {
                confirms.insert(s.to_string(), json!(t));
            }
        } else if f.kind == FieldKind::Check {
            if v == Some(&Value::Bool(true)) {
                args.insert(f.name.clone(), json!(true));
            }
        } else if !t.is_empty() {
            args.insert(f.name.clone(), json!(t));
        }
    }
    (args, confirms)
}

pub(super) fn batch_confirm(form: &mut OpenForm) -> Applied {
    let stacks = edit(form).stacks.clone();
    let action = edit(form).batch_action.clone().unwrap_or_default();
    let fields: Vec<Field> = form.desc.fields().cloned().collect();
    let errors = confirm_errors(form, &fields);
    if !errors.is_empty() {
        return hold(form, "confirm", errors);
    }
    let (args, confirms) = batch_args(form, &fields);
    let mut body = Map::new();
    body.insert("action".into(), json!(action));
    body.insert("stacks".into(), json!(stacks));
    body.insert("args".into(), Value::Object(args));
    if fields.iter().any(|f| f.name.starts_with("confirm:")) {
        body.insert("confirms".into(), Value::Object(confirms));
    }
    form.run_error = None;
    call(EditCall::Batch {
        body: Value::Object(body),
    })
}

/// The roll back dialog hands over to the action's own dialog.
pub(super) fn rollback_next(
    form: &mut OpenForm,
    step: &UiStep,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    let commit = text_of(form.values.get("commit"));
    let unit = text_of(form.values.get("unit"));
    let (action, preset) = if !commit.is_empty() {
        (ActionKind::DeployCommit, ("commit", commit))
    } else if !unit.is_empty() {
        (ActionKind::RollbackNative, ("unit", unit))
    } else {
        return Ok(held_msg("next", &say("rollback_choose", &[])));
    };
    if action.scope() > cx.scope {
        return Err(refused(
            step,
            format!("{} needs scope {:?}", action.slug(), action.scope()),
            "leave it to Kenny",
        ));
    }
    let data = edit(form).data.clone();
    let sources = Sources {
        units: strings(&data["native_units"]),
        commits: data["commits"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["commit"].as_str().map(str::to_string))
            .collect(),
        ..Sources::default()
    };
    let mut next = OpenForm::new(drive::action_form(action, &form.stack), &sources);
    next.values.insert(preset.0.into(), json!(preset.1));
    next.refresh();
    let effect = if next.step == drive::REVIEW {
        Effect::Preview
    } else {
        Effect::None
    };
    *form = next;
    Ok(Applied { effect, held: None })
}
