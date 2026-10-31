//! What else changes when one field changes, as the page's own handlers
//! do it: the preset's size, the raw file's text, the firewall's model.

use super::*;

pub fn after_set(form: &mut OpenForm, name: &str, in_sub: bool) {
    let Family::Edit(kind) = form.desc.family else {
        return;
    };
    if in_sub {
        return;
    }
    let value = form.values.get(name).cloned().unwrap_or(Value::Null);
    match (kind, name) {
        (EditKind::NewStack, "preset") => new_stack_preset(form, &value),
        (EditKind::Raw, "file") => raw_file(form, &value),
        (EditKind::Preset, "file") => preset_file(form, &value),
        (EditKind::Preset, "template") => preset_template(form, &value),
        (EditKind::AddApp, "preset") => add_app_preset(form, &value),
        (EditKind::Firewall, _) => firewall_field(form, name, value),
        (EditKind::Rollback, "commit") if value != json!("") => {
            form.values.insert("unit".into(), json!(""));
        }
        (EditKind::Rollback, "unit") if value != json!("") => {
            form.values.insert("commit".into(), json!(""));
        }
        (EditKind::Native, "unit") => native_unit(form, &value),
        _ => {}
    }
}

fn new_stack_preset(form: &mut OpenForm, value: &Value) {
    let p = edit(form).data["presets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"] == *value)
        .cloned();
    if let Some(p) = p {
        for k in ["ram_mb", "cores", "disk_gb"] {
            let v = text_of(Some(&p[k]));
            form.values.insert(k.into(), json!(v));
        }
    }
}

fn raw_file(form: &mut OpenForm, value: &Value) {
    let t = edit(form).data["texts"][value.as_str().unwrap_or("")].clone();
    form.values
        .insert("text".into(), if t.is_null() { json!("") } else { t });
}

/// Mirrors `filesCard`'s own `show()`: picking a file fills its path (then
/// locked to it in the real form) and its current text; picking
/// "(new file)" clears both.
fn preset_file(form: &mut OpenForm, value: &Value) {
    let f = value.as_str().unwrap_or("").to_string();
    form.values.insert("path".into(), json!(f));
    let content = if f.is_empty() {
        String::new()
    } else {
        text_of(edit(form).data["files"].get(f.as_str()))
    };
    form.values.insert("text".into(), json!(content));
}

/// Mirrors `templateSel`'s own change handler: only fills the text when a
/// NEW file is being made (no file picked).
fn preset_template(form: &mut OpenForm, value: &Value) {
    if !text_of(form.values.get("file")).trim().is_empty() {
        return;
    }
    let t = value.as_str().unwrap_or("").to_string();
    let content = if t.is_empty() {
        String::new()
    } else {
        text_of(edit(form).data["file_templates"].get(t.as_str()))
    };
    form.values.insert("text".into(), json!(content));
}

fn add_app_preset(form: &mut OpenForm, value: &Value) {
    let preset_name = value.as_str().unwrap_or("").to_string();
    let data = edit(form).data.clone();
    if let Some(step) = form.desc.steps.first_mut() {
        let preset_field = step
            .fields
            .iter()
            .find(|f| f.name == "preset")
            .cloned()
            .unwrap_or_else(|| es().add_app[0].field());
        step.fields = add_app_step_fields(preset_field, &data, &preset_name);
    }
    // A tile value typed for an app the new preset does not bring in is
    // stale; drop it rather than silently keep sending it.
    let keep: BTreeSet<String> = form
        .desc
        .steps
        .first()
        .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
        .unwrap_or_default();
    form.values
        .retain(|k, _| !k.starts_with("add-app-tile:") || keep.contains(k));
}

fn firewall_field(form: &mut OpenForm, name: &str, value: Value) {
    if let Some(m) = edit(form).model.as_mut()
        && m.get(name).is_some()
    {
        m[name] = value;
    }
}

const NATIVE_FIELDS: [&str; 12] = [
    "binary",
    "env_file",
    "data_dirs",
    "update_cmd",
    "stateless",
    "restore_note",
    "release_repo",
    "release_asset",
    "backup_from_newest",
    "backup_pause",
    "update_policy",
    "metrics",
];

fn native_unit(form: &mut OpenForm, value: &Value) {
    let natives = edit(form).data["natives"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let m = natives
        .iter()
        .find(|n| n["unit"] == *value)
        .map(|n| n["manifest"].clone())
        .unwrap_or(Value::Null);
    for field in NATIVE_FIELDS {
        form.values.insert(field.into(), native_current(field, &m));
    }
}
