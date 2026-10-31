//! The "next" press of a stack edit: the form's values become the edit
//! body the plan is asked for.

use super::*;

/// The edit body, or the answer that holds the step.
type Body = Result<Value, Box<Applied>>;

pub(super) fn plan_press(form: &mut OpenForm, kind: EditKind) -> Applied {
    let fields: Vec<Field> = form.desc.steps[0].fields.clone();
    let body = match plan_body(form, kind, &fields) {
        Ok(b) => b,
        Err(held) => return *held,
    };
    to_plan(form, &body, None);
    call(EditCall::Plan {
        stack: form.stack.clone(),
        edit: body,
    })
}

fn unchanged() -> Box<Applied> {
    Box::new(held_msg("next", &say("unchanged", &[])))
}

/// The model and the read data a body is diffed against.
fn model_and_data(form: &mut OpenForm) -> (Value, Value) {
    let e = edit(form);
    (e.model.clone().unwrap_or(Value::Null), e.data.clone())
}

fn plan_body(form: &mut OpenForm, kind: EditKind, fields: &[Field]) -> Body {
    match kind {
        EditKind::Settings => settings_plan(form, fields),
        EditKind::Raw => raw_plan(form, fields),
        EditKind::AddApp => Ok(add_app_plan(form, fields)),
        EditKind::Native => Ok(native_body(fields, &form.values)),
        EditKind::AddNative => add_native_plan(form, fields),
        EditKind::SettingsExt => settings_ext_plan(form, fields),
        EditKind::Apps => {
            let (model, data) = model_and_data(form);
            changed_or_held(apps_body(fields, &form.values, &model, &data["manifest"]))
        }
        EditKind::Latch => latch_plan(form, fields),
        EditKind::Checks => checks_plan(form),
        EditKind::Tiles => tiles_plan(form),
        EditKind::PublishApp => publish_plan(form, fields),
        _ => Ok(firewall_body(
            edit(form).model.as_ref().unwrap_or(&Value::Null),
        )),
    }
}

fn changed_or_held(b: Value) -> Body {
    if changes_something(&b) {
        Ok(b)
    } else {
        Err(unchanged())
    }
}

fn settings_plan(form: &mut OpenForm, fields: &[Field]) -> Body {
    let mut errors = check_fields(fields, &form.values);
    errors.extend(tile_problems(&form.values));
    if !errors.is_empty() {
        return Err(hold_b(form, "next", errors));
    }
    changed_or_held(settings_body(fields, &form.values))
}

fn settings_ext_plan(form: &mut OpenForm, fields: &[Field]) -> Body {
    let errors = check_fields(fields, &form.values);
    if !errors.is_empty() {
        return Err(hold_b(form, "next", errors));
    }
    let (model, data) = model_and_data(form);
    changed_or_held(settings_ext_body(
        fields,
        &form.values,
        &model,
        &data["manifest"],
    ))
}

fn latch_plan(form: &mut OpenForm, fields: &[Field]) -> Body {
    if let Some(Err(errors)) = latch_order_of(fields, &form.values) {
        return Err(hold_b(form, "next", errors));
    }
    let (model, data) = model_and_data(form);
    changed_or_held(latch_body(fields, &form.values, &model, &data["manifest"]))
}

fn checks_plan(form: &mut OpenForm) -> Body {
    let (model, data) = model_and_data(form);
    let manifest = &data["manifest"];
    let app = model["app"].as_str().unwrap_or("");
    let b = checks_drive_body(&form.values, &model, manifest, app);
    if checks_changed(&b, manifest, app) {
        Ok(b)
    } else {
        Err(unchanged())
    }
}

fn tiles_plan(form: &mut OpenForm) -> Body {
    let (model, data) = model_and_data(form);
    let b = tiles_drive_body(&model, &data);
    if b["tiles"].as_array().is_none_or(Vec::is_empty) {
        return Err(unchanged());
    }
    Ok(b)
}

fn publish_plan(form: &mut OpenForm, fields: &[Field]) -> Body {
    let errors = check_fields(fields, &form.values);
    if !errors.is_empty() {
        return Err(hold_b(form, "next", errors));
    }
    let app = edit(form)
        .model
        .as_ref()
        .and_then(|m| m["app"].as_str())
        .unwrap_or("")
        .to_string();
    Ok(publish_body(&form.values, &app))
}

fn add_native_plan(form: &mut OpenForm, fields: &[Field]) -> Body {
    let unit = text_of(form.values.get("unit"));
    let binary = text_of(form.values.get("binary"));
    let mut errors = BTreeMap::new();
    if unit.trim().is_empty() {
        errors.insert("unit".into(), say("needed", &[("label", "Unit name")]));
    }
    if binary.trim().is_empty() {
        errors.insert("binary".into(), say("needed", &[("label", "Binary")]));
    }
    if !errors.is_empty() {
        return Err(hold_b(form, "next", errors));
    }
    Ok(add_native_body(fields, &form.values))
}

fn add_app_plan(form: &OpenForm, fields: &[Field]) -> Value {
    let mut tiles = Map::new();
    for f in fields {
        let Some(app) = f.name.strip_prefix("add-app-tile:") else {
            continue;
        };
        let hostname = text_of(form.values.get(&f.name)).trim().to_string();
        if !hostname.is_empty() {
            tiles.insert(app.to_string(), json!(hostname));
        }
    }
    let mut out = Map::new();
    out.insert("kind".into(), json!("add_app"));
    out.insert("preset".into(), json!(text_of(form.values.get("preset"))));
    if !tiles.is_empty() {
        out.insert("tiles".into(), Value::Object(tiles));
    }
    Value::Object(out)
}

/// A needed raw-editor field that is empty holds the step.
fn needed(form: &mut OpenForm, fields: &[Field], name: &str) -> Result<(), Box<Applied>> {
    if text_of(form.values.get(name)).trim().is_empty() {
        let label = fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.label.clone())
            .unwrap_or_else(|| name.to_string());
        let errors = BTreeMap::from([(name.to_string(), say("needed", &[("label", &label)]))]);
        return Err(hold_b(form, "next", errors));
    }
    Ok(())
}

/// feat-stacks-files: create, delete and rename, the same ops the Files
/// card's buttons send (`stackedit_files::FilesEdit`); "edit" (the
/// default, an unset `op` included) stays the original raw edit.
fn raw_plan(form: &mut OpenForm, fields: &[Field]) -> Body {
    let text = |form: &OpenForm, n: &str| text_of(form.values.get(n));
    match text(form, "op").as_str() {
        "create" => {
            needed(form, fields, "new_path")?;
            let (path, content) = (text(form, "new_path"), text(form, "text"));
            Ok(json!({ "kind": "files", "op": "create", "path": path, "content": content }))
        }
        "delete" => Ok(json!({ "kind": "files", "op": "delete", "path": text(form, "file") })),
        "rename" => {
            let file = text(form, "file");
            needed(form, fields, "rename_to")?;
            let to = text(form, "rename_to");
            Ok(json!({ "kind": "files", "op": "rename", "from": file, "to": to }))
        }
        _ => {
            let (file, content) = (text(form, "file"), text(form, "text"));
            if edit(form).data["texts"][&file] == json!(content) {
                return Err(Box::new(held_msg("next", &say("raw_unchanged", &[]))));
            }
            Ok(json!({ "kind": "raw", "path": file, "content": content }))
        }
    }
}
