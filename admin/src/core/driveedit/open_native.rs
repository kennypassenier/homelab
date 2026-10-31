//! `homelab ui open` of the native-unit form and of the per-app forms
//! (`checks`, `publish`).

use super::*;

/// `<stack>[/<unit>]`: `rollback-native`'s own convention, not a second
/// CLI argument.
fn split_unit(target: &str) -> (&str, Option<&str>) {
    match target.split_once('/') {
        Some((s, u)) => (s, Some(u)),
        None => (target, None),
    }
}

fn unit_names(natives: &[Value]) -> Vec<String> {
    natives
        .iter()
        .filter_map(|n| n["unit"].as_str().map(str::to_string))
        .collect()
}

/// The unit the form opens on: the named one, else the first.
fn chosen_unit(
    req: &Req,
    stack: &str,
    natives: &[Value],
    unit: Option<&str>,
) -> Result<String, Refusal> {
    let Some(u) = unit else {
        return Ok(natives[0]["unit"].as_str().unwrap_or_default().to_string());
    };
    if !natives.iter().any(|n| n["unit"].as_str() == Some(u)) {
        return Err(req.refuse(
            format!("{stack} has no native unit called {u}"),
            format!("its native units are: {}", unit_names(natives).join(", ")),
        ));
    }
    Ok(u.to_string())
}

fn native_fields(natives: &[Value], chosen: &str) -> Vec<Field> {
    let m = natives
        .iter()
        .find(|n| n["unit"].as_str() == Some(chosen))
        .map(|n| n["manifest"].clone())
        .unwrap_or(Value::Null);
    let mut fields: Vec<Field> = es().native.iter().map(EditFieldDef::field).collect();
    for f in &mut fields {
        if f.name == "unit" {
            f.choices = Some(
                unit_names(natives)
                    .into_iter()
                    .map(|u| Choice {
                        value: u.clone(),
                        label: u,
                    })
                    .collect(),
            );
            f.current = Some(json!(chosen));
        } else {
            f.current = Some(native_current(&f.name, &m));
        }
    }
    fields
}

pub(super) fn open_native(req: &Req) -> Result<Opening, Refusal> {
    let Some(target) = req.target else {
        return Err(req.missing("stack"));
    };
    let (stack_part, unit_part) = split_unit(target);
    let stack = req.stack_named(Some(stack_part))?;
    req.unreadable(&format!("the files of {stack}"))?;
    let natives = req.data["natives"].as_array().cloned().unwrap_or_default();
    if natives.is_empty() {
        return Err(req.refuse(
            format!("{stack} has no native units"),
            format!("homelab ui open add-native {stack}"),
        ));
    }
    let chosen = chosen_unit(req, &stack, &natives, unit_part)?;
    let first = step_of(
        "native",
        "Native services",
        native_fields(&natives, &chosen),
    );
    Ok(Opening {
        id: format!("edit:native:{stack}"),
        title: format!("Native services · {stack}"),
        steps: plan_steps(first),
        page: format!("/stacks/{stack}/settings"),
        stack,
    })
}

/// `<stack>/<app>`, the stack readable and the app one of its own.
fn app_target(req: &Req) -> Result<(String, String), Refusal> {
    let Some(raw) = req.target else {
        return Err(req.missing("app"));
    };
    let Some((stack_name, app)) = raw.split_once('/') else {
        return Err(req.refuse(
            format!("{} takes <stack>/<app>, not {raw:?}", req.kind.slug()),
            format!("homelab ui open {}", req.kind.usage()),
        ));
    };
    let stack = req.stack_named(Some(stack_name))?;
    req.unreadable(&format!("the files of {stack}"))?;
    let apps = strings(&req.manifest(&stack)?["apps"]);
    if !apps.iter().any(|a| a == app) {
        return Err(req.refuse(
            format!("{app} is not an app of {stack}"),
            format!("the apps are: {}", apps.join(", ")),
        ));
    }
    Ok((stack, app.to_string()))
}

pub(super) fn open_app_form(req: &Req, es_: &mut EditState) -> Result<Opening, Refusal> {
    let (stack, app) = app_target(req)?;
    let m = &req.data["manifest"];
    let (id, title, first, page) = if req.kind == EditKind::Checks {
        (
            format!("edit:checks:{stack}:{app}"),
            format!("checks.yml · {app} · {stack}"),
            checks_step(m, &app, es_),
            format!("/stacks/{stack}/checks"),
        )
    } else {
        let fields: Vec<Field> = es().publish.iter().map(EditFieldDef::field).collect();
        es_.model = Some(json!({ "app": app }));
        (
            format!("edit:publish:{stack}:{app}"),
            format!("Publish {app} · {stack}"),
            step_of("publish", "Publish", fields),
            format!("/stacks/{stack}/apps"),
        )
    };
    Ok(Opening {
        id,
        title,
        steps: plan_steps(first),
        page,
        stack,
    })
}

/// `checks` lives on the manifest (`m`), not the envelope (`data`).
fn checks_step(m: &Value, app: &str, es_: &mut EditState) -> FormStep {
    let orig = &m["checks"][app];
    let rows = |key: &str| with_origin(orig.get(key).unwrap_or(&Value::Null));
    es_.model = Some(json!({
        "app": app,
        "checks": rows("checks"),
        "manual": with_origin(&manual_as_objects(orig.get("manual").unwrap_or(&Value::Null))),
        "probes": rows("probes"),
    }));
    let busy = orig
        .get("busy_check")
        .and_then(|b| b["command"].as_str())
        .unwrap_or("");
    let url = orig.get("url").and_then(Value::as_str).unwrap_or("");
    let apps = strings(&m["apps"]);
    let mut fields: Vec<Field> = es().checks_scalar.iter().map(EditFieldDef::field).collect();
    for f in &mut fields {
        match f.name.as_str() {
            "app" => {
                f.choices = Some(
                    apps.iter()
                        .map(|a| Choice {
                            value: a.clone(),
                            label: a.clone(),
                        })
                        .collect(),
                );
                f.current = Some(json!(app));
            }
            "busy_check" => f.current = Some(json!(busy)),
            "url" => f.current = Some(json!(url)),
            _ => {}
        }
    }
    step_of("checks", "Checks", fields)
}
