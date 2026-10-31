//! A press while a dialog is on top of the form: save a rule, save a row,
//! save or default a host.toml key, or cancel.

use super::*;

/// The dialog's errors go on the dialog; the first one is the refusal.
fn hold_sub(form: &mut OpenForm, what: &str, errors: BTreeMap<String, String>) -> Applied {
    let r = held_for(form, what, &errors);
    if let Some(s) = edit(form).sub.as_mut() {
        s.errors = errors;
    }
    Applied {
        effect: Effect::None,
        held: Some(r),
    }
}

pub(super) fn press_dialog(
    form: &mut OpenForm,
    sub: &Sub,
    button: &str,
) -> Result<Applied, Refusal> {
    if button == "cancel" {
        edit(form).sub = None;
        return Ok(plain());
    }
    if sub.kind == "rule" {
        return Ok(save_rule(form, sub));
    }
    if EditKind::row_list_kind(&sub.kind) {
        return Ok(save_row(form, sub));
    }
    Ok(save_key(form, sub, button))
}

fn save_rule(form: &mut OpenForm, sub: &Sub) -> Applied {
    let mut errors = check_fields(&sub.fields, &sub.values);
    errors.extend(rule_problems(&sub.values));
    if !errors.is_empty() {
        return hold_sub(form, "save", errors);
    }
    let rule = rule_from_values(&sub.values);
    let e = edit(form);
    if let Some(m) = e.model.as_mut() {
        let rules = m["rules"].as_array_mut().expect("the model has rules");
        match sub.target.as_u64() {
            Some(i) => {
                if let Some(x) = rules.get_mut(i as usize) {
                    x["rule"] = rule;
                }
            }
            None => rules.push(json!({ "origin": null, "rule": rule })),
        }
    }
    e.sub = None;
    plain()
}

fn row_problems(list: &str, v: &Values) -> BTreeMap<String, String> {
    match list {
        "data_mounts" => data_mount_problems(v),
        "latch_files" => latch_file_problems(v),
        "checks" => check_row_problems(v),
        "probes" => probe_row_problems(v),
        "tiles" => tile_row_problems(v),
        "retention" => retention_row_problems(v),
        _ => BTreeMap::new(),
    }
}

fn row_from_values(list: &str, v: &Values) -> Value {
    match list {
        "storage" => storage_from_values(v),
        "data_mounts" => data_mount_from_values(v),
        "log_files" => log_file_from_values(v),
        "latch_files" => latch_file_from_values(v),
        "checks" => check_row_from_values(v),
        "manual" => manual_row_from_values(v),
        "probes" => probe_row_from_values(v),
        "retention" => retention_row_from_values(v),
        _ => tile_row_from_values(v),
    }
}

fn save_row(form: &mut OpenForm, sub: &Sub) -> Applied {
    let list = sub.kind.as_str();
    let mut errors = check_fields(&sub.fields, &sub.values);
    errors.extend(row_problems(list, &sub.values));
    if !errors.is_empty() {
        return hold_sub(form, "save", errors);
    }
    let mut row = row_from_values(list, &sub.values);
    let e = edit(form);
    if let Some(m) = e.model.as_mut() {
        let rows = m[list].as_array_mut().expect("the model has this list");
        match sub.target.as_u64() {
            Some(i) => {
                if let Some(old) = rows.get(i as usize) {
                    row["origin"] = old["origin"].clone();
                }
                if let Some(x) = rows.get_mut(i as usize) {
                    *x = row;
                }
            }
            None => {
                row["origin"] = Value::Null;
                rows.push(row);
            }
        }
    }
    e.sub = None;
    plain()
}

/// The new value of the key: its default, or what was typed, parsed.
fn key_value(form: &mut OpenForm, sub: &Sub, key: &str, button: &str) -> Result<Value, String> {
    if button == "default" {
        return Ok(Value::Null);
    }
    let kind = edit(form).data["page"]["fields"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|f| f["key"] == key)
        .map(|f| f["kind"].clone())
        .unwrap_or(Value::Null);
    parse_key(&kind, sub.values.get("value").unwrap_or(&Value::Null))
}

/// A host.toml key: save or default.
fn save_key(form: &mut OpenForm, sub: &Sub, button: &str) -> Applied {
    let key = sub.target.as_str().unwrap_or("").to_string();
    let mut errors = BTreeMap::new();
    let confirm = sub.fields.iter().any(|f| f.name == "confirm");
    if confirm && text_of(sub.values.get("confirm")).trim() != key {
        errors.insert("confirm".to_string(), say("key_confirm", &[("key", &key)]));
    }
    let value = match key_value(form, sub, &key, button) {
        Ok(v) if errors.is_empty() => v,
        Ok(_) => Value::Null,
        Err(why) => {
            errors.insert("value".into(), why);
            Value::Null
        }
    };
    if !errors.is_empty() {
        return hold_sub(form, button, errors);
    }
    let e = edit(form);
    if confirm {
        e.confirmed.insert(key.clone());
    }
    e.staged.insert(key, value);
    e.sub = None;
    plain()
}
