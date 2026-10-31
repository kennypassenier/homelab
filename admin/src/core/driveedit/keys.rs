//! Edit forms: the new-stack step check and the host.toml key parser.

use super::*;

/// `checkNewStep`: the wizard's own checks of one step.
pub fn check_new_step(
    step: &FormStep,
    v: &Values,
    names: &[String],
    vmids: &[i64],
) -> BTreeMap<String, String> {
    let mut errors = check_fields(&step.fields, v);
    if step.id == "identity" {
        let name = text_of(v.get("name")).trim().to_string();
        if !errors.contains_key("name") && names.contains(&name) {
            errors.insert("name".into(), say("name_taken", &[("name", &name)]));
        }
        let t = text_of(v.get("vmid"));
        let vmid = t.trim().parse::<i64>().unwrap_or(-1);
        let vs = t.trim().to_string();
        if !errors.contains_key("vmid") && vmids.contains(&vmid) {
            errors.insert("vmid".into(), say("vmid_taken", &[("vmid", &vs)]));
        }
        if !errors.contains_key("vmid") && es().no_touch.contains(&vmid) {
            errors.insert("vmid".into(), say("vmid_no_touch", &[("vmid", &vs)]));
        }
    }
    errors
}

/// `parseKey`: a typed text as the key's JSON value; null removes it.
pub fn parse_key(kind: &Value, input: &Value) -> Result<Value, String> {
    let ty = kind["type"].as_str().unwrap_or("text");
    if ty == "bool" {
        return Ok(json!(
            *input == Value::Bool(true) || input.as_str() == Some("true")
        ));
    }
    let t = text_of(Some(input)).trim().to_string();
    if t.is_empty() {
        return Ok(Value::Null);
    }
    match ty {
        "int" => {
            let (min, max) = (
                kind["min"].as_u64().unwrap_or(0),
                kind["max"].as_u64().unwrap_or(u64::MAX),
            );
            match t.parse::<u64>() {
                Ok(n) if all_digits(&t) && (min..=max).contains(&n) => Ok(json!(n)),
                _ => Err(say(
                    "key_int",
                    &[("min", &min.to_string()), ("max", &max.to_string())],
                )),
            }
        }
        "vmid" => match t.parse::<u64>() {
            Ok(n) if all_digits(&t) && n >= 100 => Ok(json!(n)),
            _ => Err(say("key_vmid", &[])),
        },
        "url" => {
            if matches(r"https?://\S+", &t) {
                Ok(json!(t))
            } else {
                Err(say("key_url", &[]))
            }
        }
        "window" => {
            if matches(r"\d+[smhdw]", &t) {
                Ok(json!(t))
            } else {
                Err(say("key_window", &[]))
            }
        }
        "vmid_list" => {
            let parts: Vec<Option<u64>> = t
                .split(|c: char| c.is_whitespace() || c == ',')
                .filter(|x| !x.is_empty())
                .map(|x| x.parse::<u64>().ok().filter(|n| *n >= 100))
                .collect();
            if parts.iter().all(Option::is_some) {
                Ok(json!(parts.into_iter().flatten().collect::<Vec<_>>()))
            } else {
                Err(say("key_vmid_list", &[]))
            }
        }
        "text_list" => Ok(json!(
            t.split(',')
                .map(str::trim)
                .filter(|x| !x.is_empty())
                .collect::<Vec<_>>()
        )),
        "table" => Ok(json!(t)),
        _ => {
            if t.contains('\n') {
                Err(say("key_line", &[]))
            } else {
                Ok(json!(t))
            }
        }
    }
}

/// `fieldText`: the text a key's field starts with.
pub(super) fn key_text(f: &Value) -> String {
    if f["kind"]["type"] == "table" {
        return f["toml"].as_str().unwrap_or("").to_string();
    }
    if f["set"] != Value::Bool(true) || f["value"].is_null() {
        return String::new();
    }
    match &f["value"] {
        Value::Array(a) => a
            .iter()
            .map(|x| text_of(Some(x)))
            .collect::<Vec<_>>()
            .join(", "),
        v => text_of(Some(v)),
    }
}
