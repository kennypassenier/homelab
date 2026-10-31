//! Edit forms: the native-unit and preset forms: bodies and current values.

use super::*;

/// A field's value from the submitted `values`, falling back to what it was
/// opened with — the native and preset forms send every field (the server
/// diffs against the file itself), unlike `settings_body`'s "only what
/// changed".
pub(super) fn field_value(fields: &[Field], values: &Values, name: &str) -> Value {
    values
        .get(name)
        .cloned()
        .or_else(|| {
            fields
                .iter()
                .find(|f| f.name == name)
                .and_then(|f| f.current.clone())
        })
        .unwrap_or(Value::Null)
}

/// `nativeField`'s `metrics`/`update_policy` pickers read back, and the
/// plain text/textarea/check fields — `native-<field>`'s value in the
/// server's own words (`admin/web/js/editpanels.js`'s `edit()`).
pub fn native_body(fields: &[Field], values: &Values) -> Value {
    let s = |n: &str| text_of(Some(&field_value(fields, values, n)));
    let b = |n: &str| field_value(fields, values, n) == Value::Bool(true);
    let data_dirs: Vec<Value> = s("data_dirs")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| json!(l))
        .collect();
    json!({
        "kind": "native",
        "unit": s("unit"),
        "binary": s("binary"),
        "env_file": s("env_file"),
        "data_dirs": data_dirs,
        "update_cmd": s("update_cmd"),
        "stateless": b("stateless"),
        "restore_note": s("restore_note"),
        "release_repo": s("release_repo"),
        "release_asset": s("release_asset"),
        "backup_from_newest": s("backup_from_newest"),
        "backup_pause": s("backup_pause"),
        "update_policy": s("update_policy"),
        "metrics": s("metrics"),
    })
}

/// "Add a native unit"'s fields, in the server's own words.
pub fn add_native_body(fields: &[Field], values: &Values) -> Value {
    let s = |n: &str| text_of(Some(&field_value(fields, values, n)));
    let b = |n: &str| field_value(fields, values, n) == Value::Bool(true);
    let data_dirs: Vec<Value> = s("data_dirs")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| json!(l))
        .collect();
    json!({
        "kind": "add_native",
        "unit": s("unit").trim(),
        "binary": s("binary").trim(),
        "env_file": s("env_file"),
        "data_dirs": data_dirs,
        "notify": b("notify"),
    })
}

/// The manifest field of a native unit's own JSON (`NativeView`'s shape,
/// `admin/src/shell/edit/stack_read.rs`'s `read_stack_edit`), as the form field of
/// that name would hold it — `native-unit`'s `after_set` and `open`'s
/// prefill share this.
pub fn native_current(name: &str, m: &Value) -> Value {
    match name {
        "data_dirs" => json!(strings(&m["data_dirs"]).join("\n")),
        "stateless" => json!(m[name].as_bool().unwrap_or(false)),
        // fix-113 (owner decision, 2026-10-01): `backup_pause` reads from
        // the manifest as a bool (false/true) or the string "chassis" —
        // `NativeServiceManifest`'s own `BackupPause::Serialize` shape — and
        // the select wants it back as one of those three strings.
        "backup_pause" => json!(match m[name].as_str() {
            Some("chassis") => "chassis",
            _ if m[name].as_bool() == Some(true) => "true",
            _ => "false",
        }),
        "update_policy" => json!(m[name].as_str().unwrap_or("manual")),
        "metrics" => json!(if m["metrics"] == json!(false) {
            "not_measured"
        } else {
            "measured"
        }),
        "binary" => json!(m["binary"].as_str().unwrap_or("")),
        _ => json!(m[name].as_str().unwrap_or("")),
    }
}

/// A `Number(text)` or `null` when blank — `preset.yml`'s optional `cores`
/// and `disk_gb`.
pub(super) fn opt_number(t: &str) -> Value {
    if t.trim().is_empty() {
        Value::Null
    } else {
        js_number(t)
    }
}

/// `preset.yml`'s own fields, in the server's own words (feat-preset-1).
/// `unprivileged` has no field in the form (the browser's meta card does
/// not expose it either): it is carried through unchanged from what the
/// preset already held, `Value::Null` for a new one.
pub fn preset_meta_body(fields: &[Field], values: &Values, unprivileged: Value) -> Value {
    let s = |n: &str| text_of(Some(&field_value(fields, values, n)));
    let b = |n: &str| field_value(fields, values, n) == Value::Bool(true);
    let opt_str = |n: &str| {
        let t = s(n);
        if t.trim().is_empty() {
            Value::Null
        } else {
            json!(t.trim())
        }
    };
    json!({
        "description": s("description").trim(),
        "ram_mb": js_number(&s("ram_mb")),
        "cores": opt_number(&s("cores")),
        "disk_gb": opt_number(&s("disk_gb")),
        "features": opt_str("features"),
        "unprivileged": unprivileged,
        "gpu": b("gpu"),
        "vpn": b("vpn"),
    })
}

/// A preset's read meta (`PresetMetaView`, `admin/web/js/presetseditor.js`)
/// as the field of that name would hold it; `Value::Null` (a new preset)
/// gives every field its blank/default starting value.
pub(super) fn preset_meta_current(name: &str, meta: &Value) -> Value {
    match name {
        "ram_mb" => json!(meta["ram_mb"].as_u64().unwrap_or(1024).to_string()),
        "cores" | "disk_gb" => match meta[name].as_u64() {
            Some(n) => json!(n.to_string()),
            None => json!(""),
        },
        "gpu" | "vpn" => json!(meta[name].as_bool().unwrap_or(false)),
        "description" | "features" => json!(meta[name].as_str().unwrap_or("")),
        _ => Value::Null,
    }
}

/// The keys of a JSON object, sorted.
fn sorted_keys(v: &Value, skip: Option<&str>) -> Vec<String> {
    let mut keys: Vec<String> = v
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.clone())
        .filter(|k| Some(k.as_str()) != skip)
        .collect();
    keys.sort();
    keys
}

/// A picker's choices: the empty choice first, then each name.
fn choices_after(first_label: &str, names: &[String]) -> Vec<Choice> {
    let mut choices = vec![Choice {
        value: String::new(),
        label: first_label.into(),
    }];
    choices.extend(names.iter().map(|n| Choice {
        value: n.clone(),
        label: n.clone(),
    }));
    choices
}

/// feat-preset-1: the Files card's own fields, appended to the "meta"
/// step: `file`'s choices are the preset's files (minus `preset.yml`),
/// `template`'s the starter templates; the rest start blank.
pub(super) fn preset_file_fields(data: &Value) -> Vec<Field> {
    let names = sorted_keys(&data["files"], Some(presetedit::PRESET_META));
    // The templates keep the object's own order, as they always did.
    let templates: Vec<String> = data["file_templates"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.clone())
        .collect();
    es().preset_file
        .iter()
        .map(|d| {
            let mut f = d.field();
            match d.name.as_str() {
                "file" => f.choices = Some(choices_after("(new file)", &names)),
                "template" => f.choices = Some(choices_after("Blank", &templates)),
                _ => {}
            }
            f.current = Some(json!(""));
            f
        })
        .collect()
}
