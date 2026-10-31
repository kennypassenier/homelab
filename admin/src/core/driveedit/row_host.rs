//! `homelab ui row edit <key>`: a host.toml key's dialog.

use super::*;

/// A field with nothing set but its name, id, kind and label.
fn blank_field(name: &str, id: String, kind: FieldKind, label: String) -> Field {
    Field {
        name: name.into(),
        id,
        kind,
        label,
        help: String::new(),
        required: false,
        pattern: None,
        placeholder: None,
        source: None,
        empty: None,
        danger: false,
        when: None,
        expect: None,
        min: None,
        max: None,
        choices: None,
        current: None,
        show_when: None,
        change_when: None,
    }
}

/// Why a key cannot be changed from the dashboard, if it cannot.
fn ssh_only(key: &str, access: &str, step: &UiStep) -> Result<(), Refusal> {
    if matches!(access, "browser" | "confirm") {
        return Ok(());
    }
    let who = match access {
        "locked" => "ssh only (can cut the dashboard off)",
        "ssh_only" => "ssh only (safety policy)",
        "host_held" => "ssh only (generated and kept by the host itself)",
        _ => "ssh only (secret)",
    };
    Err(refused(
        step,
        format!("{key} is changed {who}"),
        "change it over ssh on pve, not from the dashboard",
    ))
}

/// What the value field starts at: the staged change, else the current one.
fn start_of(pending: Option<&Value>, ty: &str, f: &Value) -> Value {
    match (pending, ty) {
        (Some(v), "bool") => json!(*v == Value::Bool(true)),
        (Some(v), _) => json!(if v.is_null() {
            String::new()
        } else {
            text_of(Some(v))
        }),
        (None, "bool") => json!(f["value"] == Value::Bool(true)),
        (None, _) => json!(key_text(f)),
    }
}

fn key_fields(f: &Value, key: &str, start: Value, access: &str, ty: &str) -> Vec<Field> {
    let ids = &es().host_key;
    let slug = key.replace('_', "-");
    let kind = match ty {
        "bool" => FieldKind::Check,
        "table" => FieldKind::Textarea,
        _ => FieldKind::Text,
    };
    let label = f["label"].as_str().unwrap_or(key).to_string();
    let mut value = blank_field("value", fill(&ids.id, &[("slug", &slug)]), kind, label);
    value.help = f["help"].as_str().unwrap_or("").to_string();
    value.current = Some(start);
    let mut fields = vec![value];
    if access == "confirm" {
        let id = fill(&ids.confirm_id, &[("slug", &slug)]);
        let label = fill(&ids.confirm_label, &[("key", key)]);
        let mut c = blank_field("confirm", id, FieldKind::Typed, label);
        c.required = true;
        c.placeholder = Some(key.to_string());
        c.danger = true;
        c.expect = Some(key.to_string());
        fields.push(c);
    }
    fields
}

/// The host.toml key's description as the page read it.
fn host_key(data: &Value, step: &UiStep, key: &str) -> Result<Value, Refusal> {
    let fields = data["page"]["fields"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    fields.into_iter().find(|f| f["key"] == key).ok_or_else(|| {
        refused(
            step,
            format!("host.toml has no key {key:?}"),
            "the keys are the rows of homelab ui state",
        )
    })
}

pub(super) fn row_host_key(
    form: &mut OpenForm,
    step: &UiStep,
    op: &str,
    target: Option<&str>,
) -> Result<Applied, Refusal> {
    if op != "edit" {
        return Err(refused(
            step,
            format!("a host.toml key has no row step {op}"),
            "homelab ui row edit <key>",
        ));
    }
    if form.step != "keys" {
        return Err(refused(
            step,
            format!("the keys are not on screen on the step {}", form.step),
            "press back to the keys",
        ));
    }
    let key = target.unwrap_or("");
    let e = edit(form);
    let f = host_key(&e.data, step, key)?;
    let access = f["access"].as_str().unwrap_or("");
    ssh_only(key, access, step)?;
    let ty = f["kind"]["type"].as_str().unwrap_or("text");
    let start = start_of(e.staged.get(key), ty, &f);
    let title = format!("{} · {key}", f["label"].as_str().unwrap_or(key));
    let sub_fields = key_fields(&f, key, start, access, ty);
    e.sub = Some(sub_of("key", title, json!(key), sub_fields));
    Ok(plain())
}
