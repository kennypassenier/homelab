//! Presses of the import, preset and commit pages.

use super::*;

fn one_error(name: &str, why: String) -> BTreeMap<String, String> {
    BTreeMap::from([(name.to_string(), why)])
}

/// The commit page's "confirm": the subject is needed, then the commit.
pub(super) fn confirm_commit(form: &mut OpenForm, kind: EditKind) -> Applied {
    if text_of(form.values.get("subject")).trim().is_empty() {
        let fields: Vec<Field> = form.desc.steps[2].fields.clone();
        let mut errors = check_fields(&fields, &form.values);
        errors.insert("subject".into(), say("subject", &[]));
        return hold(form, "confirm", errors);
    }
    let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
    let body = commit_body(&edit_body, &form.values);
    form.run_error = None;
    call(match kind {
        EditKind::Import => EditCall::ImportCommit { body },
        k if k.preset_edit() => EditCall::PresetCommit { body },
        _ => EditCall::Commit {
            stack: form.stack.clone(),
            body,
        },
    })
}

pub(super) fn import_bundle(form: &mut OpenForm) -> Applied {
    let mut at = form.desc.steps[0].clone();
    // The name and the number are checked as the new-stack wizard checks
    // its identity step.
    at.id = "identity".into();
    let names = strings(&edit(form).data["taken"]["names"]);
    let vmids = taken_vmids(&edit(form).data);
    let errors = check_new_step(&at, &form.values, &names, &vmids);
    if !errors.is_empty() {
        return hold(form, "next", errors);
    }
    let body = json!({
        "bundle": text_of(form.values.get("bundle")),
        "name": text_of(form.values.get("name")).trim(),
        "vmid": js_number(&text_of(form.values.get("vmid"))),
    });
    to_plan(form, &body, None);
    call(EditCall::ImportPlan { body })
}

/// The container numbers already taken.
pub(super) fn taken_vmids(data: &Value) -> Vec<i64> {
    data["taken"]["vmids"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_i64)
        .collect()
}

pub(super) fn preset_meta_next(form: &mut OpenForm, kind: EditKind) -> Applied {
    let fields: Vec<Field> = form.desc.steps[0].fields.clone();
    let (name, unprivileged) = if kind == EditKind::Preset {
        let unprivileged = edit(form).data["meta"]["unprivileged"].clone();
        (form.stack.clone(), unprivileged)
    } else {
        let name = text_of(form.values.get("name")).trim().to_string();
        if name.is_empty() {
            let errors = one_error("name", say("needed", &[("label", "Preset name")]));
            return hold(form, "next", errors);
        }
        (name, Value::Null)
    };
    let meta = preset_meta_body(&fields, &form.values, unprivileged);
    let body = json!({ "kind": "meta", "name": name, "meta": meta });
    to_plan(form, &body, None);
    call(EditCall::PresetPlan { edit: body })
}

fn value_of(form: &OpenForm, name: &str) -> String {
    text_of(form.values.get(name)).trim().to_string()
}

/// The body of one of the Files card's buttons, or the held answer.
fn preset_file_body(form: &mut OpenForm, button: &str) -> Result<Value, Box<Applied>> {
    let name = form.stack.clone();
    match button {
        "save-file" => {
            let path = value_of(form, "path");
            if path.is_empty() {
                let errors = one_error("path", say("needed", &[("label", "Path")]));
                return Err(hold_b(form, button, errors));
            }
            let content = text_of(form.values.get("text"));
            Ok(json!({ "kind": "file", "name": name, "path": path, "content": content }))
        }
        "rename-file" => {
            let (from, to) = (value_of(form, "file"), value_of(form, "rename_to"));
            let mut errors = BTreeMap::new();
            if from.is_empty() {
                errors.insert("file".into(), "pick the file to rename first".into());
            }
            if to.is_empty() {
                errors.insert("rename_to".into(), say("needed", &[("label", "New path")]));
            }
            if !errors.is_empty() {
                return Err(hold_b(form, button, errors));
            }
            Ok(json!({ "kind": "rename_file", "name": name, "from": from, "to": to }))
        }
        "delete-file" => {
            let path = value_of(form, "file");
            if path.is_empty() {
                let errors = one_error("file", "pick the file to delete first".into());
                return Err(hold_b(form, button, errors));
            }
            Ok(json!({ "kind": "remove_file", "name": name, "path": path }))
        }
        _ => Ok(json!({ "kind": "remove", "name": name })),
    }
}

pub(super) fn preset_file_press(form: &mut OpenForm, button: &str) -> Applied {
    match preset_file_body(form, button) {
        Ok(body) => {
            to_plan(form, &body, Some(button));
            call(EditCall::PresetPlan { edit: body })
        }
        Err(held) => *held,
    }
}

pub(super) fn native_remove(form: &mut OpenForm) -> Applied {
    let unit = text_of(form.values.get("unit"));
    let body = json!({ "kind": "remove_native", "unit": unit });
    to_plan(form, &body, None);
    call(EditCall::Plan {
        stack: form.stack.clone(),
        edit: body,
    })
}
