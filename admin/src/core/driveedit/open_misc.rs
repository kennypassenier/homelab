//! `homelab ui open` of the forms that belong to no one stack: the new-stack
//! wizard, host.toml, the import and the presets.

use super::*;

fn single(id: &str, title: &str, steps: Vec<FormStep>, page: &str) -> Opening {
    Opening {
        id: id.into(),
        stack: String::new(),
        title: title.into(),
        steps,
        page: page.into(),
    }
}

pub(super) fn open_new_stack(req: &Req) -> Result<Opening, Refusal> {
    req.no_target()?;
    req.unreadable("the presets")?;
    let data = &req.data;
    let presets = data["presets"].as_array().cloned().unwrap_or_default();
    if presets.is_empty() {
        return Err(req.refuse(
            match data["sync_error"].as_str() {
                Some(e) => format!("the working copy is not there: {e}"),
                None => "the repository holds no presets".into(),
            },
            "the settings page shows the working copy's state",
        ));
    }
    let steps = new_stack_steps(&presets, data["suggest_vmid"].as_u64());
    Ok(single("new-stack", "New stack", steps, req.page))
}

pub(super) fn open_host_settings(req: &Req) -> Result<Opening, Refusal> {
    req.no_target()?;
    req.unreadable("host.toml")?;
    if req.data["page"]["fields"].as_array().is_none() {
        return Err(req.refuse(
            "host.toml was not read",
            "the settings page says why; try again once the host answers",
        ));
    }
    let steps = vec![
        empty_step("keys", "Keys"),
        empty_step("review", "Write host.toml"),
    ];
    let mut o = single("edit:host-settings", "Host settings", steps, "/settings");
    o.stack = crate::core::actions::HOST_TARGET.to_string();
    Ok(o)
}

pub(super) fn open_import(req: &Req) -> Result<Opening, Refusal> {
    req.no_target()?;
    req.unreadable("the taken names and numbers")?;
    if req.data["working_copy"] == json!(false) {
        return Err(req.refuse(
            "the dashboard has no working copy to commit the new stack to",
            "the settings page shows the working copy's state",
        ));
    }
    let fields = es().import.iter().map(EditFieldDef::field).collect();
    let steps = plan_steps(step_of("bundle", "Bundle", fields));
    Ok(single("import", "Import a stack", steps, req.page))
}

pub(super) fn open_preset(req: &Req) -> Result<Opening, Refusal> {
    let Some(name) = req.target else {
        return Err(req.missing("preset"));
    };
    if presetedit::valid_name(name).is_err() {
        return Err(req.refuse(
            format!("{name:?} is not a preset name"),
            "lowercase letters, digits and '-', not starting with '_'",
        ));
    }
    req.unreadable(&format!("the {name} preset"))?;
    if req.data["exists"] != json!(true) {
        return Err(req.refuse(
            format!("there is no preset called {name} yet"),
            format!("homelab ui open new-preset, then name it {name}"),
        ));
    }
    let mut fields: Vec<Field> = es().preset_meta.iter().map(EditFieldDef::field).collect();
    for f in &mut fields {
        f.current = Some(preset_meta_current(&f.name, &req.data["meta"]));
    }
    // feat-preset-1: the Files card's own fields and "Remove this
    // preset…" are only ever on screen for a preset that already exists,
    // the same `if (e.exists)` gate the browser's own dialog uses.
    fields.extend(preset_file_fields(&req.data));
    let steps = plan_steps(step_of("meta", "preset.yml", fields));
    let mut o = single(
        &format!("edit:preset:{name}"),
        &format!("{name} · preset.yml"),
        steps,
        "/presets",
    );
    o.stack = name.to_string();
    Ok(o)
}

pub(super) fn open_new_preset(req: &Req) -> Result<Opening, Refusal> {
    req.no_target()?;
    let mut fields: Vec<Field> = vec![es().new_preset_name.field()];
    for d in &es().preset_meta {
        let mut f = d.field();
        f.current = Some(preset_meta_current(&f.name, &Value::Null));
        fields.push(f);
    }
    let steps = plan_steps(step_of("meta", "New preset", fields));
    Ok(single("new-preset", "New preset", steps, "/presets"))
}

pub(super) fn open_rollback(req: &Req) -> Result<Opening, Refusal> {
    let stack = req.stack_named(req.target)?;
    req.unreadable("the roll-back options")?;
    let none = Choice {
        value: String::new(),
        label: "None".into(),
    };
    let mut fields: Vec<Field> = es().rollback.iter().map(EditFieldDef::field).collect();
    for f in &mut fields {
        let mut c = vec![none.clone()];
        if f.name == "commit" {
            c.extend(commit_choices(&req.data["commits"]));
        } else {
            c.extend(
                strings(&req.data["native_units"])
                    .into_iter()
                    .map(|u| Choice {
                        value: u.clone(),
                        label: u,
                    }),
            );
        }
        f.choices = Some(c);
        f.current = Some(json!(""));
    }
    Ok(Opening {
        id: format!("rollback:{stack}"),
        title: format!("Roll back · {stack}"),
        steps: vec![step_of("choose", "Choose", fields)],
        page: format!("/stacks/{stack}"),
        stack,
    })
}

fn commit_choices(commits: &Value) -> Vec<Choice> {
    commits
        .as_array()
        .into_iter()
        .flatten()
        .map(|x| {
            let commit = x["commit"].as_str().unwrap_or("").to_string();
            let short: String = commit.chars().take(10).collect();
            Choice {
                label: format!("{short} · {}", x["subject"].as_str().unwrap_or("")),
                value: commit,
            }
        })
        .collect()
}
