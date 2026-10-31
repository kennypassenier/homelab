//! Edit forms: the Apps tab: remove, add-blank and the add-app step.

use super::*;

/// `appsRemoveField`.
pub fn apps_remove_field(app: &str) -> Field {
    let mut f = es().apps_remove.filled(&[("app", app)]);
    f.current = Some(json!(false));
    f
}

/// `appsAddBlankField`.
pub fn apps_add_blank_field() -> Field {
    let mut f = es().apps_add_blank.field();
    f.current = Some(json!(""));
    f
}

/// feat-tiles-3: one app's tile-hostname field, on the add-app step.
pub fn add_app_tile_field(app: &str) -> Field {
    let mut f = es().add_app_tile.filled(&[("app", app)]);
    f.current = Some(json!(""));
    f
}

/// The add-app step's fields for whichever preset is picked: `preset`
/// itself, kept as it was (its choices, its current value), plus one tile
/// field per app that preset brings in — rebuilt by `after_set` on every
/// `pick`, and once up front in `open` for the preset already selected by
/// default (the first one, the same as the browser's own `renderTiles`).
pub fn add_app_step_fields(preset_field: Field, data: &Value, preset_name: &str) -> Vec<Field> {
    let apps: Vec<String> = data["presets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some(preset_name))
        .map(|p| strings(&p["apps"]))
        .unwrap_or_default();
    let mut fields = vec![preset_field];
    fields.extend(apps.iter().map(|a| add_app_tile_field(a)));
    fields
}

/// Every element of `v` (a JSON array) with its own index added as
/// `origin` — flat, since the server's edit structs here (unlike
/// `FirewallEdit.rules`) are not `{origin, rule}` nested.
pub(super) fn with_origin(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, row)| {
                let mut m = row.as_object().cloned().unwrap_or_default();
                m.insert("origin".into(), json!(i));
                Value::Object(m)
            })
            .collect(),
    )
}

/// `appsBody`: only the parts touched. `model` is the row tables as the
/// editor holds them now (`EditState.model`); `data` is the edit read
/// `open` started from, to tell an untouched list from an edited one.
pub fn apps_body(fields: &[Field], values: &Values, model: &Value, data: &Value) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("apps"));
    let mut remove = Vec::new();
    for f in fields {
        if let Some(app) = f.name.strip_prefix("apps-remove:")
            && values.get(&f.name) == Some(&Value::Bool(true))
        {
            remove.push(json!(app));
        }
    }
    if !remove.is_empty() {
        out.insert("remove".into(), Value::Array(remove));
    }
    let add_blank: Vec<Value> = text_of(values.get("add_blank"))
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| json!(s))
        .collect();
    if !add_blank.is_empty() {
        out.insert("add_blank".into(), Value::Array(add_blank));
    }
    for list in ["storage", "data_mounts", "log_files"] {
        let now = &model[list];
        let orig = with_origin(&data[list]);
        if *now != orig {
            out.insert(list.into(), now.clone());
        }
    }
    Value::Object(out)
}
