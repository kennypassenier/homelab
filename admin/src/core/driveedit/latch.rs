//! Edit forms: the Latch tab: secrets, restart order and latch files.

use super::*;

/// `latchSecretField`.
pub fn latch_secret_field(app: &str) -> Field {
    let mut f = es().latch_secret.filled(&[("app", app)]);
    f.current = Some(json!(false));
    f
}

/// The Latch card's restart order as a form field (gap 74): `unit`'s place,
/// 1 restarts first, among `count` native units. Mirrors the card's reorder
/// list (editpanels.js `restartOrderGroup`), whose Earlier / Later buttons a
/// headless step cannot press.
pub fn latch_order_field(unit: &str, place: usize, count: usize) -> Field {
    let mut f = es().latch_order.filled(&[("unit", unit)]);
    f.choices = Some(
        (1..=count)
            .map(|n| Choice {
                value: n.to_string(),
                label: n.to_string(),
            })
            .collect(),
    );
    f.current = Some(json!(place.to_string()));
    f
}

/// The native units in the order the form's places give, or the problem:
/// two units on one place. `None` for a form with no order fields.
pub fn latch_order_of(
    fields: &[Field],
    values: &Values,
) -> Option<Result<Vec<String>, BTreeMap<String, String>>> {
    let places: Vec<(&Field, String, usize)> = fields
        .iter()
        .filter_map(|f| {
            let unit = f.name.strip_prefix("latch-order:")?;
            let place = text_of(values.get(&f.name)).trim().parse::<usize>().ok()?;
            Some((f, unit.to_string(), place))
        })
        .collect();
    if places.is_empty() {
        return None;
    }
    let mut seen = BTreeSet::new();
    let mut errors = BTreeMap::new();
    for (f, _, place) in &places {
        if !seen.insert(*place) {
            errors.insert(f.name.clone(), say("order_dup", &[]));
        }
    }
    if !errors.is_empty() {
        return Some(Err(errors));
    }
    let mut sorted = places;
    sorted.sort_by_key(|(_, _, place)| *place);
    Some(Ok(sorted.into_iter().map(|(_, unit, _)| unit).collect()))
}

/// `latchBody`: only the parts touched.
pub fn latch_body(fields: &[Field], values: &Values, model: &Value, data: &Value) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("latch"));
    let mut secrets = Vec::new();
    let mut touched_secrets = false;
    for f in fields {
        if let Some(app) = f.name.strip_prefix("latch-secret:") {
            touched_secrets = true;
            if values.get(&f.name) == Some(&Value::Bool(true)) {
                secrets.push(json!(app));
            }
        }
    }
    if touched_secrets {
        let mut now: Vec<String> = strings(&data["latch"]["latch_secrets"]);
        now.sort();
        let mut want: Vec<String> = secrets
            .iter()
            .filter_map(|v| v.as_str())
            .map(str::to_string)
            .collect();
        want.sort();
        if now != want {
            out.insert("secrets".into(), Value::Array(secrets));
        }
    }
    let now_files = with_origin(&data["latch"]["latch_files"]);
    if model["latch_files"] != now_files {
        out.insert("files".into(), model["latch_files"].clone());
    }
    // The restart order, only when it is not the file's own (a duplicate
    // place is refused before this runs).
    if let Some(Ok(order)) = latch_order_of(fields, values)
        && order != strings(&data["natives"])
    {
        out.insert("order".into(), json!(order));
    }
    Value::Object(out)
}

/// `latchFileFields`.
pub fn latch_file_fields(natives: &[String], row: Option<&Value>) -> Vec<Field> {
    es().latch_file
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(r[&d.name].clone());
            }
            if d.name == "restarts" {
                let mut c = vec![Choice {
                    value: String::new(),
                    label: "(none)".into(),
                }];
                c.extend(natives.iter().map(|u| Choice {
                    value: u.clone(),
                    label: u.clone(),
                }));
                f.choices = Some(c);
            }
            f
        })
        .collect()
}

/// `latchFileFromValues` (no `origin`: the caller adds it).
pub fn latch_file_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert("from".into(), json!(text_of(v.get("from")).trim()));
    m.insert("dest".into(), json!(text_of(v.get("dest")).trim()));
    m.insert("mode".into(), json!(text_of(v.get("mode")).trim()));
    let owner = text_of(v.get("owner")).trim().to_string();
    if !owner.is_empty() {
        m.insert("owner".into(), json!(owner));
    }
    let restarts = text_of(v.get("restarts")).trim().to_string();
    if !restarts.is_empty() {
        m.insert("restarts".into(), json!(restarts));
    }
    Value::Object(m)
}

/// `latchFileProblems`: the latch --expand trap, refused before anything
/// is sent, on every field a latch_files row carries.
pub fn latch_file_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for k in ["from", "dest", "mode", "owner", "restarts"] {
        if text_of(v.get(k)).contains("${") {
            out.insert(k.into(), say("dollar_expand", &[]));
        }
    }
    out
}

/// `latchFileSummary`.
pub fn latch_file_summary(row: &Value) -> String {
    format!(
        "{} → {}",
        row["from"].as_str().unwrap_or(""),
        row["dest"].as_str().unwrap_or("")
    )
}
