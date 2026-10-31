//! Edit forms: the checks form's row dialogs: checks, probes, manual.

use super::*;

/// `checkRowFields`.
pub fn check_row_fields(row: Option<&Value>) -> Vec<Field> {
    es().check_row
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(r[&d.name].clone());
            }
            f
        })
        .collect()
}

/// `checkRowFromValues` (no `origin`: the caller adds it).
pub fn check_row_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert("name".into(), json!(text_of(v.get("name")).trim()));
    m.insert("command".into(), json!(text_of(v.get("command")).trim()));
    let expect = text_of(v.get("expect")).trim().to_string();
    m.insert(
        "expect".into(),
        json!(if expect.is_empty() {
            "never_decreases".into()
        } else {
            expect
        }),
    );
    let layer = text_of(v.get("layer")).trim().to_string();
    m.insert(
        "layer".into(),
        json!(if layer.is_empty() {
            "network".into()
        } else {
            layer
        }),
    );
    let bs = text_of(v.get("blind_spot")).trim().to_string();
    if !bs.is_empty() {
        m.insert("blind_spot".into(), json!(bs));
    }
    Value::Object(m)
}

/// `checkRowProblems`.
pub fn check_row_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let layer = text_of(v.get("layer"));
    let bs = text_of(v.get("blind_spot")).trim().to_string();
    if layer != "application" && layer != "user_visible" && bs.is_empty() {
        out.insert(
            "blind_spot".into(),
            "A check below Application layer needs a blind spot.".into(),
        );
    }
    out
}

/// `checkRowSummary`.
pub fn check_row_summary(row: &Value) -> String {
    format!(
        "{} ({})",
        row["name"].as_str().unwrap_or(""),
        row["layer"].as_str().unwrap_or("")
    )
}

/// `probeRowFields`.
pub fn probe_row_fields(row: Option<&Value>) -> Vec<Field> {
    es().probe_row
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "healthy_kind" => {
                        let h = &r["healthy"];
                        json!(if h.get("equals").is_some() {
                            "equals"
                        } else if h.get("at_most").is_some() {
                            "at_most"
                        } else {
                            "at_least"
                        })
                    }
                    "healthy_value" => {
                        let h = &r["healthy"];
                        json!(num_text(
                            h.get("equals").unwrap_or(
                                h.get("at_least")
                                    .or(h.get("at_most"))
                                    .unwrap_or(&Value::Null)
                            )
                        ))
                    }
                    k => r[k].clone(),
                });
            }
            f
        })
        .collect()
}

/// `probeRowFromValues`.
pub fn probe_row_from_values(v: &Values) -> Value {
    let kind = text_of(v.get("healthy_kind"));
    let val = text_of(v.get("healthy_value")).trim().to_string();
    let healthy = match kind.as_str() {
        "at_most" => json!({ "at_most": js_number(&val) }),
        "at_least" => json!({ "at_least": js_number(&val) }),
        _ => json!({ "equals": val }),
    };
    let mut m = Map::new();
    m.insert("name".into(), json!(text_of(v.get("name")).trim()));
    m.insert("command".into(), json!(text_of(v.get("command")).trim()));
    m.insert("healthy".into(), healthy);
    m.insert("layer".into(), json!(text_of(v.get("layer")).trim()));
    let bs = text_of(v.get("blind_spot")).trim().to_string();
    if !bs.is_empty() {
        m.insert("blind_spot".into(), json!(bs));
    }
    Value::Object(m)
}

/// `probeRowProblems`.
pub fn probe_row_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let kind = text_of(v.get("healthy_kind"));
    let val = text_of(v.get("healthy_value")).trim().to_string();
    if matches!(kind.as_str(), "at_least" | "at_most")
        && !val.is_empty()
        && val.trim_start_matches('-').parse::<i64>().is_err()
    {
        out.insert("healthy_value".into(), "A whole number.".into());
    }
    let layer = text_of(v.get("layer"));
    let bs = text_of(v.get("blind_spot")).trim().to_string();
    if layer != "application" && layer != "user_visible" && bs.is_empty() {
        out.insert(
            "blind_spot".into(),
            "A probe below Application layer needs a blind spot.".into(),
        );
    }
    out
}

/// `probeRowSummary`.
pub fn probe_row_summary(row: &Value) -> String {
    let h = &row["healthy"];
    let word = if let Some(e) = h.get("equals") {
        format!("= {}", text_of(Some(e)))
    } else if let Some(n) = h.get("at_least") {
        format!(">= {}", text_of(Some(n)))
    } else if let Some(n) = h.get("at_most") {
        format!("<= {}", text_of(Some(n)))
    } else {
        String::new()
    };
    format!("{} ({word})", row["name"].as_str().unwrap_or(""))
}

/// `checks.yml`'s `manual:` entries normalised to `{text, once}` objects —
/// a bare string is valid too, and `with_origin` would spread its
/// characters instead of its fields.
pub(super) fn manual_as_objects(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .map(|m| match m {
                Value::String(s) => json!({ "text": s, "once": false }),
                other => other.clone(),
            })
            .collect(),
    )
}

/// `manualRowFields`.
pub fn manual_row_fields(row: Option<&Value>) -> Vec<Field> {
    es().manual_row
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(r[&d.name].clone());
            }
            f
        })
        .collect()
}

/// `manualRowFromValues`.
pub fn manual_row_from_values(v: &Values) -> Value {
    json!({
        "text": text_of(v.get("text")).trim(),
        "once": v.get("once") == Some(&Value::Bool(true)),
    })
}

/// `manualRowSummary`.
pub fn manual_row_summary(row: &Value) -> String {
    let text = row["text"].as_str().unwrap_or("");
    if row["once"] == Value::Bool(true) {
        format!("{text} (once)")
    } else {
        text.to_string()
    }
}
