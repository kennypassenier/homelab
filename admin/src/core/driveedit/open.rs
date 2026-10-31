//! `homelab ui open` of an edit form: the request, the checks every kind
//! shares, and the dispatch to the builder of its kind.

use super::*;

/// What a builder yields: the form's id, stack, title, steps and page.
pub(super) struct Opening {
    pub id: String,
    pub stack: String,
    pub title: String,
    pub steps: Vec<FormStep>,
    pub page: String,
}

/// The request being opened, with the answers every kind asks of it.
pub(super) struct Req<'a> {
    pub step: &'a UiStep,
    pub kind: EditKind,
    pub form: &'a str,
    pub target: Option<&'a str>,
    pub cx: &'a Ctx<'a>,
    pub data: Value,
    pub page: &'a str,
    pub selected: &'a [String],
}

impl Req<'_> {
    pub(super) fn refuse(&self, why: impl Into<String>, fix: impl Into<String>) -> Refusal {
        refused(self.step, why, fix)
    }

    /// The target is missing: `noun` is what the form acts on.
    pub(super) fn missing(&self, noun: &str) -> Refusal {
        self.refuse(
            format!("{} acts on one {noun} and none was named", self.kind.slug()),
            format!("homelab ui open {}", self.kind.usage()),
        )
    }

    pub(super) fn stack_named(&self, t: Option<&str>) -> Result<String, Refusal> {
        let Some(t) = t else {
            return Err(self.missing("stack"));
        };
        if !self.cx.stacks.iter().any(|s| s == t) {
            return Err(self.refuse(
                format!("the fleet has no stack {t}"),
                format!("the stacks are: {}", self.cx.stacks.join(", ")),
            ));
        }
        Ok(t.to_string())
    }

    pub(super) fn no_target(&self) -> Result<(), Refusal> {
        match self.target {
            Some(t) => Err(self.refuse(
                format!("{} takes no stack, not {t}", self.kind.slug()),
                format!("homelab ui open {}", self.kind.usage()),
            )),
            None => Ok(()),
        }
    }

    pub(super) fn unreadable(&self, what: &str) -> Result<(), Refusal> {
        let Some(why) = self.data["error"].as_object() else {
            return Ok(());
        };
        Err(Refusal::new(
            format!("ui open {}", self.kind.slug()),
            format!(
                "{what} could not be read: {}",
                why.get("why").and_then(Value::as_str).unwrap_or("unknown")
            ),
            why.get("fix")
                .and_then(Value::as_str)
                .unwrap_or("homelab ui state, then try again")
                .to_string(),
        ))
    }

    /// The stack's manifest, which most forms cannot open without.
    pub(super) fn manifest(&self, stack: &str) -> Result<&Value, Refusal> {
        let m = &self.data["manifest"];
        if m.is_null() {
            return Err(self.refuse(
                format!(
                    "the {} form needs a readable lxc-compose.yml: {}",
                    self.kind.slug(),
                    self.data["manifest_error"].as_str().unwrap_or("none")
                ),
                format!("homelab ui open raw {stack}"),
            ));
        }
        Ok(m)
    }
}

/// A form step with no fields (the plan and the commit pages).
pub(super) fn empty_step(id: &str, label: &str) -> FormStep {
    FormStep {
        id: id.into(),
        label: label.into(),
        fields: Vec::new(),
    }
}

/// A form step with its fields.
pub(super) fn step_of(id: &str, label: &str, fields: Vec<Field>) -> FormStep {
    FormStep {
        id: id.into(),
        label: label.into(),
        fields,
    }
}

/// `[first, plan, commit]`: the shape of every edit that ends in a commit.
pub(super) fn plan_steps(first: FormStep) -> Vec<FormStep> {
    vec![
        first,
        empty_step("plan", "Plan"),
        empty_step("commit", "Commit"),
    ]
}

/// `homelab ui open` of an edit form.
pub fn open(
    st: &mut DriveState,
    step: &UiStep,
    kind: EditKind,
    form: &str,
    target: Option<&str>,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    if let Some(f) = &st.form {
        return Err(refused(
            step,
            format!("the dialog {} is open already", f.title),
            "homelab ui close first",
        ));
    }
    if kind.scope() > cx.scope {
        return Err(refused(
            step,
            format!(
                "{} needs scope {:?}; the token \"{}\" has {:?}",
                kind.slug(),
                kind.scope(),
                cx.by,
                cx.scope
            ),
            "drive it with a token of that scope, or leave it to Kenny",
        ));
    }
    let req = Req {
        step,
        kind,
        form,
        target,
        cx,
        data: cx.sources.edit.clone(),
        page: &st.page,
        selected: &st.selected,
    };
    let mut es_ = EditState::new(kind, req.data.clone());
    let o = build(&req, &mut es_)?;
    st.page = o.page;
    st.form = Some(OpenForm::new_edit(o.id, o.stack, o.title, o.steps, es_));
    Ok(plain())
}

fn build(req: &Req, es_: &mut EditState) -> Result<Opening, Refusal> {
    match req.kind {
        EditKind::Settings
        | EditKind::Raw
        | EditKind::AddApp
        | EditKind::Firewall
        | EditKind::SettingsExt
        | EditKind::Apps
        | EditKind::Latch
        | EditKind::Tiles
        | EditKind::AddNative => open_stack_form(req, es_),
        EditKind::Native => open_native(req),
        EditKind::Checks | EditKind::PublishApp => open_app_form(req, es_),
        EditKind::NewStack => open_new_stack(req),
        EditKind::HostSettings => open_host_settings(req),
        EditKind::Batch => open_batch(req, es_),
        EditKind::Import => open_import(req),
        EditKind::Preset => open_preset(req),
        EditKind::NewPreset => open_new_preset(req),
        EditKind::Rollback => open_rollback(req),
    }
}

/// The forms that edit one stack's files: `edit:<slug>:<stack>`.
fn open_stack_form(req: &Req, es_: &mut EditState) -> Result<Opening, Refusal> {
    let stack = req.stack_named(req.target)?;
    req.unreadable(&format!("the files of {stack}"))?;
    let m = if req.kind == EditKind::Raw {
        &req.data["manifest"]
    } else {
        req.manifest(&stack)?
    };
    let tab = if req.kind == EditKind::Firewall {
        "firewall"
    } else {
        "settings"
    };
    let (head, first) = match req.kind {
        EditKind::Settings => open_settings(m, &req.data),
        EditKind::Raw => open_raw(&req.data),
        EditKind::AddApp => open_add_app(req)?,
        EditKind::Firewall => open_firewall(m, es_),
        EditKind::SettingsExt => open_settings_ext(m, es_),
        EditKind::Apps => open_apps(m, es_),
        EditKind::Latch => open_latch(m, es_),
        EditKind::AddNative => open_add_native(),
        _ => open_tiles(m, es_),
    };
    Ok(Opening {
        id: format!("edit:{}:{stack}", req.kind.slug()),
        title: format!("{head} · {stack}"),
        steps: plan_steps(first),
        page: format!("/stacks/{stack}/{tab}"),
        stack,
    })
}
