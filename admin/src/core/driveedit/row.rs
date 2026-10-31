//! `homelab ui row …`: the firewall's rules, host.toml's keys and the row
//! lists of the Apps, Latch, Checks, Tiles and settings-ext forms.

use super::*;

pub fn row(
    form: &mut OpenForm,
    step: &UiStep,
    op: &str,
    target: Option<&str>,
) -> Result<Applied, Refusal> {
    let no_table = || {
        refused(
            step,
            format!("the form {} has no table", form.title),
            "row steps are for the firewall and the host settings",
        )
    };
    let Family::Edit(kind) = form.desc.family else {
        return Err(no_table());
    };
    if form.sent() {
        return Err(refused(
            step,
            "the form was sent",
            "homelab ui close, then open it again",
        ));
    }
    if let Some(sub) = form.edit.as_ref().and_then(|e| e.sub.as_ref()) {
        return Err(refused(
            step,
            format!("the dialog {} is open", sub.title),
            "press save or cancel first",
        ));
    }
    match kind {
        EditKind::Firewall => row_firewall(form, step, op, target),
        EditKind::HostSettings => row_host_key(form, step, op, target),
        EditKind::SettingsExt
        | EditKind::Apps
        | EditKind::Latch
        | EditKind::Checks
        | EditKind::Tiles => row_list(form, step, kind, op, target),
        _ => Err(no_table()),
    }
}

/// A dialog on top of the form, its fields at their start values.
pub(super) fn sub_of(kind: &str, title: String, target: Value, fields: Vec<Field>) -> Sub {
    let values = fields
        .iter()
        .map(|f| (f.name.clone(), drive::start_value(f)))
        .collect();
    Sub {
        kind: kind.into(),
        title,
        target,
        fields,
        values,
        errors: BTreeMap::new(),
    }
}

/// Where row `i` goes on "up" / "down", if it can.
pub(super) fn swap_target(op: &str, i: usize, n: usize) -> Option<usize> {
    if op == "up" {
        i.checked_sub(1)
    } else {
        Some(i + 1).filter(|j| *j < n)
    }
}

/// Replace one list of the form's model.
pub(super) fn set_model_list(form: &mut OpenForm, list: &str, rows: Vec<Value>) {
    if let Some(m) = edit(form).model.as_mut() {
        m[list] = Value::Array(rows);
    }
}

fn rule_index(step: &UiStep, target: Option<&str>, n: usize) -> Result<usize, Refusal> {
    let t = target.unwrap_or("");
    match t.parse::<usize>() {
        Ok(i) if (1..=n).contains(&i) => Ok(i - 1),
        _ => Err(refused(
            step,
            format!("there is no rule {t:?}"),
            if n == 0 {
                "there are no rules yet; homelab ui row add".to_string()
            } else {
                format!("the rules are numbered 1 to {n}, top to bottom")
            },
        )),
    }
}

fn rule_dialog(
    step: &UiStep,
    op: &str,
    target: Option<&str>,
    rules: &[Value],
) -> Result<Sub, Refusal> {
    let (sub_target, rule) = if op == "add" {
        if let Some(t) = target {
            return Err(refused(
                step,
                format!("row add takes no number, not {t}"),
                "homelab ui row add",
            ));
        }
        (Value::Null, None)
    } else {
        let i = rule_index(step, target, rules.len())?;
        (json!(i), Some(rules[i]["rule"].clone()))
    };
    let title = match &rule {
        Some(r) => format!("Edit rule · {}", rule_summary(r)),
        None => "Add a rule".to_string(),
    };
    Ok(sub_of(
        "rule",
        title,
        sub_target,
        rule_fields(rule.as_ref()),
    ))
}

fn row_firewall(
    form: &mut OpenForm,
    step: &UiStep,
    op: &str,
    target: Option<&str>,
) -> Result<Applied, Refusal> {
    if form.step != "rules" {
        return Err(refused(
            step,
            format!("the rules are not on screen on the step {}", form.step),
            "press back to the rules",
        ));
    }
    let model = edit(form).model.clone().unwrap_or(Value::Null);
    let mut rules = model["rules"].as_array().cloned().unwrap_or_default();
    let n = rules.len();
    match op {
        "add" | "edit" => edit(form).sub = Some(rule_dialog(step, op, target, &rules)?),
        "up" | "down" => {
            let i = rule_index(step, target, n)?;
            let Some(j) = swap_target(op, i, n) else {
                return Err(refused(
                    step,
                    format!(
                        "rule {} is the {} already",
                        i + 1,
                        if op == "up" { "first" } else { "last" }
                    ),
                    "move another rule",
                ));
            };
            rules.swap(i, j);
            set_model_list(form, "rules", rules);
        }
        "delete" => {
            rules.remove(rule_index(step, target, n)?);
            set_model_list(form, "rules", rules);
        }
        other => {
            return Err(refused(
                step,
                format!("there is no row step {other}"),
                "row add, or row edit|up|down|delete <number>",
            ));
        }
    }
    Ok(plain())
}
