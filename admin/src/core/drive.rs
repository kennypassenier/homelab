//! feat-platform-10 (milestone follow): Claude drives Kenny's open dashboard,
//! step by step, without a browser.
//!
//! `homelab ui <step>` reaches the dashboard through the host line. This
//! module is the pure half: the form descriptions (read from the same
//! `formspec.json` the browser's `actionforms.js` imports, so a step and a
//! click are checked against the same words), and the one shared "Claude is
//! driving" state every step is applied to. The shell reads what a step
//! needs (the fleet, the stack's commits), runs the final press through the
//! action queue once, and pushes the result to every tab that follows.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use homelab_proto::{Scope, UiStep};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::actions::{ActionArgs, ActionKind, Arg, HOST_TARGET, Refusal, SELF_STACK};
use crate::core::driveedit::{EditCall, EditKind, EditState};
use crate::core::drivelive::{self, Announce, Plan};

/// The shared description, compiled in.
pub const FORM_SPEC_JSON: &str = include_str!("../../web/js/formspec.json");

/// A driver who sends nothing for this long no longer holds the tabs.
pub const IDLE_S: i64 = 600;

/// fix-163: a confirmed dialog whose job has ended, with no step since, is
/// closed and the tabs given back this long after (as `homelab ui done`),
/// unless `ActConfig` says otherwise (`HOMELAB_ADMIN_RELEASE_AFTER_JOB_S`).
pub const RELEASE_AFTER_JOB_S: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Check,
    Text,
    Choice,
    Typed,
    /// A whole number between `min` and `max` (the edit forms).
    Number,
    /// Several lines (a commit note, a rule's comment, a file's text).
    Textarea,
}

/// One value of a choice field whose list is part of its description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListOnly {
    pub label: String,
    pub help: String,
    pub required: bool,
    pub placeholder: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FieldDef {
    pub kind: FieldKind,
    pub label: String,
    pub help: String,
    pub required: bool,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub empty: Option<String>,
    #[serde(default)]
    pub danger: bool,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub expect: Option<String>,
    #[serde(default)]
    pub list_only: Option<ListOnly>,
    #[serde(default)]
    pub help_for: BTreeMap<String, String>,
    /// A choice whose values are part of the description (the verdict).
    #[serde(default)]
    pub choices: Option<Vec<Choice>>,
    /// Per action: the label when it differs (a template's temporary vmid).
    #[serde(default)]
    pub label_for: BTreeMap<String, String>,
    /// Per action: whether the field must be filled when it differs.
    #[serde(default)]
    pub required_for: BTreeMap<String, bool>,
    /// On screen, checked and sent only while another field has a value.
    #[serde(default)]
    pub show_when: Option<ShowWhen>,
    /// Other words (and required) while another field has a value.
    #[serde(default)]
    pub change_when: Option<ChangeWhen>,
}

/// A field shown only while `field` has `value` (the answer's days, with
/// accept); `says` is when, in words, for `homelab ui state` and a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowWhen {
    pub field: String,
    pub value: String,
    pub says: String,
}

/// A field's label, help and required while `field` has `value` (the note
/// becomes the required reason with accept).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeWhen {
    pub field: String,
    pub value: String,
    pub label: String,
    pub help: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepLabels {
    pub options: String,
    pub review: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Messages {
    pub typed_missing: String,
    pub choose: String,
    pub typed_wrong: String,
    pub pattern: String,
}

/// `formspec.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct FormSpec {
    pub review_args: Vec<String>,
    /// Per action, the further arguments asked on the review step (a
    /// one-line form such as exec has no options step).
    #[serde(default)]
    pub review_for: BTreeMap<String, Vec<String>>,
    pub steps: StepLabels,
    pub patterns: BTreeMap<String, String>,
    pub fields: BTreeMap<String, FieldDef>,
    pub messages: Messages,
    pub pages: Vec<String>,
    pub stack_tabs: Vec<String>,
    /// fix-199: every `Open{form}` target Claude or a click can name — the
    /// edit forms' slugs (`EditKind::ALL`) and the action forms' slugs
    /// (`ActionKind::ALL`), one list, read by the browser the same way
    /// `pages` already is. A test (`drivelive::tests`) holds it equal to
    /// those two enums, so a kind added to either and forgotten here fails
    /// the build rather than silently under-reporting what this version
    /// knows.
    #[serde(default)]
    pub forms: Vec<String>,
    /// The edit forms (`driveedit`).
    pub edit: crate::core::driveedit::EditSpec,
}

/// fix-199 (Kenny, 2026-10-02: "het enige wat een versie check moet doen is
/// om ons te laten weten welke pagina's of commandos we kunnen gebruiken
/// voor die versie, dat moet niks tegenhouden" — a version check informs,
/// it never blocks): what a following Live view tab has told the driver it
/// knows, from its OWN loaded copy of `formspec.json` (`POST
/// /data/drive/attach`). A tab that never reported anything (`DriveState.
/// tab_caps == None`) is not held to any capability — nothing is refused on
/// the strength of a version nobody has told the driver about, the same
/// rule fix-185 already applied to a bare version string.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabCaps {
    /// The page names the tab's own router recognises (`formspec.json`'s
    /// `pages`, as its bundle has them — `""` is the home page).
    pub pages: std::collections::BTreeSet<String>,
    /// The `Open{form}` slugs the tab's own bundle can draw a dialog for
    /// (`formspec.json`'s `forms`, as its bundle has them).
    pub forms: std::collections::BTreeSet<String>,
}

/// The description, read once. It is compiled in and covered by a test, so
/// a file that does not read is a build that does not pass.
pub fn spec() -> &'static FormSpec {
    static SPEC: OnceLock<FormSpec> = OnceLock::new();
    SPEC.get_or_init(|| serde_json::from_str(FORM_SPEC_JSON).expect("formspec.json reads"))
}

/// `{word}` placeholders filled, the way `actionforms.js` `fill` does.
pub fn fill(template: &str, words: &[(&str, &str)]) -> String {
    let mut out = template.to_string();
    for (k, v) in words {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

pub fn arg_name(a: Arg) -> &'static str {
    match a {
        Arg::Force => "force",
        Arg::Confirm => "confirm",
        Arg::Snapshot => "snapshot",
        Arg::App => "app",
        Arg::Unit => "unit",
        Arg::SkipBackup => "skip_backup",
        Arg::SkipSafetyCopy => "skip_safety_copy",
        Arg::Commit => "commit",
        Arg::Vmid => "vmid",
        Arg::Command => "command",
        Arg::Tag => "tag",
        Arg::Version => "version",
        Arg::Privileged => "privileged",
        Arg::Base => "base",
        Arg::Check => "check",
        Arg::Verdict => "verdict",
        Arg::Days => "days",
        Arg::Note => "note",
        Arg::Destroy => "destroy",
        Arg::SecretRef => "secret_ref",
        Arg::StageToken => "stage_token",
    }
}

/// One field of a form, as `actionforms.js` `argField` builds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Field {
    pub name: String,
    pub id: String,
    pub kind: FieldKind,
    pub label: String,
    pub help: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty: Option<String>,
    pub danger: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    /// The list of a choice field whose values are part of the form.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Choice>>,
    /// The value an edit form starts with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_when: Option<ShowWhen>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_when: Option<ChangeWhen>,
}

/// The field as the form stands with these values, as `actionforms.js`
/// `shownField`: `None` while its `show_when` does not hold (hidden, not
/// checked, not sent), its `change_when` words laid on while that holds.
pub fn shown_field<'a>(f: &'a Field, values: &Values) -> Option<std::borrow::Cow<'a, Field>> {
    let holds = |field: &str, value: &str| values.get(field).and_then(Value::as_str) == Some(value);
    if let Some(w) = &f.show_when
        && !holds(&w.field, &w.value)
    {
        return None;
    }
    match &f.change_when {
        Some(c) if holds(&c.field, &c.value) => {
            let mut g = f.clone();
            g.label = c.label.clone();
            g.help = c.help.clone();
            g.required = c.required;
            Some(std::borrow::Cow::Owned(g))
        }
        _ => Some(std::borrow::Cow::Borrowed(f)),
    }
}

pub fn arg_field(arg: Arg, action: ActionKind, stack: &str) -> Field {
    let name = arg_name(arg);
    let def = &spec().fields[name];
    let f = |s: &str| fill(s, &[("stack", stack)]);
    let list_only = (arg == Arg::Confirm && !action.confirm())
        .then_some(def.list_only.as_ref())
        .flatten();
    let help = def
        .help_for
        .get(action.slug())
        .cloned()
        .unwrap_or_else(|| list_only.map_or(def.help.clone(), |l| l.help.clone()));
    let label = def
        .label_for
        .get(action.slug())
        .cloned()
        .unwrap_or_else(|| list_only.map_or(def.label.clone(), |l| l.label.clone()));
    let required = def
        .required_for
        .get(action.slug())
        .copied()
        .unwrap_or_else(|| list_only.map_or(def.required, |l| l.required));
    Field {
        name: name.to_string(),
        id: format!("act-{}", name.replace('_', "-")),
        kind: def.kind,
        label: f(&label),
        help: f(&help),
        required,
        pattern: def
            .pattern
            .as_ref()
            .map(|p| spec().patterns.get(p).cloned().unwrap_or_else(|| p.clone())),
        placeholder: list_only
            .map(|l| l.placeholder.clone())
            .or_else(|| def.placeholder.clone())
            .map(|p| f(&p)),
        source: def.source.clone(),
        empty: def.empty.clone(),
        danger: def.danger,
        when: def.when.clone(),
        expect: def.expect.as_deref().map(f),
        min: None,
        max: None,
        choices: def.choices.clone(),
        current: None,
        show_when: def.show_when.clone(),
        change_when: def.change_when.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FormStep {
    pub id: String,
    pub label: String,
    pub fields: Vec<Field>,
}

/// One action's form on one target, as `actionforms.js` `actionForm`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ActionForm {
    pub id: String,
    pub action: ActionKind,
    pub stack: String,
    pub title: String,
    pub what: String,
    pub submit: String,
    pub steps: Vec<FormStep>,
    pub refused: Option<String>,
    pub destructive: bool,
}

pub const REVIEW: &str = "review";

pub fn action_form(action: ActionKind, stack: &str) -> ActionForm {
    let stack = if action.host_wide() {
        HOST_TARGET
    } else {
        stack
    };
    let fields: Vec<Field> = action
        .args()
        .iter()
        .map(|a| arg_field(*a, action, stack))
        .collect();
    let also = spec().review_for.get(action.slug());
    let (review, options): (Vec<Field>, Vec<Field>) = fields.into_iter().partition(|f| {
        spec().review_args.contains(&f.name) || also.is_some_and(|a| a.contains(&f.name))
    });
    let mut steps = Vec::new();
    if !options.is_empty() {
        steps.push(FormStep {
            id: "options".into(),
            label: spec().steps.options.clone(),
            fields: options,
        });
    }
    steps.push(FormStep {
        id: REVIEW.into(),
        label: spec().steps.review.clone(),
        fields: review,
    });
    let place = if action.host_wide() {
        "the whole host"
    } else {
        stack
    };
    ActionForm {
        id: format!("action:{}", action.slug()),
        action,
        stack: stack.to_string(),
        title: format!("{} · {}", action.label(), place),
        what: action.what().to_string(),
        submit: action.label().to_string(),
        steps,
        refused: (stack == SELF_STACK && action.refused_for_self()).then(|| {
            "The dashboard never does this to its own stack (arch-self); use the CLI from a workstation."
                .to_string()
        }),
        destructive: action.scope() == Scope::All,
    }
}

impl ActionForm {
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.steps.iter().flat_map(|s| s.fields.iter())
    }
}

pub type Values = BTreeMap<String, Value>;

/// The values a fresh form starts with.
pub fn initial_values(form: &ActionForm) -> Values {
    form.fields()
        .map(|f| {
            let v = if f.kind == FieldKind::Check {
                Value::Bool(false)
            } else {
                Value::String(String::new())
            };
            (f.name.clone(), v)
        })
        .collect()
}

/// What is wrong with one step's values (or the whole form's), by field
/// name, in `actionforms.js` `checkValues`'s words.
pub fn check_values(
    form: &ActionForm,
    values: &Values,
    step: Option<&str>,
) -> BTreeMap<String, String> {
    check_steps(&form.steps, values, step)
}

/// `check_values` over any action form's steps.
pub fn check_steps(
    steps: &[FormStep],
    values: &Values,
    step: Option<&str>,
) -> BTreeMap<String, String> {
    let m = &spec().messages;
    let mut errors = BTreeMap::new();
    for s in steps.iter().filter(|s| step.is_none_or(|id| s.id == id)) {
        for f in &s.fields {
            let Some(f) = shown_field(f, values) else {
                continue;
            };
            if f.kind == FieldKind::Check {
                continue;
            }
            let text = values
                .get(&f.name)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let label = f.label.to_lowercase();
            let expect = f.expect.clone().unwrap_or_default();
            let words = [
                ("expect", expect.as_str()),
                ("label", label.as_str()),
                ("text", text.as_str()),
            ];
            if f.required && text.is_empty() {
                let t = if f.kind == FieldKind::Typed {
                    &m.typed_missing
                } else {
                    &m.choose
                };
                errors.insert(f.name.clone(), fill(t, &words));
                continue;
            }
            if text.is_empty() {
                continue;
            }
            if f.kind == FieldKind::Typed && text != expect {
                errors.insert(f.name.clone(), fill(&m.typed_wrong, &words));
                continue;
            }
            if let Some(p) = &f.pattern {
                let ok = regex::Regex::new(&format!("^(?:{p})$"))
                    .map(|r| r.is_match(&text))
                    .unwrap_or(false);
                if !ok {
                    errors.insert(f.name.clone(), fill(&m.pattern, &words));
                }
            }
        }
    }
    errors
}

/// The request body, as `actionforms.js` `buildArgs`: only what is set.
pub fn build_args(form: &ActionForm, values: &Values) -> ActionArgs {
    build_args_of(form.fields(), values)
}

/// `build_args` over any list of action fields.
pub fn build_args_of<'a>(fields: impl Iterator<Item = &'a Field>, values: &Values) -> ActionArgs {
    let mut args = ActionArgs::default();
    for f in fields.filter(|f| shown_field(f, values).is_some()) {
        let v = values.get(&f.name);
        let on = v == Some(&Value::Bool(true));
        let text = v
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_string);
        match f.name.as_str() {
            "force" => args.force = on,
            "skip_backup" => args.skip_backup = on,
            "skip_safety_copy" => args.skip_safety_copy = on,
            "confirm" => args.confirm = text,
            "snapshot" => args.snapshot = text,
            "app" => args.app = text,
            "unit" => args.unit = text,
            "commit" => args.commit = text,
            "vmid" => args.vmid = text,
            "command" => args.command = text,
            "tag" => args.tag = text,
            "version" => args.version = text,
            "privileged" => args.privileged = on,
            "base" => args.base = text,
            "check" => args.check = text,
            "verdict" => args.verdict = text,
            "days" => args.days = text,
            "note" => args.note = text,
            "destroy" => args.destroy = text,
            _ => {}
        }
    }
    args
}

// ── the driving state ──────────────────────────────────────────────────

/// The job the final press started, as the tabs and `homelab ui state`
/// see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobRef {
    pub job: u64,
    pub state: String,
    pub message: Option<String>,
    pub progress: Option<String>,
}

/// One field as the driver reads it on screen.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldState {
    pub id: String,
    pub name: String,
    pub kind: FieldKind,
    pub label: String,
    pub step: String,
    pub value: Value,
    pub error: Option<String>,
    pub shown: bool,
    /// Why a hidden field is hidden: when it shows, in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden_why: Option<String>,
    pub choices: Vec<String>,
}

/// The dialog Claude has open.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OpenForm {
    pub id: String,
    pub action: String,
    pub stack: String,
    pub title: String,
    pub steps: Vec<String>,
    pub step: String,
    pub step_index: usize,
    /// By field name, as the dialog keeps them.
    pub values: Values,
    /// By field name.
    pub errors: BTreeMap<String, String>,
    /// Choice fields' values, by field name.
    pub choices: BTreeMap<String, Vec<String>>,
    pub guard: Option<Refusal>,
    pub cli: Option<String>,
    pub restarts_dashboard: bool,
    /// Why the last confirm did not start a job.
    pub run_error: Option<Refusal>,
    pub job: Option<JobRef>,
    pub buttons: Vec<String>,
    pub fields: Vec<FieldState>,
    /// The edit forms' own state: the firewall's rules, a dialog on top,
    /// the plan, what the final press answered (`driveedit`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<EditState>,
    /// fix-185: does this form's final press read the stack files from the
    /// repository (`Needs::Spec`, `Needs::Apply`)? `homelab ui` preflights
    /// such a press on the machine driving it, before the dashboard (and
    /// its own working copy) is asked at all.
    #[serde(default)]
    pub reads_repo: bool,
    #[serde(skip)]
    pub desc: FormDesc,
}

/// fix-185: `f`'s final press reads the repository — a single action's own
/// `Needs`, or a batch's wrapped action.
fn reads_repo(f: &OpenForm) -> bool {
    let of = |k: ActionKind| {
        matches!(
            k.needs(),
            crate::core::actions::Needs::Spec | crate::core::actions::Needs::Apply
        )
    };
    match f.desc.family {
        Family::Action(kind) => of(kind),
        Family::Edit(EditKind::Batch) => f
            .edit
            .as_ref()
            .and_then(|e| e.batch_action.as_deref())
            .and_then(ActionKind::from_slug)
            .is_some_and(of),
        Family::Edit(_) => false,
    }
}

/// What a form is: one of the actions, or one of the edit forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Action(ActionKind),
    Edit(EditKind),
}

/// The description an open form is checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormDesc {
    pub family: Family,
    pub steps: Vec<FormStep>,
}

impl FormDesc {
    pub fn fields(&self) -> impl Iterator<Item = &Field> {
        self.steps.iter().flat_map(|s| s.fields.iter())
    }
}

/// The one shared "Claude is driving" state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DriveState {
    pub active: bool,
    pub by: Option<String>,
    /// Steps applied so far; a tab that saw `seq - 1` animates the next,
    /// any other catches up.
    pub seq: u64,
    pub page: String,
    pub form: Option<OpenForm>,
    /// Unix seconds of the last step.
    pub last_at: i64,
    pub idle_s: i64,
    /// Decision "23 constants" (2026-09-30): [`RELEASE_AFTER_JOB_S`] by
    /// default.
    pub release_after_job_s: i64,
    /// Live view: the step announced now, before it is taken (`drivelive`).
    pub announce: Option<Announce>,
    /// Live view: a viewer paused; the next step waits for Continue.
    pub paused_by: Option<String>,
    /// Live view: a viewer stopped the drive; every step is refused until
    /// the driver says `done`.
    pub stopped_by: Option<String>,
    /// Live view: the sequence the driver sent up front.
    pub plan: Option<Plan>,
    /// Owner decision 2026-09-30: the Overview fleet table's ticked stacks
    /// (`homelab ui select`), so a batch dialog can be opened from them,
    /// exactly as the page's own "Run on the selected…" button does.
    #[serde(default)]
    pub selected: Vec<String>,
    /// fix-185: the loaded dashboard version a following tab most recently
    /// reported (`POST /data/drive/attach`). `None` until a tab has ever
    /// reported — a step is never refused on the strength of a version
    /// nobody has told the driver about.
    #[serde(default)]
    pub tab_page_version: Option<String>,
    /// fix-199: the same tab's self-reported capabilities, alongside its
    /// version string. `None` until a tab has ever reported them (an old
    /// tab whose `/data/drive/attach` predates this field, or none yet) —
    /// permissive, never a reason to refuse a step.
    #[serde(default)]
    pub tab_caps: Option<TabCaps>,
}

impl Default for DriveState {
    fn default() -> Self {
        DriveState {
            active: false,
            by: None,
            seq: 0,
            // nav-decisions (chassis-rs 3.1.0): the root is Apps now, not
            // Overview (moved to `/overview`); a fresh browser session
            // lands there too.
            page: "/".into(),
            form: None,
            last_at: 0,
            idle_s: IDLE_S,
            release_after_job_s: RELEASE_AFTER_JOB_S,
            announce: None,
            paused_by: None,
            stopped_by: None,
            plan: None,
            selected: Vec::new(),
            tab_page_version: None,
            tab_caps: None,
        }
    }
}

/// What the lists a choice is filled from hold, read by the shell before
/// an `open`.
#[derive(Debug, Clone, Default)]
pub struct Sources {
    pub apps: Vec<String>,
    pub units: Vec<String>,
    pub commits: Vec<String>,
    /// dashboard-latest: the "release tag" dropdown's values (`latest`,
    /// then every release GitHub lists) of update-host or install-native.
    pub releases: Vec<String>,
    /// The manual checks' ids (answer-check).
    pub checks: Vec<String>,
    /// The host's OS templates (template-build's base).
    pub templates: Vec<String>,
    /// What an edit form's `open` reads first (the stack's files, the
    /// presets, host.toml, the roll-back list, the batch's previews).
    pub edit: Value,
}

pub struct Ctx<'a> {
    pub now: i64,
    pub by: &'a str,
    pub scope: Scope,
    /// The stacks of the fleet as the dashboard read it.
    pub stacks: &'a [String],
    pub sources: &'a Sources,
}

/// What the shell does after a step was applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    /// The review step is on screen (or force changed): read the guard and
    /// the CLI line.
    Preview,
    /// The final press: run the action with these arguments, once.
    Run(Box<ActionArgs>),
    /// An edit form needs the dashboard's server: a plan, the data
    /// folders, or its final press (the commit, host.toml, the batch).
    Edit(EditCall),
    /// fix-185 (`homelab ui reload`): tell the driven tab to take the
    /// dashboard's current page and wait, bounded, for it to re-attach.
    Reload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub effect: Effect,
    /// The press happened and is shown, but the form held it (a field in
    /// error, the deploy guard): the driver is told why.
    pub held: Option<Refusal>,
}

pub fn refused(step: &UiStep, why: impl Into<String>, fix: impl Into<String>) -> Refusal {
    Refusal::new(format!("ui {}", step.verb()), why, fix)
}

const BUTTONS: &[&str] = &[
    "next",
    "back",
    "confirm",
    "close",
    "save",
    "default",
    "cancel",
    // feat-native-1: native-remove's own button, beside "next".
    "remove",
    // feat-preset-1: the Files card's own three buttons and "Remove this
    // preset…", beside "next" (`EditState.action` names which one built
    // the current plan).
    "save-file",
    "rename-file",
    "delete-file",
    "remove-preset",
];

impl OpenForm {
    pub fn new(desc: ActionForm, sources: &Sources) -> Self {
        let values = initial_values(&desc);
        let mut choices = BTreeMap::new();
        for f in desc.fields().filter(|f| f.kind == FieldKind::Choice) {
            let list = match f.source.as_deref() {
                Some("apps") => sources.apps.clone(),
                Some("units") => sources.units.clone(),
                Some("commits") => sources.commits.clone(),
                Some("releases") => sources.releases.clone(),
                Some("checks") => sources.checks.clone(),
                Some("templates") => sources.templates.clone(),
                _ => Vec::new(),
            };
            choices.insert(f.name.clone(), list);
        }
        let mut form = OpenForm {
            id: desc.id.clone(),
            action: desc.action.slug().to_string(),
            stack: desc.stack.clone(),
            title: desc.title.clone(),
            steps: desc.steps.iter().map(|s| s.id.clone()).collect(),
            step: desc.steps[0].id.clone(),
            step_index: 0,
            values,
            errors: BTreeMap::new(),
            choices,
            guard: None,
            cli: None,
            restarts_dashboard: false,
            run_error: None,
            job: None,
            buttons: Vec::new(),
            fields: Vec::new(),
            edit: None,
            reads_repo: false,
            desc: FormDesc {
                family: Family::Action(desc.action),
                steps: desc.steps,
            },
        };
        form.refresh();
        form
    }

    /// An edit form (`driveedit::open`).
    pub fn new_edit(
        id: String,
        stack: String,
        title: String,
        steps: Vec<FormStep>,
        edit: EditState,
    ) -> Self {
        let mut values = Values::new();
        for f in steps.iter().flat_map(|s| s.fields.iter()) {
            values.insert(f.name.clone(), start_value(f));
        }
        let mut form = OpenForm {
            id,
            action: edit.family.slug().to_string(),
            stack,
            title,
            steps: steps.iter().map(|s| s.id.clone()).collect(),
            step: steps[0].id.clone(),
            step_index: 0,
            values,
            errors: BTreeMap::new(),
            choices: BTreeMap::new(),
            guard: None,
            cli: None,
            restarts_dashboard: false,
            run_error: None,
            job: None,
            buttons: Vec::new(),
            fields: Vec::new(),
            reads_repo: false,
            desc: FormDesc {
                family: Family::Edit(edit.family),
                steps,
            },
            edit: Some(edit),
        };
        form.refresh();
        form
    }

    pub fn last(&self) -> usize {
        self.steps.len() - 1
    }

    /// When a hidden field shows, in words; `None` while it is shown. Force
    /// is only on screen while the guard refuses, or once ticked; a
    /// `show_when` field only while the other field has its value.
    fn hidden_why(&self, f: &Field) -> Option<String> {
        let guard = f.when.as_deref() != Some("guard")
            || self.guard.is_some()
            || self.edit.as_ref().is_some_and(|e| e.guarded > 0)
            || self.values.get(&f.name) == Some(&Value::Bool(true));
        if !guard {
            return Some("shown only when the deploy guard refuses".into());
        }
        if shown_field(f, &self.values).is_none() {
            let says = f.show_when.as_ref().map_or("", |w| w.says.as_str());
            return Some(format!("shown only when {says}"));
        }
        None
    }

    /// Whether the final press was made: its job or its answer is there.
    pub fn sent(&self) -> bool {
        self.job.is_some() || self.edit.as_ref().is_some_and(|e| e.result.is_some())
    }

    /// The values a choice field may take.
    pub fn choice_values(&self, f: &Field) -> Vec<String> {
        match &f.choices {
            Some(c) => c.iter().map(|c| c.value.clone()).collect(),
            None => self.choices.get(&f.name).cloned().unwrap_or_default(),
        }
    }

    /// Recompute what a reader of the screen needs: the step, the buttons,
    /// every field with its value and error.
    pub fn refresh(&mut self) {
        self.steps = self.desc.steps.iter().map(|s| s.id.clone()).collect();
        self.step_index = self.step_index.min(self.steps.len().saturating_sub(1));
        self.step = self.steps[self.step_index].clone();
        self.reads_repo = reads_repo(self);
        if let Family::Edit(_) = self.desc.family {
            crate::core::driveedit::refresh(self);
            return;
        }
        self.buttons = if self.job.is_some() {
            vec!["close".into()]
        } else {
            let mut b = Vec::new();
            if self.step_index > 0 {
                b.push("back".to_string());
            }
            b.push(if self.step_index == self.last() {
                "confirm".into()
            } else {
                "next".into()
            });
            b.push("close".into());
            b
        };
        self.fields = self.field_states();
    }

    /// Every field on screen or behind a step, as a reader of the screen
    /// sees it: the form's own, then an open dialog's.
    pub fn field_states(&self) -> Vec<FieldState> {
        let mut out = Vec::new();
        for s in &self.desc.steps {
            for f in &s.fields {
                let hidden_why = self.hidden_why(f);
                out.push(FieldState {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    kind: f.kind,
                    label: shown_field(f, &self.values)
                        .map_or_else(|| f.label.clone(), |g| g.label.clone()),
                    step: s.id.clone(),
                    value: self.values.get(&f.name).cloned().unwrap_or(Value::Null),
                    error: self.errors.get(&f.name).cloned(),
                    shown: hidden_why.is_none(),
                    hidden_why,
                    choices: self.choice_values(f),
                });
            }
        }
        if let Some(sub) = self.edit.as_ref().and_then(|e| e.sub.as_ref()) {
            for f in &sub.fields {
                out.push(FieldState {
                    id: f.id.clone(),
                    name: f.name.clone(),
                    kind: f.kind,
                    label: f.label.clone(),
                    step: sub.kind.clone(),
                    value: sub.values.get(&f.name).cloned().unwrap_or(Value::Null),
                    error: sub.errors.get(&f.name).cloned(),
                    shown: true,
                    hidden_why: None,
                    choices: f
                        .choices
                        .as_ref()
                        .map(|c| c.iter().map(|c| c.value.clone()).collect())
                        .unwrap_or_default(),
                });
            }
        }
        out
    }

    fn field<'a>(&'a self, step: &UiStep, id: &str) -> Result<(&'a Field, &'a str), Refusal> {
        if let Some(sub) = self.edit.as_ref().and_then(|e| e.sub.as_ref())
            && let Some(f) = sub.fields.iter().find(|f| f.id == id)
        {
            return Ok((f, sub.kind.as_str()));
        }
        for s in &self.desc.steps {
            if let Some(f) = s.fields.iter().find(|f| f.id == id) {
                return Ok((f, s.id.as_str()));
            }
        }
        let sub = self.edit.as_ref().and_then(|e| e.sub.as_ref());
        let ids: Vec<&str> = sub
            .into_iter()
            .flat_map(|s| s.fields.iter())
            .chain(self.desc.fields())
            .map(|f| f.id.as_str())
            .collect();
        Err(refused(
            step,
            format!("the form {} has no field {id}", self.title),
            if ids.is_empty() {
                "this form has no fields; press next or confirm".to_string()
            } else {
                format!("its fields are: {}", ids.join(", "))
            },
        ))
    }

    /// The field, on the step on screen, shown.
    fn field_here<'a>(
        &'a self,
        step: &UiStep,
        id: &str,
        kinds: &[FieldKind],
        verb_for: fn(FieldKind) -> &'static str,
    ) -> Result<&'a Field, Refusal> {
        if self.sent() {
            return Err(refused(
                step,
                "the form was sent; its job runs",
                "homelab ui close, then open the form again for another run",
            ));
        }
        let (f, on) = self.field(step, id)?;
        if let Some(sub) = self.edit.as_ref().and_then(|e| e.sub.as_ref()) {
            if on != sub.kind {
                return Err(refused(
                    step,
                    format!("{id} is behind the open dialog {}", sub.title),
                    "press save or cancel in that dialog first",
                ));
            }
            if !kinds.contains(&f.kind) {
                return Err(refused(
                    step,
                    format!("{id} is a {:?} field", f.kind).to_lowercase(),
                    format!("use homelab ui {} {id} …", verb_for(f.kind)),
                ));
            }
            return Ok(f);
        }
        if !kinds.contains(&f.kind) {
            return Err(refused(
                step,
                format!("{id} is a {:?} field", f.kind).to_lowercase(),
                format!("use homelab ui {} {id} …", verb_for(f.kind)),
            ));
        }
        if on != self.step {
            return Err(refused(
                step,
                format!(
                    "{id} is on the step {on}; the step on screen is {}",
                    self.step
                ),
                "press next or back to reach it",
            ));
        }
        if let Some(why) = self.hidden_why(f) {
            let fix = match &f.show_when {
                Some(w) if f.when.as_deref() != Some("guard") => {
                    let other = self
                        .desc
                        .fields()
                        .find(|x| x.name == w.field)
                        .map_or(w.field.as_str(), |x| x.id.as_str());
                    format!("homelab ui pick {other} {} first", w.value)
                }
                _ => "nothing to set; the guard does not refuse this deploy".to_string(),
            };
            return Err(refused(
                step,
                format!("{id} is not on screen: it is {why}"),
                fix,
            ));
        }
        Ok(f)
    }
}

fn verb_for(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Check => "check",
        FieldKind::Choice => "pick",
        FieldKind::Text | FieldKind::Typed | FieldKind::Number => "type",
        FieldKind::Textarea => "edit",
    }
}

/// The value a field starts with: its `current`, else empty or off.
pub fn start_value(f: &Field) -> Value {
    match &f.current {
        Some(v) => v.clone(),
        None if f.kind == FieldKind::Check => Value::Bool(false),
        None => Value::String(String::new()),
    }
}

impl OpenForm {
    /// Set one field by its id, checked: the field is on screen, of one of
    /// `kinds`, and a choice's value is on its list. Returns the field's
    /// name and whether it is an open dialog's.
    pub fn set(
        &mut self,
        step: &UiStep,
        id: &str,
        kinds: &[FieldKind],
        value: Value,
    ) -> Result<(String, bool), Refusal> {
        let f = self.field_here(step, id, kinds, verb_for)?.clone();
        if f.kind == FieldKind::Choice {
            let v = value.as_str().unwrap_or_default().to_string();
            let choices = self.choice_values(&f);
            let listed = f.choices.is_some();
            if (listed || !v.is_empty()) && !choices.contains(&v) {
                return Err(refused(
                    step,
                    format!("{v} is not a choice of {id}"),
                    if choices.is_empty() {
                        "the list is empty; nothing can be picked".to_string()
                    } else {
                        format!("its choices are: {}", choices.join(", "))
                    },
                ));
            }
        }
        let sub = self.edit.as_mut().and_then(|e| e.sub.as_mut());
        let in_sub = sub
            .as_ref()
            .is_some_and(|s| s.fields.iter().any(|x| x.id == id));
        match sub {
            Some(s) if in_sub => {
                s.values.insert(f.name.clone(), value);
                s.errors.remove(&f.name);
            }
            _ => {
                self.values.insert(f.name.clone(), value);
                self.errors.remove(&f.name);
            }
        }
        Ok((f.name, in_sub))
    }
}

/// Is `path` a page of the dashboard? The page list is `formspec.json`'s,
/// which a web test holds equal to the router's. nav-decisions
/// (chassis-rs 3.1.0): every page lives at the root now, so `path` is
/// checked as-is rather than stripped of an `/app` prefix.
fn known_page(path: &str, stacks: &[String]) -> Result<String, String> {
    let p = path.split(['?', '#']).next().unwrap_or("");
    if !p.starts_with('/') {
        return Err(format!("{path} is not an absolute path"));
    }
    let rest = p.trim_start_matches('/').trim_end_matches('/');
    if spec().pages.iter().any(|x| x == rest) {
        return Ok(format!("/{rest}"));
    }
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.first() == Some(&"stacks") && (parts.len() == 2 || parts.len() == 3) {
        let name = parts[1];
        if !stacks.iter().any(|s| s == name) {
            return Err(format!("the fleet has no stack {name}"));
        }
        let tab = parts.get(2).copied().unwrap_or("overview");
        if !spec().stack_tabs.iter().any(|t| t == tab) {
            return Err(format!(
                "a stack page has no tab {tab}; its tabs are {}",
                spec().stack_tabs.join(", ")
            ));
        }
        return Ok(if tab == "overview" {
            format!("/stacks/{name}")
        } else {
            format!("/stacks/{name}/{tab}")
        });
    }
    Err(format!("there is no page at {path}"))
}

impl DriveState {
    /// fix-163 (Kenny, 2026-09-29): is the drive due to be released? The
    /// open dialog was confirmed and its job has ended (`job_finished_at`,
    /// unix seconds), and no step came for `RELEASE_AFTER_JOB_S` after the
    /// later of that end and the last step. Never while a step is
    /// announced or a viewer paused, and never a drive that is not active.
    pub fn release_due(&self, now: i64, job_finished_at: Option<i64>) -> bool {
        let Some(ended) = job_finished_at else {
            return false;
        };
        self.active
            && self.announce.is_none()
            && self.paused_by.is_none()
            && self.stopped_by.is_none()
            && self.form.as_ref().is_some_and(|f| f.job.is_some())
            && now - ended.max(self.last_at) >= self.release_after_job_s
    }

    /// Release the drive as `homelab ui done` does: the dialog closes and
    /// the tabs are the viewer's again. One step on, so every tab hears it.
    pub fn release(&mut self, now: i64) {
        self.form = None;
        self.active = false;
        self.announce = None;
        self.paused_by = None;
        self.plan = None;
        self.seq += 1;
        self.last_at = now;
    }

    /// The state as a reader sees it now: a driver silent past `idle_s` no
    /// longer holds the tabs.
    pub fn snapshot(&self, now: i64) -> DriveState {
        let mut s = self.clone();
        s.active = s.active && now - s.last_at < s.idle_s;
        s
    }

    /// Apply one step. `Err`: refused, nothing changed. `Ok`: applied (the
    /// press may have been held by the form, which `held` says).
    ///
    /// Live view: a drive a viewer stopped refuses every step until `done`;
    /// `plan` records the sequence; a step taken moves the plan on.
    pub fn apply(&mut self, step: &UiStep, cx: &Ctx) -> Result<Applied, Refusal> {
        let plain = Applied {
            effect: Effect::None,
            held: None,
        };
        match step {
            UiStep::State => return Ok(plain),
            UiStep::Done => {
                self.stopped_by = None;
                self.paused_by = None;
                self.announce = None;
                self.plan = None;
            }
            _ => {
                if let Some(who) = &self.stopped_by {
                    return Err(drivelive::stopped_refusal(step, who));
                }
            }
        }
        if let UiStep::Plan { steps } = step {
            self.plan = Some(drivelive::new_plan(steps, cx.by, self)?);
            self.bump(cx, true);
            return Ok(plain);
        }
        let applied = self.apply_step(step, cx)?;
        if drivelive::holds(step)
            && let Some(p) = self.plan.as_mut()
        {
            p.advance(step);
        }
        Ok(applied)
    }

    fn apply_step(&mut self, step: &UiStep, cx: &Ctx) -> Result<Applied, Refusal> {
        let plain = Applied {
            effect: Effect::None,
            held: None,
        };
        let applied = match step {
            UiStep::State | UiStep::Plan { .. } => return Ok(plain),
            UiStep::Done => {
                self.form = None;
                self.active = false;
                self.bump(cx, false);
                return Ok(plain);
            }
            UiStep::Goto { path } => {
                if let Some(f) = &self.form {
                    return Err(refused(
                        step,
                        format!("the dialog {} is open", f.title),
                        "homelab ui close first",
                    ));
                }
                let page = known_page(path, cx.stacks).map_err(|why| {
                    refused(
                        step,
                        why,
                        format!(
                            "pages: /, /{}; a stack: /stacks/<name>[/<tab>]",
                            spec()
                                .pages
                                .iter()
                                .filter(|p| !p.is_empty())
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", /")
                        ),
                    )
                })?;
                self.page = page;
                plain
            }
            UiStep::Open { form, target } => {
                if let Some(f) = &self.form {
                    return Err(refused(
                        step,
                        format!("the dialog {} is open already", f.title),
                        "homelab ui close first",
                    ));
                }
                if let Some(kind) = EditKind::from_form(form) {
                    let applied = crate::core::driveedit::open(
                        self,
                        step,
                        kind,
                        form,
                        target.as_deref(),
                        cx,
                    )?;
                    self.bump(cx, true);
                    return Ok(applied);
                }
                let Some(kind) = ActionKind::from_slug(form) else {
                    let all: Vec<&str> = ActionKind::ALL.iter().map(|k| k.slug()).collect();
                    let edit: Vec<&str> = EditKind::ALL.iter().map(|k| k.usage()).collect();
                    return Err(refused(
                        step,
                        format!("there is no form {form}"),
                        format!(
                            "the forms are the actions: {}; and the edit forms: {}",
                            all.join(", "),
                            edit.join(", ")
                        ),
                    ));
                };
                let stack = if kind.host_wide() {
                    if target.as_deref().is_some_and(|t| t != HOST_TARGET) {
                        return Err(refused(
                            step,
                            format!("{form} acts on the whole host, not on a stack"),
                            format!("homelab ui open {form}"),
                        ));
                    }
                    HOST_TARGET.to_string()
                } else {
                    let Some(t) = target else {
                        return Err(refused(
                            step,
                            format!("{form} acts on one stack and none was named"),
                            format!("homelab ui open {form} <stack>"),
                        ));
                    };
                    // feat-retired-1: a wipe's target left the fleet on
                    // purpose (that is what retired it); it is never in
                    // `cx.stacks`, so the membership check below is for
                    // every OTHER action only. The key's shape is checked
                    // when the form is actually sent (`actions::validate`);
                    // a stale or misspelled key surfaces there, same as a
                    // click from the Retired page would.
                    if !kind.targets_retired() && !cx.stacks.iter().any(|s| s == t) {
                        return Err(refused(
                            step,
                            format!("the fleet has no stack {t}"),
                            format!("the stacks are: {}", cx.stacks.join(", ")),
                        ));
                    }
                    t.clone()
                };
                let desc = action_form(kind, &stack);
                if let Some(why) = &desc.refused {
                    return Err(refused(step, why.clone(), "leave this stack to the CLI"));
                }
                if kind.scope() > cx.scope {
                    return Err(refused(
                        step,
                        format!(
                            "{form} needs scope {:?}; the token \"{}\" has {:?}",
                            kind.scope(),
                            cx.by,
                            cx.scope
                        ),
                        "drive it with a token of that scope, or leave it to Kenny",
                    ));
                }
                self.page = if kind.host_wide() {
                    "/host".into()
                } else if self.page.starts_with(&format!("/stacks/{stack}")) {
                    self.page.clone()
                } else {
                    format!("/stacks/{stack}")
                };
                let open = OpenForm::new(desc, cx.sources);
                let effect = if open.step == REVIEW {
                    Effect::Preview
                } else {
                    Effect::None
                };
                self.form = Some(open);
                Applied { effect, held: None }
            }
            UiStep::Select { stacks } => {
                if self.page != "/overview" {
                    return Err(refused(
                        step,
                        format!(
                            "the fleet table's selection lives on the Overview page, not {}",
                            self.page
                        ),
                        "homelab ui goto /overview first",
                    ));
                }
                let mut unknown: Vec<&String> = stacks
                    .iter()
                    .filter(|s| !cx.stacks.iter().any(|x| x == *s))
                    .collect();
                unknown.sort();
                unknown.dedup();
                if !unknown.is_empty() {
                    let names: Vec<&str> = unknown.iter().map(|s| s.as_str()).collect();
                    return Err(refused(
                        step,
                        format!("the fleet has no stack {}", names.join(", ")),
                        format!("the stacks are: {}", cx.stacks.join(", ")),
                    ));
                }
                let mut list = stacks.clone();
                list.sort();
                list.dedup();
                self.selected = list;
                plain
            }
            UiStep::Close => {
                if self.form.take().is_none() {
                    return Err(refused(step, "no dialog is open", "nothing to close"));
                }
                plain
            }
            // fix-185: nothing on the shared state changes; the shell tells
            // the driven tab to reload and waits for it to re-attach.
            UiStep::Reload => Applied {
                effect: Effect::Reload,
                held: None,
            },
            UiStep::Type { field, text } => {
                let form = self.open_form(step)?;
                // dashboard-latest, back-compat: `tag` was a text field
                // before the "release tag" dropdown; `ui type act-tag
                // v3.63.0` still sets it, checked as a pick would be.
                let kinds = [
                    FieldKind::Text,
                    FieldKind::Typed,
                    FieldKind::Number,
                    FieldKind::Textarea,
                    FieldKind::Choice,
                ];
                let (name, sub) = form.set(step, field, &kinds, Value::String(text.clone()))?;
                crate::core::driveedit::after_set(form, &name, sub);
                // cli-yes: the typed name decides whether the CLI line
                // carries --yes, and a field typed on the review step (exec's
                // command) changes the line, so the review reads it again.
                let action = matches!(form.desc.family, Family::Action(_));
                Applied {
                    effect: if action && form.step == REVIEW && !sub {
                        Effect::Preview
                    } else {
                        Effect::None
                    },
                    held: None,
                }
            }
            UiStep::Edit { field, text } => {
                let form = self.open_form(step)?;
                let kinds = [FieldKind::Textarea, FieldKind::Text];
                let (name, sub) = form.set(step, field, &kinds, Value::String(text.clone()))?;
                crate::core::driveedit::after_set(form, &name, sub);
                plain
            }
            UiStep::Pick { field, value } => {
                let form = self.open_form(step)?;
                let (name, sub) = form.set(
                    step,
                    field,
                    &[FieldKind::Choice],
                    Value::String(value.clone()),
                )?;
                crate::core::driveedit::after_set(form, &name, sub);
                plain
            }
            UiStep::Check { field, on } => {
                let form = self.open_form(step)?;
                let (name, sub) = form.set(step, field, &[FieldKind::Check], Value::Bool(*on))?;
                crate::core::driveedit::after_set(form, &name, sub);
                let action = matches!(form.desc.family, Family::Action(_));
                Applied {
                    effect: if name == "force" && action {
                        Effect::Preview
                    } else {
                        Effect::None
                    },
                    held: None,
                }
            }
            UiStep::Row { op, target } => {
                let form = self.open_form(step)?;
                crate::core::driveedit::row(form, step, op, target.as_deref())?
            }
            UiStep::Press { button } => self.press(step, button, cx)?,
        };
        self.bump(cx, true);
        Ok(applied)
    }

    fn bump(&mut self, cx: &Ctx, active: bool) {
        self.seq += 1;
        self.active = active;
        self.by = Some(cx.by.to_string());
        self.last_at = cx.now;
        if let Some(f) = self.form.as_mut() {
            f.refresh();
        }
    }

    pub fn open_form(&mut self, step: &UiStep) -> Result<&mut OpenForm, Refusal> {
        self.form.as_mut().ok_or_else(|| {
            refused(
                step,
                "no dialog is open",
                "homelab ui open <action> <stack> first",
            )
        })
    }

    fn press(&mut self, step: &UiStep, button: &str, cx: &Ctx) -> Result<Applied, Refusal> {
        if !BUTTONS.contains(&button) {
            return Err(refused(
                step,
                format!("there is no button {button}"),
                format!("the buttons are: {}", BUTTONS.join(", ")),
            ));
        }
        if button == "close" {
            if self.form.take().is_none() {
                return Err(refused(step, "no dialog is open", "nothing to close"));
            }
            return Ok(Applied {
                effect: Effect::None,
                held: None,
            });
        }
        let form = self.open_form(step)?;
        if let Family::Edit(_) = form.desc.family {
            return crate::core::driveedit::press(form, step, button, cx);
        }
        if let Some(j) = &form.job {
            return Err(refused(
                step,
                format!(
                    "this form was sent already: job {} runs it; one press runs once",
                    j.job
                ),
                "homelab ui state follows the job; close the dialog to start over",
            ));
        }
        if !form.buttons.iter().any(|b| b == button) {
            return Err(refused(
                step,
                format!("{button} is not on screen on the step {}", form.step),
                format!("the buttons on screen are: {}", form.buttons.join(", ")),
            ));
        }
        let held = |what: &str, errors: &BTreeMap<String, String>, form: &OpenForm| {
            let list: Vec<String> = errors
                .iter()
                .map(|(name, e)| {
                    let id = form
                        .desc
                        .fields()
                        .find(|f| &f.name == name)
                        .map_or(name.as_str(), |f| f.id.as_str());
                    format!("{id}: {e}")
                })
                .collect();
            Refusal::new(
                format!("ui press {what}"),
                format!("the form holds: {}", list.join("; ")),
                "correct the named fields and press again",
            )
        };
        match button {
            "back" => {
                form.step_index -= 1;
                form.errors.clear();
                Ok(Applied {
                    effect: Effect::None,
                    held: None,
                })
            }
            "next" => {
                let errors = check_steps(&form.desc.steps, &form.values, Some(&form.step));
                if !errors.is_empty() {
                    let r = held("next", &errors, form);
                    form.errors = errors;
                    return Ok(Applied {
                        effect: Effect::None,
                        held: Some(r),
                    });
                }
                form.errors.clear();
                form.step_index += 1;
                form.refresh();
                Ok(Applied {
                    effect: if form.step == REVIEW {
                        Effect::Preview
                    } else {
                        Effect::None
                    },
                    held: None,
                })
            }
            _ => {
                // confirm: the final press.
                let Family::Action(kind) = form.desc.family else {
                    return Err(refused(step, "not an action form", "homelab ui state"));
                };
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
                        "leave this press to Kenny",
                    ));
                }
                let errors = check_steps(&form.desc.steps, &form.values, None);
                if !errors.is_empty() {
                    let r = held("confirm", &errors, form);
                    form.errors = errors;
                    return Ok(Applied {
                        effect: Effect::None,
                        held: Some(r),
                    });
                }
                if let Some(g) = &form.guard
                    && form.values.get("force") != Some(&Value::Bool(true))
                {
                    let r = Refusal::new(
                        form.title.clone(),
                        "the deploy guard refuses this deploy",
                        "pull the working copy first, or homelab ui check act-force on if undoing the host's last deploy is the point",
                    );
                    form.run_error = Some(r.clone());
                    let _ = g;
                    return Ok(Applied {
                        effect: Effect::None,
                        held: Some(r),
                    });
                }
                form.run_error = None;
                Ok(Applied {
                    effect: Effect::Run(Box::new(build_args_of(form.desc.fields(), &form.values))),
                    held: None,
                })
            }
        }
    }
}
