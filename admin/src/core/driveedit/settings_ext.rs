//! Edit forms: the settings form's second page and the retention rows.

use super::*;

/// `settingsExtForm`'s fields.
pub fn settings_ext_fields(m: &Value) -> Vec<Field> {
    es().settings_ext
        .iter()
        .map(|d| {
            let mut f = d.field();
            f.current = Some(match d.name.as_str() {
                "ip" => m["network"]["ip"].clone(),
                "gateway" => m["network"]["gateway"].clone(),
                "bridge" => m["network"]["bridge"].clone(),
                "vlan" => json!(num_text(&m["network"]["vlan"])),
                "unprivileged" => json!(m["lxc"]["unprivileged"].as_bool().unwrap_or(true)),
                "gpu" => json!(m["lxc"]["gpu"].as_bool().unwrap_or(false)),
                "vpn" => json!(m["lxc"]["vpn"].as_bool().unwrap_or(false)),
                "timezone" => {
                    let tz = m["lxc"]["timezone"].as_str().unwrap_or("host");
                    json!(tz)
                }
                "storage" => m["resources"]["storage"].clone(),
                "on_demand" => json!(m["on_demand"].as_bool().unwrap_or(false)),
                _ => Value::Null,
            });
            f
        })
        .collect()
}

/// `settingsExtBody`: only what differs from now — `retention` the same
/// origin-less full list `appsBody`'s storage/data_mounts/log_files use,
/// `model["retention"]` as the editor holds it now, `data` the manifest
/// `open` started from (`SettingsExtEdit.retention` is a plain
/// `Option<Vec<RetentionTierEdit>>`, not origin-tracked on the server
/// either — a tier carries no identity of its own to keep across an edit).
pub fn settings_ext_body(fields: &[Field], values: &Values, model: &Value, data: &Value) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("settings_ext"));
    for f in fields {
        let v = values.get(&f.name);
        if f.kind == FieldKind::Check {
            if v != f.current.as_ref() {
                out.insert(f.name.clone(), json!(v == Some(&Value::Bool(true))));
            }
            continue;
        }
        let Some(Value::String(t)) = v else { continue };
        let t = t.trim();
        let now = text_of(f.current.as_ref());
        if t.is_empty() || t == now {
            continue;
        }
        if f.kind == FieldKind::Number {
            out.insert(f.name.clone(), js_number(t));
        } else {
            out.insert(f.name.clone(), json!(t));
        }
    }
    let now_retention = with_origin(&data["retention"]);
    if model["retention"] != now_retention {
        let rows: Vec<Value> = model["retention"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(without_origin)
            .collect();
        out.insert("retention".into(), Value::Array(rows));
    }
    Value::Object(out)
}

/// The opposite of `with_origin`: a row with its bookkeeping `origin` key
/// dropped, the shape the server's own plain (non-origin-tracked) lists
/// take — `SettingsExtEdit.retention` (`Vec<RetentionTierEdit>`).
pub(super) fn without_origin(row: &Value) -> Value {
    let mut m = row.as_object().cloned().unwrap_or_default();
    m.remove("origin");
    Value::Object(m)
}

/// `retentionRowFields`.
pub fn retention_row_fields(row: Option<&Value>) -> Vec<Field> {
    es().retention_tier
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(json!(num_text(&r[&d.name])));
            }
            f
        })
        .collect()
}

/// `retentionRowFromValues` (no `origin`: the caller adds it).
pub fn retention_row_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert(
        "every_days".into(),
        js_number(text_of(v.get("every_days")).trim()),
    );
    let span = text_of(v.get("span_days")).trim().to_string();
    if !span.is_empty() {
        m.insert("span_days".into(), js_number(&span));
    }
    Value::Object(m)
}

/// `retentionRowProblems`.
pub fn retention_row_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let every = text_of(v.get("every_days")).trim().to_string();
    let span = text_of(v.get("span_days")).trim().to_string();
    if let (Ok(e), Ok(s)) = (every.parse::<i64>(), span.parse::<i64>())
        && s < e
    {
        out.insert(
            "span_days".into(),
            "Must be at least the keep-one-every span.".into(),
        );
    }
    out
}

/// `retentionRowSummary`.
pub fn retention_row_summary(row: &Value) -> String {
    match row["span_days"].as_i64() {
        Some(span) => format!("every {}d, kept {span}d", row["every_days"]),
        None => format!("every {}d, kept forever", row["every_days"]),
    }
}
