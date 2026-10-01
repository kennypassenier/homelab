//! feat-platform-10 (milestone follow), the edit forms: Claude drives the
//! stack settings form and its raw editor, "add an app", the firewall with
//! its rule dialog, the new-stack wizard, the host settings, the batch
//! dialog and the roll back dialog, the way `drive` drives an action's
//! dialog.
//!
//! The fields' words come from `formspec.json`'s `edit` section, the same
//! description `editforms.js` draws the browser's forms from; the checks
//! here are the browser's (`checkFields`, `ruleProblems`, `checkNewStep`,
//! `parseKey`), held equal by the cases file both suites read. The final
//! press (the commit and push, host.toml, the batch) is an `EditCall` the
//! shell runs through the very function a click's route runs. Pure.

use std::collections::{BTreeMap, BTreeSet};

use homelab_proto::{Scope, UiStep};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::core::actions::{ActionKind, Arg, Refusal, SELF_STACK};
use crate::core::drive::{
    self, fill, refused, spec, Applied, Choice, Ctx, DriveState, Effect, Family, Field, FieldKind,
    FormStep, OpenForm, Sources, Values,
};
use crate::core::presetedit;

// ── the description ─────────────────────────────────────────────────────

/// One field of an edit form in `formspec.json`.
#[derive(Debug, Clone, Deserialize)]
pub struct EditFieldDef {
    pub name: String,
    pub id: String,
    pub kind: FieldKind,
    pub label: String,
    #[serde(default)]
    pub help: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub placeholder: Option<String>,
    #[serde(default)]
    pub choices: Option<Vec<Choice>>,
    #[serde(default)]
    pub current: Option<Value>,
}

impl EditFieldDef {
    pub fn field(&self) -> Field {
        Field {
            name: self.name.clone(),
            id: self.id.clone(),
            kind: self.kind,
            label: self.label.clone(),
            help: self.help.clone(),
            required: self.required,
            pattern: self.pattern.clone(),
            placeholder: self.placeholder.clone(),
            source: None,
            empty: None,
            danger: false,
            when: None,
            expect: None,
            min: self.min,
            max: self.max,
            choices: self.choices.clone(),
            current: self.current.clone(),
            show_when: None,
            change_when: None,
        }
    }

    /// With `{word}`s filled in (an image's key, a data folder's path).
    fn filled(&self, words: &[(&str, &str)]) -> Field {
        let mut f = self.field();
        f.name = fill(&f.name, words);
        f.id = fill(&f.id, words);
        f.label = fill(&f.label, words);
        f.help = fill(&f.help, words);
        f
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EditStepDef {
    pub id: String,
    pub label: String,
    pub fields: Vec<EditFieldDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewDefaults {
    pub ram_mb: u64,
    pub cores: u64,
    pub disk_gb: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HostKeyIds {
    pub id: String,
    pub confirm_id: String,
    pub confirm_label: String,
}

/// `formspec.json` → `edit`.
#[derive(Debug, Clone, Deserialize)]
pub struct EditSpec {
    pub messages: BTreeMap<String, String>,
    pub no_touch: Vec<i64>,
    pub follow: BTreeMap<String, String>,
    pub settings: Vec<EditFieldDef>,
    pub image: EditFieldDef,
    pub tile_watch: EditFieldDef,
    pub tile_down: EditFieldDef,
    pub commit: Vec<EditFieldDef>,
    pub raw: Vec<EditFieldDef>,
    pub add_app: Vec<EditFieldDef>,
    /// feat-native-1: the native-service form (one unit's `service.yml`).
    pub native: Vec<EditFieldDef>,
    /// feat-native-1: "add a native unit".
    pub add_native: Vec<EditFieldDef>,
    pub firewall: Vec<EditFieldDef>,
    pub rule: Vec<EditFieldDef>,
    pub new_stack: Vec<EditStepDef>,
    pub new_defaults: NewDefaults,
    pub nodata: EditFieldDef,
    pub host_key: HostKeyIds,
    pub rollback: Vec<EditFieldDef>,
    /// TUI parity: the import form's first step (the bundle, the name, the
    /// container number).
    pub import: Vec<EditFieldDef>,
    /// feat-preset-1: `preset.yml`'s own fields, shared by `preset` (an
    /// existing one) and `new-preset`.
    pub preset_meta: Vec<EditFieldDef>,
    /// feat-preset-1: `new-preset`'s own first field, ahead of
    /// `preset_meta`.
    pub new_preset_name: EditFieldDef,
    /// feat-preset-1: the Files card's own fields, on the same "meta" step.
    pub preset_file: Vec<EditFieldDef>,
    /// feat-stacks-9: the settings form's second page.
    pub settings_ext: Vec<EditFieldDef>,
    /// feat-stacks-10: one app's "remove" checkbox / the blank-app field.
    pub apps_remove: EditFieldDef,
    pub apps_add_blank: EditFieldDef,
    /// feat-tiles-3: one app's optional tile hostname, on the add-app step.
    pub add_app_tile: EditFieldDef,
    pub storage_entry: Vec<EditFieldDef>,
    pub data_mount: Vec<EditFieldDef>,
    pub log_file: Vec<EditFieldDef>,
    /// W2: the settings-ext form's retention row dialog.
    pub retention_tier: Vec<EditFieldDef>,
    /// feat-stacks-11: one app's `latch_secrets` checkbox.
    pub latch_secret: EditFieldDef,
    pub latch_file: Vec<EditFieldDef>,
    /// feat-checks-1: the checks form's row dialogs and its scalar fields
    /// (which app, the busy check, the link).
    pub check_row: Vec<EditFieldDef>,
    pub probe_row: Vec<EditFieldDef>,
    pub manual_row: Vec<EditFieldDef>,
    pub checks_scalar: Vec<EditFieldDef>,
    /// feat-tiles-1: one tile's row dialog.
    pub tile_row: Vec<EditFieldDef>,
    /// feat-publish-1: the publish-app dialog.
    pub publish: Vec<EditFieldDef>,
}

fn es() -> &'static EditSpec {
    &spec().edit
}

/// A message of the edit forms, `{word}`s filled in.
pub fn say(key: &str, words: &[(&str, &str)]) -> String {
    fill(
        es().messages.get(key).map(String::as_str).unwrap_or(key),
        words,
    )
}

// ── the forms ───────────────────────────────────────────────────────────

/// The edit forms Claude can open, besides the actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum EditKind {
    Settings,
    Raw,
    AddApp,
    Firewall,
    NewStack,
    HostSettings,
    Batch,
    Rollback,
    /// TUI parity (`homelab import`): a bundle as a new stack.
    Import,
    /// feat-native-1: one native unit's `service.yml` fields. Target
    /// `<stack>/<unit>` (`<stack>` alone picks its first native unit).
    Native,
    /// feat-native-1: a new native unit.
    AddNative,
    /// feat-preset-1: an existing preset's `preset.yml` and files. Target
    /// the preset's name.
    Preset,
    /// feat-preset-1: a new preset (name typed in the form itself).
    NewPreset,
    /// feat-stacks-9: network, lxc flags, resources.storage, on_demand,
    /// retention — the settings form's second page.
    SettingsExt,
    /// feat-stacks-10: the Apps tab — remove/add-blank an app, storage,
    /// data_mounts, log_files.
    Apps,
    /// feat-stacks-11: `latch_secrets` and `latch_files`.
    Latch,
    /// feat-checks-1: one app's whole `checks.yml`. Target `<stack>/<app>`.
    Checks,
    /// feat-tiles-1: the stack's `tiles:` map, created/renamed/removed.
    Tiles,
    /// feat-publish-1: one app's route + optional tile. Target
    /// `<stack>/<app>`.
    PublishApp,
}

impl EditKind {
    pub const ALL: [EditKind; 19] = [
        EditKind::Settings,
        EditKind::Raw,
        EditKind::AddApp,
        EditKind::Firewall,
        EditKind::NewStack,
        EditKind::HostSettings,
        EditKind::Batch,
        EditKind::Rollback,
        EditKind::Import,
        EditKind::Native,
        EditKind::AddNative,
        EditKind::Preset,
        EditKind::NewPreset,
        EditKind::SettingsExt,
        EditKind::Apps,
        EditKind::Latch,
        EditKind::Checks,
        EditKind::Tiles,
        EditKind::PublishApp,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            EditKind::Settings => "settings",
            EditKind::Raw => "raw",
            EditKind::AddApp => "add-app",
            EditKind::Firewall => "firewall",
            EditKind::NewStack => "new-stack",
            EditKind::HostSettings => "host-settings",
            EditKind::Batch => "batch",
            EditKind::Rollback => "rollback",
            EditKind::Import => "import",
            EditKind::Native => "native",
            EditKind::AddNative => "add-native",
            EditKind::Preset => "preset",
            EditKind::NewPreset => "new-preset",
            EditKind::SettingsExt => "settings-ext",
            EditKind::Apps => "apps",
            EditKind::Latch => "latch",
            EditKind::Checks => "checks",
            EditKind::Tiles => "tiles",
            EditKind::PublishApp => "publish",
        }
    }

    /// How `homelab ui open` names it.
    pub fn usage(self) -> &'static str {
        match self {
            EditKind::Settings => "settings <stack>",
            EditKind::Raw => "raw <stack>",
            EditKind::AddApp => "add-app <stack>",
            EditKind::Firewall => "firewall <stack>",
            EditKind::NewStack => "new-stack",
            EditKind::HostSettings => "host-settings",
            EditKind::Batch => "batch <action> <stack>,<stack>",
            EditKind::Rollback => "rollback <stack>",
            EditKind::Import => "import",
            EditKind::Native => "native <stack>[/<unit>]",
            EditKind::AddNative => "add-native <stack>",
            EditKind::Preset => "preset <name>",
            EditKind::NewPreset => "new-preset",
            EditKind::SettingsExt => "settings-ext <stack>",
            EditKind::Apps => "apps <stack>",
            EditKind::Latch => "latch <stack>",
            EditKind::Checks => "checks <stack>/<app>",
            EditKind::Tiles => "tiles <stack>",
            EditKind::PublishApp => "publish <stack>/<app>",
        }
    }

    /// `batch:<action>` is the batch form of that action.
    pub fn from_form(form: &str) -> Option<EditKind> {
        if form == "batch" || form.starts_with("batch:") {
            return Some(EditKind::Batch);
        }
        EditKind::ALL.iter().copied().find(|k| k.slug() == form)
    }

    /// A stack's own edit, ending in the plan and the commit.
    pub fn stack_edit(self) -> bool {
        matches!(
            self,
            EditKind::Settings
                | EditKind::Raw
                | EditKind::AddApp
                | EditKind::Firewall
                | EditKind::Native
                | EditKind::AddNative
                | EditKind::SettingsExt
                | EditKind::Apps
                | EditKind::Latch
                | EditKind::Checks
                | EditKind::Tiles
                | EditKind::PublishApp
        )
    }

    /// Storage/data_mounts/log_files/latch_files: the row dialogs Apps
    /// and Latch share (`Sub.kind` is the list's own name for these).
    pub fn row_list_kind(name: &str) -> bool {
        matches!(
            name,
            "storage"
                | "data_mounts"
                | "log_files"
                | "latch_files"
                | "checks"
                | "manual"
                | "probes"
                | "tiles"
                | "retention"
        )
    }

    /// A preset edit, ending in its own plan and commit
    /// (`/data/presets/plan`, `/data/presets/commit` — not a stack's).
    pub fn preset_edit(self) -> bool {
        matches!(self, EditKind::Preset | EditKind::NewPreset)
    }

    /// The scope a driving token needs to open it. host.toml can take the
    /// backups or the dashboard's route down: `all` (Kenny may lower it).
    pub fn scope(self) -> Scope {
        match self {
            EditKind::HostSettings => Scope::All,
            _ => Scope::Operate,
        }
    }
}

/// A dialog on top of the form: the firewall's rule dialog, a host.toml
/// key's dialog.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sub {
    /// `rule` or `key`.
    pub kind: String,
    pub title: String,
    /// The rule's index (from 0) or the key; null for a new rule.
    pub target: Value,
    #[serde(skip)]
    pub fields: Vec<Field>,
    pub values: Values,
    pub errors: BTreeMap<String, String>,
}

/// The edit form's own state.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EditState {
    pub family: EditKind,
    /// The firewall as the editor holds it (`firewallModel`'s shape).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<Value>,
    /// The table in words: the firewall's rules, host.toml's keys.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<Sub>,
    /// The plan, in short.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan: Option<Value>,
    /// What the final press answered: the commit, host.toml, the batch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// host.toml: the changes kept, by key, and the keys whose name was
    /// typed.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub staged: BTreeMap<String, Value>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub confirmed: BTreeSet<String>,
    /// The batch: the action and its stacks, and how many guards refuse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_action: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub stacks: Vec<String>,
    pub guarded: usize,
    /// feat-preset-1: which named button built the current `body`, when
    /// it was not the form's own "next"/review (`save-file`,
    /// `rename-file`, `delete-file`, `remove-preset`) — the browser's
    /// `sync` clicks that DOM button to open the real plan dialog, the
    /// same way it always clicked "review" before there was more than one
    /// button to choose from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// The edit the plan was made for; the commit sends the same.
    #[serde(skip)]
    pub body: Option<Value>,
    /// What `open` read.
    #[serde(skip)]
    pub data: Value,
}

impl EditState {
    fn new(family: EditKind, data: Value) -> Self {
        EditState {
            family,
            model: None,
            rows: Vec::new(),
            sub: None,
            plan: None,
            result: None,
            staged: BTreeMap::new(),
            confirmed: BTreeSet::new(),
            batch_action: None,
            stacks: Vec::new(),
            guarded: 0,
            action: None,
            body: None,
            data,
        }
    }
}

/// What the shell runs for an edit form, through the same functions the
/// routes a click reaches run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCall {
    /// `POST /data/stacks/{stack}/plan`.
    Plan { stack: String, edit: Value },
    /// `POST /data/stacks/{stack}/commit`: the final press.
    Commit { stack: String, body: Value },
    /// `POST /data/stacks-new/appdata`.
    Appdata { body: Value },
    /// `POST /data/stacks-new/plan`.
    NewPlan { body: Value },
    /// `POST /data/stacks-new/commit`: the final press.
    NewCommit { body: Value },
    /// `PUT /data/host-settings`: the final press.
    HostWrite { body: Value },
    /// `POST /data/actions/batch`: the final press.
    Batch { body: Value },
    /// `POST /data/stacks-import/plan`.
    ImportPlan { body: Value },
    /// `POST /data/stacks-import/commit`: the final press.
    ImportCommit { body: Value },
    /// `POST /data/presets/plan` (feat-preset-1: `preset` and `new-preset`
    /// share this — a preset is not under any stack).
    PresetPlan { edit: Value },
    /// `POST /data/presets/commit`: the final press.
    PresetCommit { body: Value },
}

impl EditCall {
    /// Whether this is a form's final press.
    pub fn final_press(&self) -> bool {
        matches!(
            self,
            EditCall::Commit { .. }
                | EditCall::NewCommit { .. }
                | EditCall::HostWrite { .. }
                | EditCall::Batch { .. }
                | EditCall::ImportCommit { .. }
                | EditCall::PresetCommit { .. }
        )
    }
}

// ── the browser's checks, in the browser's words ────────────────────────

fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

fn all_digits(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

fn matches(pattern: &str, text: &str) -> bool {
    regex::Regex::new(&format!("^(?:{pattern})$"))
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// `checkFields`: what is wrong with the values, by field name.
pub fn check_fields<'a>(
    fields: impl IntoIterator<Item = &'a Field>,
    values: &Values,
) -> BTreeMap<String, String> {
    let mut errors = BTreeMap::new();
    for f in fields {
        if f.kind == FieldKind::Check {
            continue;
        }
        let text = match values.get(&f.name) {
            Some(Value::String(s)) => s.trim().to_string(),
            _ => String::new(),
        };
        if text.is_empty() {
            if f.required {
                errors.insert(f.name.clone(), say("needed", &[("label", &f.label)]));
            }
            continue;
        }
        if f.kind == FieldKind::Number {
            let (min, max) = (f.min.unwrap_or(0), f.max.unwrap_or(i64::MAX));
            let ok =
                all_digits(&text) && text.parse::<i64>().is_ok_and(|n| (min..=max).contains(&n));
            if !ok {
                let (lo, hi) = (min.to_string(), max.to_string());
                errors.insert(
                    f.name.clone(),
                    say("number", &[("label", &f.label), ("min", &lo), ("max", &hi)]),
                );
            }
        } else if f.kind == FieldKind::Typed && Some(&text) != f.expect.as_ref() {
            errors.insert(
                f.name.clone(),
                say("typed", &[("expect", f.expect.as_deref().unwrap_or(""))]),
            );
        } else if f.pattern.as_deref().is_some_and(|p| !matches(p, &text)) {
            let lower = f.label.to_lowercase();
            errors.insert(
                f.name.clone(),
                say("pattern", &[("text", &text), ("lower", &lower)]),
            );
        }
    }
    errors
}

/// `ruleProblems`: what Proxmox would reject or misread, by field.
pub fn rule_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let peer = text_of(v.get("peer")).trim().to_string();
    if !peer.is_empty() {
        let re = regex::Regex::new(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?:/(\d{1,2}))?$")
            .expect("the address pattern reads");
        match re.captures(&peer) {
            Some(c) if (1..=4).all(|i| c[i].parse::<u32>().unwrap_or(999) <= 255) => {
                if let Some(bits) = c.get(5) {
                    let bits: u32 = bits.as_str().parse().unwrap_or(99);
                    let o = |i: usize| c[i].parse::<u32>().unwrap_or(0);
                    let ip = (o(1) << 24) | (o(2) << 16) | (o(3) << 8) | o(4);
                    if bits > 32 {
                        out.insert("peer".into(), say("peer_prefix", &[("peer", &peer)]));
                    } else {
                        let mask = if bits == 0 {
                            0
                        } else {
                            u32::MAX << (32 - bits)
                        };
                        if ip & mask != ip {
                            out.insert("peer".into(), say("peer_host_bits", &[("peer", &peer)]));
                        }
                    }
                }
            }
            _ => {
                out.insert("peer".into(), say("peer_form", &[("peer", &peer)]));
            }
        }
    }
    let ports: String = text_of(v.get("dport"))
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let proto = text_of(v.get("proto"));
    if !ports.is_empty() {
        if proto.is_empty() || proto == "icmp" {
            let key = if proto == "icmp" {
                "icmp_ports"
            } else {
                "port_proto"
            };
            out.insert("dport".into(), say(key, &[]));
        } else {
            let ok = |n: Option<f64>| {
                n.is_some_and(|n| n.fract() == 0.0 && (1.0..=65535.0).contains(&n))
            };
            let num = |s: Option<&str>| -> Option<f64> {
                let s = s?;
                if s.is_empty() {
                    Some(0.0)
                } else {
                    s.parse::<f64>().ok().filter(|n| n.is_finite())
                }
            };
            for part in ports.split(',') {
                let mut it = part.split(':');
                let a = num(it.next());
                let b = num(it.next());
                let bad = !ok(a) || (part.contains(':') && (!ok(b) || b <= a));
                if bad {
                    out.insert("dport".into(), say("port_range", &[("part", part)]));
                    break;
                }
            }
        }
    }
    if text_of(v.get("note")).contains('\n') {
        out.insert("note".into(), say("note_line", &[]));
    }
    out
}

/// `ruleFromValues`.
pub fn rule_from_values(v: &Values) -> Value {
    let s = |k: &str| text_of(v.get(k)).trim().to_string();
    let dir = if s("dir") == "out" { "out" } else { "in" };
    let mut r = Map::new();
    r.insert("dir".into(), json!(dir));
    let action = s("action");
    r.insert(
        "action".into(),
        json!(if action.is_empty() {
            "ACCEPT".to_string()
        } else {
            action
        }),
    );
    if !s("peer").is_empty() {
        r.insert(
            if dir == "in" { "source" } else { "dest" }.into(),
            json!(s("peer")),
        );
    }
    if !s("proto").is_empty() {
        r.insert("proto".into(), json!(s("proto")));
    }
    if !s("dport").is_empty() {
        let d: String = s("dport").chars().filter(|c| !c.is_whitespace()).collect();
        r.insert("dport".into(), json!(d));
    }
    if !s("note").is_empty() {
        r.insert("note".into(), json!(s("note")));
    }
    let c = text_of(v.get("comment")).trim_end().to_string();
    if !c.trim().is_empty() {
        r.insert("comment".into(), json!(c));
    }
    Value::Object(r)
}

fn clean_rule(r: &Value) -> Value {
    let mut out = Map::new();
    out.insert("dir".into(), r["dir"].clone());
    out.insert("action".into(), r["action"].clone());
    for k in ["source", "dest", "proto", "dport", "comment", "note"] {
        let v = &r[k];
        if v.is_null() {
            continue;
        }
        let t = text_of(Some(v));
        if t.trim().is_empty() {
            continue;
        }
        out.insert(
            k.into(),
            if k == "comment" {
                v.clone()
            } else {
                json!(t.trim())
            },
        );
    }
    Value::Object(out)
}

/// `firewallModel`: a stack without a firewall starts closed and off.
pub fn firewall_model(fw: &Value) -> Value {
    let or = |k: &str, d: &str| fw[k].as_str().unwrap_or(d).to_string();
    let rules: Vec<Value> = fw["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, rule)| json!({ "origin": i, "rule": rule }))
        .collect();
    json!({
        "enabled": fw["enabled"].as_bool().unwrap_or(false),
        "comment": or("comment", ""),
        "policy_in": or("policy_in", "DROP"),
        "policy_out": or("policy_out", "ACCEPT"),
        "management_open": or("management_open", ""),
        "rules": rules,
    })
}

/// `firewallBody`: the edit the server takes.
pub fn firewall_body(m: &Value) -> Value {
    let comment = m["comment"].as_str().unwrap_or("");
    let mgmt = m["management_open"].as_str().unwrap_or("").trim();
    let rules: Vec<Value> = m["rules"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| json!({ "origin": r["origin"], "rule": clean_rule(&r["rule"]) }))
        .collect();
    json!({
        "kind": "firewall",
        "enabled": m["enabled"].as_bool().unwrap_or(false),
        "comment": if comment.trim().is_empty() { Value::Null } else { json!(comment) },
        "policy_in": m["policy_in"],
        "policy_out": m["policy_out"],
        "management_open": if mgmt.is_empty() { Value::Null } else { json!(mgmt) },
        "rules": rules,
    })
}

/// `firewallChanged`.
pub fn firewall_changed(m: &Value, fw: &Value) -> bool {
    firewall_body(m) != firewall_body(&firewall_model(fw))
}

// ── feat-stacks-9: network, lxc flags, storage, on_demand, retention ────

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
fn without_origin(row: &Value) -> Value {
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
    if let (Ok(e), Ok(s)) = (every.parse::<i64>(), span.parse::<i64>()) {
        if s < e {
            out.insert(
                "span_days".into(),
                "Must be at least the keep-one-every span.".into(),
            );
        }
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

// ── feat-stacks-10: apps & storage ──────────────────────────────────────

/// `appsRemoveField`.
pub fn apps_remove_field(app: &str) -> Field {
    let mut f = es().apps_remove.filled(&[("app", app)]);
    f.current = Some(json!(false));
    f
}

/// `appsAddBlankField`.
pub fn apps_add_blank_field() -> Field {
    let mut f = es().apps_add_blank.field();
    f.current = Some(json!(""));
    f
}

/// feat-tiles-3: one app's tile-hostname field, on the add-app step.
pub fn add_app_tile_field(app: &str) -> Field {
    let mut f = es().add_app_tile.filled(&[("app", app)]);
    f.current = Some(json!(""));
    f
}

/// The add-app step's fields for whichever preset is picked: `preset`
/// itself, kept as it was (its choices, its current value), plus one tile
/// field per app that preset brings in — rebuilt by `after_set` on every
/// `pick`, and once up front in `open` for the preset already selected by
/// default (the first one, the same as the browser's own `renderTiles`).
pub fn add_app_step_fields(preset_field: Field, data: &Value, preset_name: &str) -> Vec<Field> {
    let apps: Vec<String> = data["presets"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["name"].as_str() == Some(preset_name))
        .map(|p| strings(&p["apps"]))
        .unwrap_or_default();
    let mut fields = vec![preset_field];
    fields.extend(apps.iter().map(|a| add_app_tile_field(a)));
    fields
}

/// Every element of `v` (a JSON array) with its own index added as
/// `origin` — flat, since the server's edit structs here (unlike
/// `FirewallEdit.rules`) are not `{origin, rule}` nested.
fn with_origin(v: &Value) -> Value {
    Value::Array(
        v.as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, row)| {
                let mut m = row.as_object().cloned().unwrap_or_default();
                m.insert("origin".into(), json!(i));
                Value::Object(m)
            })
            .collect(),
    )
}

/// `appsBody`: only the parts touched. `model` is the row tables as the
/// editor holds them now (`EditState.model`); `data` is the edit read
/// `open` started from, to tell an untouched list from an edited one.
pub fn apps_body(fields: &[Field], values: &Values, model: &Value, data: &Value) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("apps"));
    let mut remove = Vec::new();
    for f in fields {
        if let Some(app) = f.name.strip_prefix("apps-remove:") {
            if values.get(&f.name) == Some(&Value::Bool(true)) {
                remove.push(json!(app));
            }
        }
    }
    if !remove.is_empty() {
        out.insert("remove".into(), Value::Array(remove));
    }
    let add_blank: Vec<Value> = text_of(values.get("add_blank"))
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| json!(s))
        .collect();
    if !add_blank.is_empty() {
        out.insert("add_blank".into(), Value::Array(add_blank));
    }
    for list in ["storage", "data_mounts", "log_files"] {
        let now = &model[list];
        let orig = with_origin(&data[list]);
        if *now != orig {
            out.insert(list.into(), now.clone());
        }
    }
    Value::Object(out)
}

/// `storageFields`.
pub fn storage_fields(apps: &[String], row: Option<&Value>) -> Vec<Field> {
    es().storage_entry
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "no_data" => json!(r["no_data"].as_bool().unwrap_or(false)),
                    "host_owner_uid" => json!(num_text(&r["host_owner_uid"])),
                    k => r[k].clone(),
                });
            }
            if d.name == "app" {
                let mut c = vec![Choice {
                    value: String::new(),
                    label: "(the stack itself)".into(),
                }];
                c.extend(apps.iter().map(|a| Choice {
                    value: a.clone(),
                    label: a.clone(),
                }));
                f.choices = Some(c);
            }
            f
        })
        .collect()
}

/// `storageFromValues` (no `origin`: the caller adds it).
pub fn storage_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert(
        "host_path".into(),
        json!(text_of(v.get("host_path")).trim()),
    );
    m.insert(
        "mount_point".into(),
        json!(text_of(v.get("mount_point")).trim()),
    );
    m.insert(
        "no_data".into(),
        json!(v.get("no_data") == Some(&Value::Bool(true))),
    );
    let app = text_of(v.get("app")).trim().to_string();
    if !app.is_empty() {
        m.insert("app".into(), json!(app));
    }
    let no_backup = text_of(v.get("no_backup")).trim().to_string();
    if !no_backup.is_empty() {
        m.insert("no_backup".into(), json!(no_backup));
    }
    let uid = text_of(v.get("host_owner_uid")).trim().to_string();
    if !uid.is_empty() {
        m.insert("host_owner_uid".into(), js_number(&uid));
    }
    Value::Object(m)
}

/// `storageSummary`.
pub fn storage_summary(row: &Value) -> String {
    let app = row["app"].as_str().filter(|a| !a.is_empty());
    match app {
        Some(a) => format!(
            "{} → {} ({a})",
            row["host_path"].as_str().unwrap_or(""),
            row["mount_point"].as_str().unwrap_or("")
        ),
        None => format!(
            "{} → {}",
            row["host_path"].as_str().unwrap_or(""),
            row["mount_point"].as_str().unwrap_or("")
        ),
    }
}

/// `dataMountFields`.
pub fn data_mount_fields(row: Option<&Value>) -> Vec<Field> {
    es().data_mount
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "rotate_files" => r["rotate"]["files"].clone(),
                    "rotate_keep" => json!(num_text(&r["rotate"]["keep"])),
                    "rotate_container" => r["rotate"]["reopen"]["container"].clone(),
                    "rotate_signal" => r["rotate"]["reopen"]["signal"].clone(),
                    k => r[k].clone(),
                });
            }
            f
        })
        .collect()
}

/// `dataMountFromValues`.
pub fn data_mount_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert(
        "host_path".into(),
        json!(text_of(v.get("host_path")).trim()),
    );
    m.insert(
        "mount_point".into(),
        json!(text_of(v.get("mount_point")).trim()),
    );
    let note = text_of(v.get("note")).trim().to_string();
    if !note.is_empty() {
        m.insert("note".into(), json!(note));
    }
    let files = text_of(v.get("rotate_files")).trim().to_string();
    if !files.is_empty() {
        let mut r = Map::new();
        r.insert("files".into(), json!(files));
        let keep = text_of(v.get("rotate_keep")).trim().to_string();
        if !keep.is_empty() {
            r.insert("keep".into(), js_number(&keep));
        }
        let container = text_of(v.get("rotate_container")).trim().to_string();
        if !container.is_empty() {
            let mut reopen = Map::new();
            reopen.insert("container".into(), json!(container));
            let signal = text_of(v.get("rotate_signal")).trim().to_string();
            if !signal.is_empty() {
                reopen.insert("signal".into(), json!(signal));
            }
            r.insert("reopen".into(), Value::Object(reopen));
        }
        m.insert("rotate".into(), Value::Object(r));
    }
    Value::Object(m)
}

/// `dataMountProblems`.
pub fn data_mount_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if text_of(v.get("rotate_files")).trim().is_empty() {
        for k in ["rotate_keep", "rotate_container", "rotate_signal"] {
            if !text_of(v.get(k)).trim().is_empty() {
                out.insert(k.into(), "Needs a rotate file name/glob above.".into());
            }
        }
    }
    out
}

/// `dataMountSummary`.
pub fn data_mount_summary(row: &Value) -> String {
    format!(
        "{} → {}",
        row["host_path"].as_str().unwrap_or(""),
        row["mount_point"].as_str().unwrap_or("")
    )
}

/// `logFileFields`.
pub fn log_file_fields(row: Option<&Value>) -> Vec<Field> {
    es().log_file
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

/// `logFileFromValues`.
pub fn log_file_from_values(v: &Values) -> Value {
    json!({
        "path": text_of(v.get("path")).trim(),
        "job": text_of(v.get("job")).trim(),
    })
}

/// `logFileSummary`.
pub fn log_file_summary(row: &Value) -> String {
    format!(
        "{} ({})",
        row["path"].as_str().unwrap_or(""),
        row["job"].as_str().unwrap_or("")
    )
}

// ── feat-stacks-11: latch_secrets and latch_files ───────────────────────

/// `latchSecretField`.
pub fn latch_secret_field(app: &str) -> Field {
    let mut f = es().latch_secret.filled(&[("app", app)]);
    f.current = Some(json!(false));
    f
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

// ── feat-checks-1: check/probe/manual rows, the app/busy/url scalars ────

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
fn manual_as_objects(v: &Value) -> Value {
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

/// `checksBody`: only the parts touched. `model` is `{checks, manual,
/// probes}` as the editor holds them now; `data` is the edit read `open`
/// started from.
pub fn checks_drive_body(values: &Values, model: &Value, data: &Value, app: &str) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("checks"));
    out.insert("app".into(), json!(app));
    let orig = &data["checks"][app];
    let orig_checks = with_origin(orig.get("checks").unwrap_or(&Value::Null));
    let orig_manual = with_origin(&manual_as_objects(
        orig.get("manual").unwrap_or(&Value::Null),
    ));
    let orig_probes = with_origin(orig.get("probes").unwrap_or(&Value::Null));
    if model["checks"] != orig_checks {
        out.insert("checks".into(), model["checks"].clone());
    }
    if model["manual"] != orig_manual {
        out.insert("manual".into(), model["manual"].clone());
    }
    if model["probes"] != orig_probes {
        out.insert("probes".into(), model["probes"].clone());
    }
    let busy = text_of(values.get("busy_check")).trim().to_string();
    out.insert(
        "busy_check".into(),
        if busy.is_empty() {
            Value::Null
        } else {
            json!(busy)
        },
    );
    let url = text_of(values.get("url")).trim().to_string();
    out.insert(
        "url".into(),
        if url.is_empty() {
            Value::Null
        } else {
            json!(url)
        },
    );
    Value::Object(out)
}

/// Whether `checks_drive_body`'s result actually changes anything —
/// `changes_something` alone cannot tell, since `busy_check`/`url` are
/// always present (the plain-scalar convention `firewallBody` uses for
/// `management_open`: PUT the whole wanted value, not a patch).
pub fn checks_changed(body: &Value, data: &Value, app: &str) -> bool {
    if body.get("checks").is_some() || body.get("manual").is_some() || body.get("probes").is_some()
    {
        return true;
    }
    let orig = &data["checks"][app];
    let now_busy = orig.get("busy_check").and_then(|b| b["command"].as_str());
    let now_url = orig.get("url").and_then(Value::as_str);
    body["busy_check"].as_str() != now_busy || body["url"].as_str() != now_url
}

// ── feat-tiles-1: one tile's row dialog ──────────────────────────────────

/// `tileRowFields`.
pub fn tile_row_fields(groups: &[String], row: Option<&Value>) -> Vec<Field> {
    es().tile_row
        .iter()
        .map(|d| {
            let mut f = d.field();
            if let Some(r) = row {
                f.current = Some(match d.name.as_str() {
                    "order" | "watch_every" | "down_after" => json!(num_text(&r[&d.name])),
                    k => r[k].clone(),
                });
            }
            if d.name == "group" {
                f.choices = Some(
                    groups
                        .iter()
                        .map(|g| Choice {
                            value: g.clone(),
                            label: g.clone(),
                        })
                        .collect(),
                );
            }
            f
        })
        .collect()
}

/// `tileRowFromValues` (no `origin`/`key`: the caller sets those).
pub fn tile_row_from_values(v: &Values) -> Value {
    let mut m = Map::new();
    m.insert("key".into(), json!(text_of(v.get("key")).trim()));
    m.insert("name".into(), json!(text_of(v.get("name")).trim()));
    m.insert("group".into(), json!(text_of(v.get("group")).trim()));
    let order = text_of(v.get("order")).trim().to_string();
    if !order.is_empty() {
        m.insert("order".into(), js_number(&order));
    }
    for k in ["description", "url", "reading"] {
        let t = text_of(v.get(k)).trim().to_string();
        if !t.is_empty() {
            m.insert(k.into(), json!(t));
        }
    }
    let we = text_of(v.get("watch_every")).trim().to_string();
    if !we.is_empty() {
        m.insert("watch_every".into(), js_number(&we));
    }
    let da = text_of(v.get("down_after")).trim().to_string();
    if !da.is_empty() {
        m.insert("down_after".into(), js_number(&da));
    }
    Value::Object(m)
}

/// `tileRowProblems`.
pub fn tile_row_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let we = text_of(v.get("watch_every")).trim().to_string();
    let da = text_of(v.get("down_after")).trim().to_string();
    if !we.is_empty() && !da.is_empty() {
        if let (Ok(w), Ok(d)) = (we.parse::<i64>(), da.parse::<i64>()) {
            if d < w {
                out.insert(
                    "down_after".into(),
                    "Must be at least check every (seconds).".into(),
                );
            }
        }
    }
    out
}

/// `tileRowSummary`.
pub fn tile_row_summary(row: &Value) -> String {
    format!(
        "{} · {} ({})",
        row["key"].as_str().unwrap_or(""),
        row["name"].as_str().unwrap_or(""),
        row["group"].as_str().unwrap_or("")
    )
}

/// `tilesEditBody`/`tilesBody`: `model["tiles"]`'s rows (`origin` the old
/// hostname or null) into `TilesEdit.tiles` — a sparse change list
/// (`stackedit_tiles::TilesEdit` is not a full-list `Seq`, unlike the
/// lists above: a tile not mentioned is left alone, and deleting an
/// EXISTING one needs an explicit `delete: true` tombstone).
pub fn tiles_drive_body(model: &Value, data: &Value) -> Value {
    let originals = data["manifest"]["tiles"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let rows = model["tiles"].as_array().cloned().unwrap_or_default();
    let present: std::collections::BTreeSet<String> = rows
        .iter()
        .filter_map(|r| r["origin"].as_str().map(str::to_string))
        .collect();
    let mut out = Vec::new();
    for row in &rows {
        let key = row["key"].as_str().unwrap_or("").to_string();
        let mut tile = row.as_object().cloned().unwrap_or_default();
        tile.remove("origin");
        tile.remove("key");
        match row["origin"].as_str() {
            None => out.push(json!({ "origin": null, "key": key, "delete": false, "tile": tile })),
            Some(origin) => {
                let was = originals.get(origin);
                let changed = was.map(|w| w.as_object() != Some(&tile)).unwrap_or(true);
                if changed || key != origin {
                    out.push(
                        json!({ "origin": origin, "key": key, "delete": false, "tile": tile }),
                    );
                }
            }
        }
    }
    for key in originals.keys() {
        if !present.contains(key) {
            out.push(json!({ "origin": key, "key": key, "delete": true, "tile": {} }));
        }
    }
    json!({ "kind": "tiles", "tiles": out })
}

// ── feat-publish-1: the publish-app dialog ───────────────────────────────

/// `openPublishDialog`'s body.
pub fn publish_body(values: &Values, app: &str) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("publish_app"));
    out.insert("app".into(), json!(app));
    out.insert(
        "hostname".into(),
        json!(text_of(values.get("hostname")).trim()),
    );
    out.insert("port".into(), js_number(&text_of(values.get("port"))));
    out.insert(
        "external".into(),
        json!(values.get("external") == Some(&Value::Bool(true))),
    );
    out.insert(
        "separate_file".into(),
        json!(values.get("separate_file") == Some(&Value::Bool(true))),
    );
    if values.get("create_tile") == Some(&Value::Bool(true)) {
        let name = text_of(values.get("tile_name")).trim().to_string();
        let group = text_of(values.get("tile_group")).trim().to_string();
        out.insert(
            "tile".into(),
            json!({
                "name": if name.is_empty() { app.to_string() } else { name },
                "group": if group.is_empty() { "Own".to_string() } else { group },
            }),
        );
    }
    Value::Object(out)
}

/// `ruleSummary`: one rule in words.
pub fn rule_summary(r: &Value) -> String {
    let dir = r["dir"].as_str().unwrap_or("in");
    let (peer, word) = if dir == "in" {
        (&r["source"], "from")
    } else {
        (&r["dest"], "to")
    };
    let mut parts = vec![
        dir.to_uppercase(),
        r["action"].as_str().unwrap_or("").to_string(),
    ];
    match peer.as_str().filter(|p| !p.is_empty()) {
        Some(p) => parts.push(format!("{word} {p}")),
        None => parts.push(format!("{word} anywhere")),
    }
    if let Some(p) = r["proto"].as_str().filter(|p| !p.is_empty()) {
        parts.push(p.to_string());
    }
    if let Some(p) = r["dport"].as_str().filter(|p| !p.is_empty()) {
        parts.push(format!("port {p}"));
    }
    parts.join(" ")
}

/// `ruleFields`: the rule dialog, for a new rule or the one edited.
pub fn rule_fields(r: Option<&Value>) -> Vec<Field> {
    let from = |name: &str| -> Option<Value> {
        let r = r?;
        let v = match name {
            "peer" => {
                if r["dir"] == "in" {
                    r["source"].clone()
                } else {
                    r["dest"].clone()
                }
            }
            n => r[n].clone(),
        };
        (!v.is_null()).then_some(v)
    };
    es().rule
        .iter()
        .map(|d| {
            let mut f = d.field();
            let peer_default = (d.name == "peer" && r.is_some()).then(|| json!(""));
            f.current = from(&d.name).or(peer_default).or(f.current);
            f
        })
        .collect()
}

/// `settingsForm`'s fields for one stack's manifest (the edit read's
/// `manifest`) and its images.
pub fn settings_fields(m: &Value, images: &Value) -> Vec<Field> {
    let num = |v: &Value| match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let r = &m["resources"];
    let mut out: Vec<Field> = es()
        .settings
        .iter()
        .map(|d| {
            let mut f = d.field();
            f.current = Some(match d.name.as_str() {
                "onboot" => json!(m["boot"]["onboot"].as_bool().unwrap_or(false)),
                "protection" => json!(m["protection"].as_bool().unwrap_or(false)),
                "order" => json!(num(&m["boot"]["order"])),
                k => json!(num(&r[k])),
            });
            f
        })
        .collect();
    if let Some(map) = images.as_object() {
        for (key, image) in map {
            out.push(image_field(key, image.as_str().unwrap_or("")));
        }
    }
    if let Some(map) = m["tiles"].as_object() {
        let mut keys: Vec<&String> = map.keys().collect();
        keys.sort();
        for key in keys {
            out.extend(tile_fields(key, &map[key]));
        }
    }
    out
}

/// `imageField`.
pub fn image_field(key: &str, image: &str) -> Field {
    let slug = regex::Regex::new("[^a-zA-Z0-9]+")
        .expect("reads")
        .replace_all(key, "-")
        .to_string();
    let mut f = es().image.filled(&[("key", key), ("slug", &slug)]);
    f.current = Some(json!(image));
    f
}

/// `tileFields`: one tile's two watch fields (owner remark 2026-09-30).
pub fn tile_fields(key: &str, tile: &Value) -> Vec<Field> {
    let slug = regex::Regex::new("[^a-zA-Z0-9]+")
        .expect("reads")
        .replace_all(key, "-")
        .to_string();
    let words = [("key", key), ("slug", &slug)];
    let mut watch = es().tile_watch.filled(&words);
    watch.current = Some(json!(num_text(&tile["watch_every"])));
    let mut down = es().tile_down.filled(&words);
    down.current = Some(json!(num_text(&tile["down_after"])));
    vec![watch, down]
}

fn num_text(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// `settingsBody`: only what differs from now.
pub fn settings_body(fields: &[Field], values: &Values) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("settings"));
    let mut images = Map::new();
    let mut tiles = Map::new();
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
        if t == now {
            continue;
        }
        if let Some(key) = f.name.strip_prefix("image:") {
            images.insert(key.to_string(), json!(t));
        } else if let Some(key) = f.name.strip_prefix("tile_watch_every:") {
            if !t.is_empty() {
                tile_entry(&mut tiles, key).insert("watch_every".into(), js_number(t));
            }
        } else if let Some(key) = f.name.strip_prefix("tile_down_after:") {
            if !t.is_empty() {
                tile_entry(&mut tiles, key).insert("down_after".into(), js_number(t));
            }
        } else if !t.is_empty() {
            out.insert(f.name.clone(), js_number(t));
        }
    }
    if !images.is_empty() {
        out.insert("images".into(), Value::Object(images));
    }
    if !tiles.is_empty() {
        out.insert("tiles".into(), Value::Object(tiles));
    }
    Value::Object(out)
}

/// The tile's map within `tiles`, made fresh the first time it is touched.
fn tile_entry<'a>(tiles: &'a mut Map<String, Value>, key: &str) -> &'a mut Map<String, Value> {
    tiles
        .entry(key.to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("just inserted as an object")
}

/// A field's value from the submitted `values`, falling back to what it was
/// opened with — the native and preset forms send every field (the server
/// diffs against the file itself), unlike `settings_body`'s "only what
/// changed".
fn field_value(fields: &[Field], values: &Values, name: &str) -> Value {
    values
        .get(name)
        .cloned()
        .or_else(|| {
            fields
                .iter()
                .find(|f| f.name == name)
                .and_then(|f| f.current.clone())
        })
        .unwrap_or(Value::Null)
}

/// `nativeField`'s `metrics`/`update_policy` pickers read back, and the
/// plain text/textarea/check fields — `native-<field>`'s value in the
/// server's own words (`admin/web/js/editpanels.js`'s `edit()`).
pub fn native_body(fields: &[Field], values: &Values) -> Value {
    let s = |n: &str| text_of(Some(&field_value(fields, values, n)));
    let b = |n: &str| field_value(fields, values, n) == Value::Bool(true);
    let data_dirs: Vec<Value> = s("data_dirs")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| json!(l))
        .collect();
    json!({
        "kind": "native",
        "unit": s("unit"),
        "binary": s("binary"),
        "env_file": s("env_file"),
        "data_dirs": data_dirs,
        "update_cmd": s("update_cmd"),
        "stateless": b("stateless"),
        "restore_note": s("restore_note"),
        "release_repo": s("release_repo"),
        "release_asset": s("release_asset"),
        "backup_from_newest": s("backup_from_newest"),
        "backup_pause": s("backup_pause"),
        "update_policy": s("update_policy"),
        "metrics": s("metrics"),
    })
}

/// "Add a native unit"'s fields, in the server's own words.
pub fn add_native_body(fields: &[Field], values: &Values) -> Value {
    let s = |n: &str| text_of(Some(&field_value(fields, values, n)));
    let b = |n: &str| field_value(fields, values, n) == Value::Bool(true);
    let data_dirs: Vec<Value> = s("data_dirs")
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| json!(l))
        .collect();
    json!({
        "kind": "add_native",
        "unit": s("unit").trim(),
        "binary": s("binary").trim(),
        "env_file": s("env_file"),
        "data_dirs": data_dirs,
        "notify": b("notify"),
    })
}

/// The manifest field of a native unit's own JSON (`NativeView`'s shape,
/// `admin/src/shell/edit.rs`'s `read_stack_edit`), as the form field of
/// that name would hold it — `native-unit`'s `after_set` and `open`'s
/// prefill share this.
pub fn native_current(name: &str, m: &Value) -> Value {
    match name {
        "data_dirs" => json!(strings(&m["data_dirs"]).join("\n")),
        "stateless" => json!(m[name].as_bool().unwrap_or(false)),
        // fix-113 (owner decision, 2026-10-01): `backup_pause` reads from
        // the manifest as a bool (false/true) or the string "chassis" —
        // `NativeServiceManifest`'s own `BackupPause::Serialize` shape — and
        // the select wants it back as one of those three strings.
        "backup_pause" => json!(match m[name].as_str() {
            Some("chassis") => "chassis",
            _ if m[name].as_bool() == Some(true) => "true",
            _ => "false",
        }),
        "update_policy" => json!(m[name].as_str().unwrap_or("manual")),
        "metrics" => json!(if m["metrics"] == json!(false) {
            "not_measured"
        } else {
            "measured"
        }),
        "binary" => json!(m["binary"].as_str().unwrap_or("")),
        _ => json!(m[name].as_str().unwrap_or("")),
    }
}

/// A `Number(text)` or `null` when blank — `preset.yml`'s optional `cores`
/// and `disk_gb`.
fn opt_number(t: &str) -> Value {
    if t.trim().is_empty() {
        Value::Null
    } else {
        js_number(t)
    }
}

/// `preset.yml`'s own fields, in the server's own words (feat-preset-1).
/// `unprivileged` has no field in the form (the browser's meta card does
/// not expose it either): it is carried through unchanged from what the
/// preset already held, `Value::Null` for a new one.
pub fn preset_meta_body(fields: &[Field], values: &Values, unprivileged: Value) -> Value {
    let s = |n: &str| text_of(Some(&field_value(fields, values, n)));
    let b = |n: &str| field_value(fields, values, n) == Value::Bool(true);
    let opt_str = |n: &str| {
        let t = s(n);
        if t.trim().is_empty() {
            Value::Null
        } else {
            json!(t.trim())
        }
    };
    json!({
        "description": s("description").trim(),
        "ram_mb": js_number(&s("ram_mb")),
        "cores": opt_number(&s("cores")),
        "disk_gb": opt_number(&s("disk_gb")),
        "features": opt_str("features"),
        "unprivileged": unprivileged,
        "gpu": b("gpu"),
        "vpn": b("vpn"),
    })
}

/// A preset's read meta (`PresetMetaView`, `admin/web/js/presetseditor.js`)
/// as the field of that name would hold it; `Value::Null` (a new preset)
/// gives every field its blank/default starting value.
fn preset_meta_current(name: &str, meta: &Value) -> Value {
    match name {
        "ram_mb" => json!(meta["ram_mb"].as_u64().unwrap_or(1024).to_string()),
        "cores" | "disk_gb" => match meta[name].as_u64() {
            Some(n) => json!(n.to_string()),
            None => json!(""),
        },
        "gpu" | "vpn" => json!(meta[name].as_bool().unwrap_or(false)),
        "description" | "features" => json!(meta[name].as_str().unwrap_or("")),
        _ => Value::Null,
    }
}

/// feat-preset-1: the Files card's own fields, appended to the "meta"
/// step — `file`'s choices are the preset's files (minus `preset.yml`),
/// `template`'s the starter templates; the rest start blank.
fn preset_file_fields(data: &Value) -> Vec<Field> {
    let mut names: Vec<String> = data["files"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.clone())
        .filter(|k| k != presetedit::PRESET_META)
        .collect();
    names.sort();
    let templates: Vec<String> = data["file_templates"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, _)| k.clone())
        .collect();
    es().preset_file
        .iter()
        .map(|d| {
            let mut f = d.field();
            match d.name.as_str() {
                "file" => {
                    let mut choices = vec![Choice {
                        value: String::new(),
                        label: "(new file)".into(),
                    }];
                    choices.extend(names.iter().map(|n| Choice {
                        value: n.clone(),
                        label: n.clone(),
                    }));
                    f.choices = Some(choices);
                    f.current = Some(json!(""));
                }
                "template" => {
                    let mut choices = vec![Choice {
                        value: String::new(),
                        label: "Blank".into(),
                    }];
                    choices.extend(templates.iter().map(|n| Choice {
                        value: n.clone(),
                        label: n.clone(),
                    }));
                    f.choices = Some(choices);
                    f.current = Some(json!(""));
                }
                _ => f.current = Some(json!("")),
            }
            f
        })
        .collect()
}

/// `tileProblems`: down after must be at least check every, when both are
/// typed.
pub fn tile_problems(values: &Values) -> BTreeMap<String, String> {
    let mut keys: BTreeSet<String> = BTreeSet::new();
    for name in values.keys() {
        if let Some(k) = name.strip_prefix("tile_watch_every:") {
            keys.insert(k.to_string());
        } else if let Some(k) = name.strip_prefix("tile_down_after:") {
            keys.insert(k.to_string());
        }
    }
    let mut out = BTreeMap::new();
    for key in keys {
        let w = text_of(values.get(&format!("tile_watch_every:{key}")));
        let d = text_of(values.get(&format!("tile_down_after:{key}")));
        let (w, d) = (w.trim(), d.trim());
        if w.is_empty() || d.is_empty() {
            continue;
        }
        if let (Ok(wn), Ok(dn)) = (w.parse::<i64>(), d.parse::<i64>()) {
            if dn < wn {
                out.insert(
                    format!("tile_down_after:{key}"),
                    say(
                        "tile_down_low",
                        &[
                            ("key", &key),
                            ("watch", &wn.to_string()),
                            ("down", &dn.to_string()),
                        ],
                    ),
                );
            }
        }
    }
    out
}

/// `Number(text)` as JSON.
fn js_number(t: &str) -> Value {
    let t = t.trim();
    if t.is_empty() {
        return json!(0);
    }
    if let Ok(n) = t.parse::<u64>() {
        return json!(n);
    }
    t.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|n| json!(n))
        .unwrap_or(Value::Null)
}

/// `changesSomething`.
pub fn changes_something(body: &Value) -> bool {
    body.as_object()
        .is_some_and(|m| m.keys().any(|k| k != "kind"))
}

/// `commitFields`: the plan dialog's commit step.
pub fn commit_fields(follow_ups: &[String], subject: &str) -> Vec<Field> {
    let e = es();
    let mut offered = vec!["none".to_string()];
    offered.extend(
        follow_ups
            .iter()
            .filter(|f| e.follow.contains_key(*f))
            .cloned(),
    );
    let mut out: Vec<Field> = e.commit.iter().map(EditFieldDef::field).collect();
    for f in &mut out {
        match f.name.as_str() {
            "subject" => f.current = Some(json!(subject)),
            "follow" => {
                f.choices = Some(
                    offered
                        .iter()
                        .map(|o| Choice {
                            value: o.clone(),
                            label: e.follow[o].clone(),
                        })
                        .collect(),
                );
                let deploy = follow_ups.iter().any(|f| f == "deploy");
                f.current = Some(json!(if deploy { "deploy" } else { "none" }));
            }
            _ => {}
        }
    }
    out
}

/// `commitBody`.
pub fn commit_body(edit: &Value, v: &Values) -> Value {
    let subject = text_of(v.get("subject")).trim().to_string();
    let note = text_of(v.get("note")).trim().to_string();
    let follow = match v.get("follow") {
        Some(Value::String(s)) => s.clone(),
        _ => "none".into(),
    };
    let mut out = Map::new();
    out.insert("edit".into(), edit.clone());
    if !subject.is_empty() {
        out.insert("subject".into(), json!(subject));
    }
    if !note.is_empty() {
        out.insert("note".into(), json!(note));
    }
    if !follow.is_empty() && follow != "none" {
        out.insert("follow".into(), json!(follow));
    }
    Value::Object(out)
}

/// `newStackWizard`'s steps, for the presets the repository holds.
pub fn new_stack_steps(presets: &[Value], suggest: Option<u64>) -> Vec<FormStep> {
    let e = es();
    let first = presets.first();
    let pick =
        |k: &str, d: u64| -> String { first.and_then(|p| p[k].as_u64()).unwrap_or(d).to_string() };
    let now: BTreeMap<&str, String> = [
        (
            "preset",
            first
                .and_then(|p| p["name"].as_str())
                .unwrap_or("")
                .to_string(),
        ),
        ("vmid", suggest.map(|v| v.to_string()).unwrap_or_default()),
        ("ram_mb", pick("ram_mb", e.new_defaults.ram_mb)),
        ("cores", pick("cores", e.new_defaults.cores)),
        ("disk_gb", pick("disk_gb", e.new_defaults.disk_gb)),
        ("swap_mb", String::new()),
    ]
    .into_iter()
    .collect();
    e.new_stack
        .iter()
        .map(|s| FormStep {
            id: s.id.clone(),
            label: s.label.clone(),
            fields: s
                .fields
                .iter()
                .map(|d| {
                    let mut f = d.field();
                    if d.name == "preset" {
                        f.choices = Some(
                            presets
                                .iter()
                                .map(|p| {
                                    let n = p["name"].as_str().unwrap_or("").to_string();
                                    Choice {
                                        label: format!(
                                            "{n} · {}",
                                            p["description"].as_str().unwrap_or("")
                                        ),
                                        value: n,
                                    }
                                })
                                .collect(),
                        );
                    }
                    if let Some(v) = now.get(d.name.as_str()) {
                        f.current = Some(json!(v));
                    }
                    f
                })
                .collect(),
        })
        .collect()
}

/// `dataFields`: one check per `/appdata` folder.
pub fn data_fields(paths: &[String]) -> Vec<Field> {
    paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let i = i.to_string();
            let mut f = es().nodata.filled(&[("path", p), ("i", &i)]);
            f.current = Some(json!(false));
            f
        })
        .collect()
}

/// `newStackBody`.
pub fn new_stack_body(v: &Values) -> Value {
    let t = |k: &str| text_of(v.get(k)).trim().to_string();
    let n = |k: &str| js_number(&t(k));
    let mut out = Map::new();
    out.insert("name".into(), json!(t("name")));
    out.insert("vmid".into(), n("vmid"));
    out.insert("preset".into(), json!(text_of(v.get("preset"))));
    out.insert("ram_mb".into(), n("ram_mb"));
    out.insert("cores".into(), n("cores"));
    out.insert("disk_gb".into(), n("disk_gb"));
    if !t("swap_mb").is_empty() {
        out.insert("swap_mb".into(), n("swap_mb"));
    }
    let no_data: Vec<Value> = v
        .iter()
        .filter(|(k, x)| k.starts_with("nodata:") && **x == Value::Bool(true))
        .map(|(k, _)| json!(&k["nodata:".len()..]))
        .collect();
    out.insert("no_data".into(), Value::Array(no_data));
    // feat-tiles-3: the wizard's Tile step, folded into this SAME body —
    // `newStackTile` on the browser's side — so `prepare_new` applies it
    // to the staged manifest in the one commit, not a second one after.
    let hostname = t("tile_hostname");
    if !hostname.is_empty() {
        let mut tile = Map::new();
        tile.insert("hostname".into(), json!(hostname));
        let name = t("tile_name");
        tile.insert(
            "name".into(),
            json!(if name.is_empty() { t("name") } else { name }),
        );
        let group = t("tile_group");
        tile.insert(
            "group".into(),
            json!(if group.is_empty() {
                "Own".into()
            } else {
                group
            }),
        );
        let description = t("tile_description");
        if !description.is_empty() {
            tile.insert("description".into(), json!(description));
        }
        if !t("tile_watch_every").is_empty() {
            tile.insert("watch_every".into(), n("tile_watch_every"));
        }
        if !t("tile_down_after").is_empty() {
            tile.insert("down_after".into(), n("tile_down_after"));
        }
        out.insert("tile".into(), Value::Object(tile));
    }
    Value::Object(out)
}

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
        "text_list" => Ok(json!(t
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>())),
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
fn key_text(f: &Value) -> String {
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

/// `hostSettingsBody`.
pub fn host_settings_body(
    sha256: &str,
    fields: &[Value],
    staged: &BTreeMap<String, Value>,
    confirmed: &BTreeSet<String>,
) -> Value {
    let mut values = Map::new();
    let mut fragments = Map::new();
    for (key, v) in staged {
        let table = fields
            .iter()
            .any(|f| f["key"] == key.as_str() && f["kind"]["type"] == "table");
        if table {
            fragments.insert(key.clone(), json!(text_of(Some(v))));
        } else {
            values.insert(key.clone(), v.clone());
        }
    }
    let confirms: Vec<&String> = confirmed
        .iter()
        .filter(|k| staged.contains_key(*k))
        .collect();
    json!({ "expect_sha256": sha256, "values": values, "fragments": fragments, "confirms": confirms })
}

/// The batch form (`batchForm`): the action's own fields but an app and
/// the typed name, plus one typed name per stack when the action asks it.
pub fn batch_fields(kind: ActionKind, stacks: &[String]) -> Vec<Field> {
    let mut out: Vec<Field> = kind
        .args()
        .iter()
        .filter(|a| !matches!(a, Arg::App | Arg::Confirm))
        .map(|a| drive::arg_field(*a, kind, "each stack"))
        .collect();
    if kind.confirm() {
        for s in stacks {
            let mut f = drive::arg_field(Arg::Confirm, kind, s);
            f.id = format!("act-confirm-{s}");
            f.name = format!("confirm:{s}");
            out.push(f);
        }
    }
    out
}

// ── the machine ─────────────────────────────────────────────────────────

fn plain() -> Applied {
    Applied {
        effect: Effect::None,
        held: None,
    }
}

fn held_for(form: &OpenForm, what: &str, errors: &BTreeMap<String, String>) -> Refusal {
    let all: Vec<Field> = form
        .desc
        .fields()
        .cloned()
        .chain(
            form.edit
                .as_ref()
                .and_then(|e| e.sub.as_ref())
                .map(|s| s.fields.clone())
                .unwrap_or_default(),
        )
        .collect();
    let list: Vec<String> = errors
        .iter()
        .map(|(name, e)| {
            let id = all
                .iter()
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
}

fn held_msg(what: &str, why: &str) -> Applied {
    Applied {
        effect: Effect::None,
        held: Some(Refusal::new(
            format!("ui press {what}"),
            format!("the form holds: {why}"),
            "change something first, or homelab ui close",
        )),
    }
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_string))
        .collect()
}

fn edit(form: &mut OpenForm) -> &mut EditState {
    form.edit.as_mut().expect("an edit form has its edit state")
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
    let stack_named = |t: Option<&str>| -> Result<String, Refusal> {
        let Some(t) = t else {
            return Err(refused(
                step,
                format!("{} acts on one stack and none was named", kind.slug()),
                format!("homelab ui open {}", kind.usage()),
            ));
        };
        if !cx.stacks.iter().any(|s| s == t) {
            return Err(refused(
                step,
                format!("the fleet has no stack {t}"),
                format!("the stacks are: {}", cx.stacks.join(", ")),
            ));
        }
        Ok(t.to_string())
    };
    let no_target = |t: Option<&str>| -> Result<(), Refusal> {
        match t {
            Some(t) => Err(refused(
                step,
                format!("{} takes no stack, not {t}", kind.slug()),
                format!("homelab ui open {}", kind.usage()),
            )),
            None => Ok(()),
        }
    };
    let data = cx.sources.edit.clone();
    let unreadable = |what: &str| -> Result<(), Refusal> {
        if let Some(why) = data["error"].as_object() {
            return Err(Refusal::new(
                format!("ui open {}", kind.slug()),
                format!(
                    "{what} could not be read: {}",
                    why.get("why").and_then(Value::as_str).unwrap_or("unknown")
                ),
                why.get("fix")
                    .and_then(Value::as_str)
                    .unwrap_or("homelab ui state, then try again")
                    .to_string(),
            ));
        }
        Ok(())
    };
    let mut es_ = EditState::new(kind, data.clone());
    let (id, stack, title, steps, page) = match kind {
        EditKind::Settings
        | EditKind::Raw
        | EditKind::AddApp
        | EditKind::Firewall
        | EditKind::SettingsExt
        | EditKind::Apps
        | EditKind::Latch
        | EditKind::Tiles
        | EditKind::AddNative => {
            let stack = stack_named(target)?;
            unreadable(&format!("the files of {stack}"))?;
            let m = &data["manifest"];
            let need_manifest = kind != EditKind::Raw;
            if need_manifest && m.is_null() {
                return Err(refused(
                    step,
                    format!(
                        "the {} form needs a readable lxc-compose.yml: {}",
                        kind.slug(),
                        data["manifest_error"].as_str().unwrap_or("none")
                    ),
                    format!("homelab ui open raw {stack}"),
                ));
            }
            let tab = if kind == EditKind::Firewall {
                "firewall"
            } else {
                "settings"
            };
            let page = format!("/app/stacks/{stack}/{tab}");
            let plan_steps = |first: FormStep| {
                vec![
                    first,
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ]
            };
            match kind {
                EditKind::Settings => (
                    format!("edit:settings:{stack}"),
                    stack.clone(),
                    format!("Settings · {stack}"),
                    plan_steps(FormStep {
                        id: "settings".into(),
                        label: "Settings".into(),
                        fields: settings_fields(m, &data["images"]),
                    }),
                    page,
                ),
                EditKind::Raw => {
                    let texts = data["texts"].as_object().cloned().unwrap_or_default();
                    let files: Vec<String> = texts.keys().cloned().collect();
                    let first = if files.iter().any(|f| f == "lxc-compose.yml") {
                        "lxc-compose.yml".to_string()
                    } else {
                        files.first().cloned().unwrap_or_default()
                    };
                    let mut fields: Vec<Field> = es().raw.iter().map(EditFieldDef::field).collect();
                    for f in &mut fields {
                        match f.name.as_str() {
                            "file" => {
                                f.choices = Some(
                                    files
                                        .iter()
                                        .map(|x| Choice {
                                            value: x.clone(),
                                            label: x.clone(),
                                        })
                                        .collect(),
                                );
                                f.current = Some(json!(first));
                            }
                            "text" => {
                                f.current = Some(texts.get(&first).cloned().unwrap_or(json!("")));
                            }
                            // "op" starts at "edit" (its choices are the
                            // fixed list formspec.json gives it); the two
                            // path fields (create/rename) start empty.
                            "op" => f.current = Some(json!("edit")),
                            _ => {}
                        }
                    }
                    (
                        format!("edit:raw:{stack}"),
                        stack.clone(),
                        format!("Edit a file · {stack}"),
                        plan_steps(FormStep {
                            id: "file".into(),
                            label: "File".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::AddApp => {
                    let presets = data["presets"].as_array().cloned().unwrap_or_default();
                    if presets.is_empty() {
                        return Err(refused(
                            step,
                            "the repository holds no preset with an app",
                            "add one under presets/ first",
                        ));
                    }
                    let mut preset_field = es().add_app[0].field();
                    preset_field.choices = Some(
                        presets
                            .iter()
                            .map(|p| {
                                let n = p["name"].as_str().unwrap_or("").to_string();
                                Choice {
                                    label: format!(
                                        "{n} · {} ({})",
                                        p["description"].as_str().unwrap_or(""),
                                        strings(&p["apps"]).join(", ")
                                    ),
                                    value: n,
                                }
                            })
                            .collect(),
                    );
                    let first_preset = presets[0]["name"].as_str().unwrap_or("").to_string();
                    preset_field.current = Some(json!(first_preset));
                    // feat-tiles-3: the tile fields of the preset already
                    // selected by default, the same as the browser's own
                    // `renderTiles` on first render.
                    let fields = add_app_step_fields(preset_field, &data, &first_preset);
                    (
                        format!("edit:add-app:{stack}"),
                        stack.clone(),
                        format!("Add an app · {stack}"),
                        plan_steps(FormStep {
                            id: "app".into(),
                            label: "Add an app".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::Firewall => {
                    let model = firewall_model(&m["firewall"]);
                    let mut fields: Vec<Field> =
                        es().firewall.iter().map(EditFieldDef::field).collect();
                    for f in &mut fields {
                        f.current = Some(model[&f.name].clone());
                    }
                    es_.model = Some(model);
                    (
                        format!("edit:firewall:{stack}"),
                        stack.clone(),
                        format!("Firewall · {stack}"),
                        plan_steps(FormStep {
                            id: "rules".into(),
                            label: "Rules".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::SettingsExt => {
                    // W2: retention is a row table like storage/data_mounts
                    // (full end-state list, origin tracks the old row), not
                    // a plain field — `es_.model` carries it the same way
                    // Apps' three lists ride along beside their fields.
                    es_.model = Some(json!({ "retention": with_origin(&m["retention"]) }));
                    (
                        format!("edit:settings-ext:{stack}"),
                        stack.clone(),
                        format!("Network & hardware · {stack}"),
                        plan_steps(FormStep {
                            id: "settings_ext".into(),
                            label: "Network & hardware".into(),
                            fields: settings_ext_fields(m),
                        }),
                        page,
                    )
                }
                EditKind::Apps => {
                    let apps = strings(&m["apps"]);
                    let mut fields: Vec<Field> =
                        apps.iter().map(|a| apps_remove_field(a)).collect();
                    fields.push(apps_add_blank_field());
                    // storage/data_mounts/log_files live on the manifest
                    // (`m`), not on the edit-read envelope (`data`) itself
                    // — `data["storage"]` was always null, so the row
                    // table opened empty no matter what the stack held.
                    es_.model = Some(json!({
                        "storage": with_origin(&m["storage"]),
                        "data_mounts": with_origin(&m["data_mounts"]),
                        "log_files": with_origin(&m["log_files"]),
                    }));
                    (
                        format!("edit:apps:{stack}"),
                        stack.clone(),
                        format!("Apps & storage · {stack}"),
                        plan_steps(FormStep {
                            id: "apps".into(),
                            label: "Apps & storage".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::Latch => {
                    let apps = strings(&m["apps"]);
                    // `latch` lives on the manifest (`m`) too, the same
                    // fix as storage/data_mounts/log_files above.
                    let secrets = strings(&m["latch"]["latch_secrets"]);
                    let fields: Vec<Field> = apps
                        .iter()
                        .map(|a| {
                            let mut f = latch_secret_field(a);
                            f.current = Some(json!(secrets.contains(a)));
                            f
                        })
                        .collect();
                    es_.model = Some(json!({
                        "latch_files": with_origin(&m["latch"]["latch_files"]),
                    }));
                    (
                        format!("edit:latch:{stack}"),
                        stack.clone(),
                        format!("Latch · {stack}"),
                        plan_steps(FormStep {
                            id: "latch".into(),
                            label: "Latch".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::AddNative => {
                    let fields: Vec<Field> =
                        es().add_native.iter().map(EditFieldDef::field).collect();
                    (
                        format!("edit:add-native:{stack}"),
                        stack.clone(),
                        format!("Add a native unit · {stack}"),
                        plan_steps(FormStep {
                            id: "add_native".into(),
                            label: "Add a native unit".into(),
                            fields,
                        }),
                        page,
                    )
                }
                EditKind::Tiles => {
                    let tiles = m["tiles"].as_object().cloned().unwrap_or_default();
                    let mut keys: Vec<&String> = tiles.keys().collect();
                    keys.sort();
                    let list: Vec<Value> = keys
                        .into_iter()
                        .map(|key| {
                            let mut row = tiles[key].as_object().cloned().unwrap_or_default();
                            row.insert("key".into(), json!(key));
                            row.insert("origin".into(), json!(key));
                            Value::Object(row)
                        })
                        .collect();
                    es_.model = Some(json!({ "tiles": Value::Array(list) }));
                    (
                        format!("edit:tiles:{stack}"),
                        stack.clone(),
                        format!("Tiles · {stack}"),
                        plan_steps(FormStep {
                            id: "tiles".into(),
                            label: "Tiles".into(),
                            fields: Vec::new(),
                        }),
                        page,
                    )
                }
                _ => unreachable!("the outer match restricts kind to this bucket's 9 variants"),
            }
        }
        EditKind::Native => {
            // `<stack>[/<unit>]` (`stack_named` reads a plain stack name,
            // so the unit is split off first — `rollback-native`'s own
            // `<stack>[/<unit>]` convention, not a second CLI argument).
            let (stack_part, unit_part) = match target {
                Some(t) => match t.split_once('/') {
                    Some((s, u)) => (s, Some(u)),
                    None => (t, None),
                },
                None => {
                    return Err(refused(
                        step,
                        format!("{} acts on one stack and none was named", kind.slug()),
                        format!("homelab ui open {}", kind.usage()),
                    ))
                }
            };
            let stack = stack_named(Some(stack_part))?;
            unreadable(&format!("the files of {stack}"))?;
            let natives = data["natives"].as_array().cloned().unwrap_or_default();
            if natives.is_empty() {
                return Err(refused(
                    step,
                    format!("{stack} has no native units"),
                    format!("homelab ui open add-native {stack}"),
                ));
            }
            let chosen = match unit_part {
                Some(u) => {
                    if !natives.iter().any(|n| n["unit"].as_str() == Some(u)) {
                        return Err(refused(
                            step,
                            format!("{stack} has no native unit called {u}"),
                            format!(
                                "its native units are: {}",
                                natives
                                    .iter()
                                    .filter_map(|n| n["unit"].as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    }
                    u.to_string()
                }
                None => natives[0]["unit"].as_str().unwrap_or_default().to_string(),
            };
            let m = natives
                .iter()
                .find(|n| n["unit"].as_str() == Some(chosen.as_str()))
                .map(|n| n["manifest"].clone())
                .unwrap_or(Value::Null);
            let mut fields: Vec<Field> = es().native.iter().map(EditFieldDef::field).collect();
            for f in &mut fields {
                if f.name == "unit" {
                    f.choices = Some(
                        natives
                            .iter()
                            .filter_map(|n| n["unit"].as_str())
                            .map(|u| Choice {
                                value: u.to_string(),
                                label: u.to_string(),
                            })
                            .collect(),
                    );
                    f.current = Some(json!(chosen));
                } else {
                    f.current = Some(native_current(&f.name, &m));
                }
            }
            (
                format!("edit:native:{stack}"),
                stack.clone(),
                format!("Native services · {stack}"),
                vec![
                    FormStep {
                        id: "native".into(),
                        label: "Native services".into(),
                        fields,
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                format!("/app/stacks/{stack}/settings"),
            )
        }
        EditKind::Checks | EditKind::PublishApp => {
            let Some(raw) = target else {
                return Err(refused(
                    step,
                    format!("{} acts on one app and none was named", kind.slug()),
                    format!("homelab ui open {}", kind.usage()),
                ));
            };
            let Some((stack_name, app)) = raw.split_once('/') else {
                return Err(refused(
                    step,
                    format!("{} takes <stack>/<app>, not {raw:?}", kind.slug()),
                    format!("homelab ui open {}", kind.usage()),
                ));
            };
            let stack = stack_named(Some(stack_name))?;
            unreadable(&format!("the files of {stack}"))?;
            let m = &data["manifest"];
            if m.is_null() {
                return Err(refused(
                    step,
                    format!(
                        "the {} form needs a readable lxc-compose.yml: {}",
                        kind.slug(),
                        data["manifest_error"].as_str().unwrap_or("none")
                    ),
                    format!("homelab ui open raw {stack}"),
                ));
            }
            let apps = strings(&m["apps"]);
            if !apps.iter().any(|a| a == app) {
                return Err(refused(
                    step,
                    format!("{app} is not an app of {stack}"),
                    format!("the apps are: {}", apps.join(", ")),
                ));
            }
            let plan_steps = |first: FormStep| {
                vec![
                    first,
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ]
            };
            match kind {
                EditKind::Checks => {
                    // `checks` lives on the manifest (`m`), not the
                    // envelope (`data`) — same fix as Apps/Latch above.
                    let orig = &m["checks"][app];
                    let checks_rows = with_origin(orig.get("checks").unwrap_or(&Value::Null));
                    let manual_rows = with_origin(&manual_as_objects(
                        orig.get("manual").unwrap_or(&Value::Null),
                    ));
                    let probes_rows = with_origin(orig.get("probes").unwrap_or(&Value::Null));
                    let busy = orig
                        .get("busy_check")
                        .and_then(|b| b["command"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let url = orig
                        .get("url")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    es_.model = Some(json!({
                        "app": app,
                        "checks": checks_rows,
                        "manual": manual_rows,
                        "probes": probes_rows,
                    }));
                    let mut fields: Vec<Field> =
                        es().checks_scalar.iter().map(EditFieldDef::field).collect();
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
                    (
                        format!("edit:checks:{stack}:{app}"),
                        stack.clone(),
                        format!("checks.yml · {app} · {stack}"),
                        plan_steps(FormStep {
                            id: "checks".into(),
                            label: "Checks".into(),
                            fields,
                        }),
                        format!("/app/stacks/{stack}/checks"),
                    )
                }
                _ => {
                    let fields: Vec<Field> = es().publish.iter().map(EditFieldDef::field).collect();
                    es_.model = Some(json!({ "app": app }));
                    (
                        format!("edit:publish:{stack}:{app}"),
                        stack.clone(),
                        format!("Publish {app} · {stack}"),
                        plan_steps(FormStep {
                            id: "publish".into(),
                            label: "Publish".into(),
                            fields,
                        }),
                        format!("/app/stacks/{stack}/apps"),
                    )
                }
            }
        }
        EditKind::NewStack => {
            no_target(target)?;
            unreadable("the presets")?;
            let presets = data["presets"].as_array().cloned().unwrap_or_default();
            if presets.is_empty() {
                return Err(refused(
                    step,
                    match data["sync_error"].as_str() {
                        Some(e) => format!("the working copy is not there: {e}"),
                        None => "the repository holds no presets".into(),
                    },
                    "the settings page shows the working copy's state",
                ));
            }
            (
                "new-stack".to_string(),
                String::new(),
                "New stack".to_string(),
                new_stack_steps(&presets, data["suggest_vmid"].as_u64()),
                st.page.clone(),
            )
        }
        EditKind::HostSettings => {
            no_target(target)?;
            unreadable("host.toml")?;
            if data["page"]["fields"].as_array().is_none() {
                return Err(refused(
                    step,
                    "host.toml was not read",
                    "the settings page says why; try again once the host answers",
                ));
            }
            (
                "edit:host-settings".to_string(),
                crate::core::actions::HOST_TARGET.to_string(),
                "Host settings".to_string(),
                vec![
                    FormStep {
                        id: "keys".into(),
                        label: "Keys".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "review".into(),
                        label: "Write host.toml".into(),
                        fields: Vec::new(),
                    },
                ],
                "/app/settings".to_string(),
            )
        }
        EditKind::Batch => {
            let action = form.strip_prefix("batch:").unwrap_or("");
            let entry = ActionKind::from_slug(action).filter(|k| {
                !k.host_wide()
                    && !k
                        .args()
                        .iter()
                        .any(|a| matches!(a, Arg::Unit | Arg::Commit))
            });
            let Some(ak) = entry else {
                let all: Vec<&str> = ActionKind::ALL
                    .iter()
                    .filter(|k| {
                        !k.host_wide()
                            && !k
                                .args()
                                .iter()
                                .any(|a| matches!(a, Arg::Unit | Arg::Commit))
                    })
                    .map(|k| k.slug())
                    .collect();
                return Err(refused(
                    step,
                    format!("{action:?} is not an action several stacks can be given at once"),
                    format!(
                        "homelab ui open batch <action> <stack>,<stack> with one of: {}",
                        all.join(", ")
                    ),
                ));
            };
            // Owner decision 2026-09-30: no stacks named opens the batch
            // dialog from the Overview table's own ticked selection
            // (`homelab ui select` first), exactly as the page's "Run on
            // the selected…" button does.
            let list: Vec<String> = match target {
                Some(t) => t
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect(),
                None => st.selected.clone(),
            };
            if list.is_empty() {
                return Err(refused(
                    step,
                    "a batch needs its stacks: none were named and the fleet table's selection is empty",
                    format!(
                        "homelab ui select <stack>,<stack> then homelab ui open batch {action}, \
                         or homelab ui open batch {action} <stack>,<stack>"
                    ),
                ));
            }
            for s in &list {
                stack_named(Some(s))?;
            }
            if ak.refused_for_self() && list.iter().any(|s| s == SELF_STACK) {
                return Err(refused(
                    step,
                    format!(
                        "The dashboard never does this to its own stack ({SELF_STACK}); leave it out of the selection."
                    ),
                    "name the other stacks only",
                ));
            }
            if ak.scope() > cx.scope {
                return Err(refused(
                    step,
                    format!(
                        "{action} needs scope {:?}; the token \"{}\" has {:?}",
                        ak.scope(),
                        cx.by,
                        cx.scope
                    ),
                    "drive it with a token of that scope, or leave it to Kenny",
                ));
            }
            es_.batch_action = Some(action.to_string());
            es_.stacks = list.clone();
            es_.guarded = data["guarded"].as_u64().unwrap_or(0) as usize;
            let n = list.len();
            (
                format!("batch:{action}"),
                list.join(","),
                format!(
                    "{} · {n} {}",
                    ak.label(),
                    if n == 1 { "stack" } else { "stacks" }
                ),
                vec![FormStep {
                    id: "review".into(),
                    label: "Review and run".into(),
                    fields: batch_fields(ak, &list),
                }],
                "/app/".to_string(),
            )
        }
        EditKind::Import => {
            no_target(target)?;
            unreadable("the taken names and numbers")?;
            if data["working_copy"] == json!(false) {
                return Err(refused(
                    step,
                    "the dashboard has no working copy to commit the new stack to",
                    "the settings page shows the working copy's state",
                ));
            }
            (
                "import".to_string(),
                String::new(),
                "Import a stack".to_string(),
                vec![
                    FormStep {
                        id: "bundle".into(),
                        label: "Bundle".into(),
                        fields: es().import.iter().map(EditFieldDef::field).collect(),
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                st.page.clone(),
            )
        }
        EditKind::Preset => {
            let Some(name) = target else {
                return Err(refused(
                    step,
                    format!("{} acts on one preset and none was named", kind.slug()),
                    format!("homelab ui open {}", kind.usage()),
                ));
            };
            if presetedit::valid_name(name).is_err() {
                return Err(refused(
                    step,
                    format!("{name:?} is not a preset name"),
                    "lowercase letters, digits and '-', not starting with '_'",
                ));
            }
            unreadable(&format!("the {name} preset"))?;
            if data["exists"] != json!(true) {
                return Err(refused(
                    step,
                    format!("there is no preset called {name} yet"),
                    format!("homelab ui open new-preset, then name it {name}"),
                ));
            }
            let meta = &data["meta"];
            let mut fields: Vec<Field> = es().preset_meta.iter().map(EditFieldDef::field).collect();
            for f in &mut fields {
                f.current = Some(preset_meta_current(&f.name, meta));
            }
            // feat-preset-1: the Files card's own fields and "Remove this
            // preset…" are only ever on screen for a preset that already
            // exists — the same `if (e.exists)` gate the browser's own
            // dialog uses.
            fields.extend(preset_file_fields(&data));
            (
                format!("edit:preset:{name}"),
                name.to_string(),
                format!("{name} · preset.yml"),
                vec![
                    FormStep {
                        id: "meta".into(),
                        label: "preset.yml".into(),
                        fields,
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                "/app/presets".to_string(),
            )
        }
        EditKind::NewPreset => {
            no_target(target)?;
            let mut fields: Vec<Field> = vec![es().new_preset_name.field()];
            for d in &es().preset_meta {
                let mut f = d.field();
                f.current = Some(preset_meta_current(&f.name, &Value::Null));
                fields.push(f);
            }
            (
                "new-preset".to_string(),
                String::new(),
                "New preset".to_string(),
                vec![
                    FormStep {
                        id: "meta".into(),
                        label: "New preset".into(),
                        fields,
                    },
                    FormStep {
                        id: "plan".into(),
                        label: "Plan".into(),
                        fields: Vec::new(),
                    },
                    FormStep {
                        id: "commit".into(),
                        label: "Commit".into(),
                        fields: Vec::new(),
                    },
                ],
                "/app/presets".to_string(),
            )
        }
        EditKind::Rollback => {
            let stack = stack_named(target)?;
            unreadable("the roll-back options")?;
            let none = Choice {
                value: String::new(),
                label: "None".into(),
            };
            let mut fields: Vec<Field> = es().rollback.iter().map(EditFieldDef::field).collect();
            for f in &mut fields {
                let mut c = vec![none.clone()];
                if f.name == "commit" {
                    for x in data["commits"].as_array().into_iter().flatten() {
                        let commit = x["commit"].as_str().unwrap_or("").to_string();
                        let short: String = commit.chars().take(10).collect();
                        c.push(Choice {
                            label: format!("{short} · {}", x["subject"].as_str().unwrap_or("")),
                            value: commit,
                        });
                    }
                } else {
                    for u in strings(&data["native_units"]) {
                        c.push(Choice {
                            value: u.clone(),
                            label: u,
                        });
                    }
                }
                f.choices = Some(c);
                f.current = Some(json!(""));
            }
            (
                format!("rollback:{stack}"),
                stack.clone(),
                format!("Roll back · {stack}"),
                vec![FormStep {
                    id: "choose".into(),
                    label: "Choose".into(),
                    fields,
                }],
                format!("/app/stacks/{stack}"),
            )
        }
    };
    st.page = page;
    st.form = Some(OpenForm::new_edit(id, stack, title, steps, es_));
    Ok(plain())
}

/// What else changes when one field changes, as the page's own handlers
/// do it: the preset's size, the raw file's text, the firewall's model.
pub fn after_set(form: &mut OpenForm, name: &str, in_sub: bool) {
    let Family::Edit(kind) = form.desc.family else {
        return;
    };
    if in_sub {
        return;
    }
    let value = form.values.get(name).cloned().unwrap_or(Value::Null);
    match (kind, name) {
        (EditKind::NewStack, "preset") => {
            let p = edit(form).data["presets"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|p| p["name"] == value)
                .cloned();
            if let Some(p) = p {
                for k in ["ram_mb", "cores", "disk_gb"] {
                    let v = text_of(Some(&p[k]));
                    form.values.insert(k.into(), json!(v));
                }
            }
        }
        (EditKind::Raw, "file") => {
            let t = edit(form).data["texts"][value.as_str().unwrap_or("")].clone();
            form.values
                .insert("text".into(), if t.is_null() { json!("") } else { t });
        }
        (EditKind::Preset, "file") => {
            // Mirrors `filesCard`'s own `show()`: picking a file fills
            // its path (then locked to it in the real form) and its
            // current text; picking "(new file)" clears both.
            let f = value.as_str().unwrap_or("").to_string();
            form.values.insert("path".into(), json!(f));
            let content = if f.is_empty() {
                String::new()
            } else {
                text_of(edit(form).data["files"].get(f.as_str()))
            };
            form.values.insert("text".into(), json!(content));
        }
        (EditKind::Preset, "template") => {
            // Mirrors `templateSel`'s own change handler: only fills the
            // text when a NEW file is being made (no file picked).
            if text_of(form.values.get("file")).trim().is_empty() {
                let t = value.as_str().unwrap_or("").to_string();
                let content = if t.is_empty() {
                    String::new()
                } else {
                    text_of(edit(form).data["file_templates"].get(t.as_str()))
                };
                form.values.insert("text".into(), json!(content));
            }
        }
        (EditKind::AddApp, "preset") => {
            let preset_name = value.as_str().unwrap_or("").to_string();
            let data = edit(form).data.clone();
            if let Some(step) = form.desc.steps.first_mut() {
                let preset_field = step
                    .fields
                    .iter()
                    .find(|f| f.name == "preset")
                    .cloned()
                    .unwrap_or_else(|| es().add_app[0].field());
                step.fields = add_app_step_fields(preset_field, &data, &preset_name);
            }
            // A tile value typed for an app the new preset does not bring
            // in is stale; drop it rather than silently keep sending it.
            let keep: std::collections::BTreeSet<String> = form
                .desc
                .steps
                .first()
                .map(|s| s.fields.iter().map(|f| f.name.clone()).collect())
                .unwrap_or_default();
            form.values
                .retain(|k, _| !k.starts_with("add-app-tile:") || keep.contains(k));
        }
        (EditKind::Firewall, _) => {
            if let Some(m) = edit(form).model.as_mut() {
                if m.get(name).is_some() {
                    m[name] = value;
                }
            }
        }
        (EditKind::Rollback, "commit") if value != json!("") => {
            form.values.insert("unit".into(), json!(""));
        }
        (EditKind::Rollback, "unit") if value != json!("") => {
            form.values.insert("commit".into(), json!(""));
        }
        (EditKind::Native, "unit") => {
            let natives = edit(form).data["natives"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let m = natives
                .iter()
                .find(|n| n["unit"] == value)
                .map(|n| n["manifest"].clone())
                .unwrap_or(Value::Null);
            for field in [
                "binary",
                "env_file",
                "data_dirs",
                "update_cmd",
                "stateless",
                "restore_note",
                "release_repo",
                "release_asset",
                "backup_from_newest",
                "backup_pause",
                "update_policy",
                "metrics",
            ] {
                form.values.insert(field.into(), native_current(field, &m));
            }
        }
        _ => {}
    }
}

/// The buttons on screen and the table in words.
pub fn refresh(form: &mut OpenForm) {
    let Family::Edit(kind) = form.desc.family else {
        return;
    };
    let step = form.step.clone();
    let first = form.step_index == 0;
    let last = form.step_index == form.last();
    let e = form.edit.as_ref().expect("an edit form has its edit state");
    let mut b: Vec<&str> = Vec::new();
    if e.result.is_some() || form.job.is_some() {
        b.push("close");
    } else if let Some(sub) = &e.sub {
        b.push("save");
        if sub.kind == "key" {
            b.push("default");
        }
        b.push("cancel");
    } else {
        let valid = e.plan.as_ref().is_some_and(|p| p["valid"] == true);
        if !first {
            b.push("back");
        }
        match kind {
            _ if kind.stack_edit() || kind == EditKind::Import => match step.as_str() {
                "plan" => {
                    if valid {
                        b.push("next");
                    }
                }
                "commit" => b.push("confirm"),
                _ => {
                    let changed = match (kind, &e.model) {
                        (EditKind::Firewall, Some(m)) => {
                            firewall_changed(m, &e.data["manifest"]["firewall"])
                        }
                        _ => true,
                    };
                    if changed {
                        b.push("next");
                    }
                    // feat-native-1: a unit can be removed straight from
                    // the native step, the same "remove_native" the click
                    // path's own `#native-remove` button sends — a second
                    // button alongside "next", not its own `EditKind`.
                    if kind == EditKind::Native {
                        b.push("remove");
                    }
                }
            },
            _ if kind.preset_edit() => match step.as_str() {
                "plan" => {
                    if valid {
                        b.push("next");
                    }
                }
                "commit" => b.push("confirm"),
                _ => {
                    b.push("next");
                    // feat-preset-1: the Files card's own three buttons
                    // and "Remove this preset…" — only drawn for a preset
                    // that already exists, matching `preset_file_fields`.
                    if kind == EditKind::Preset {
                        b.push("save-file");
                        b.push("rename-file");
                        b.push("delete-file");
                        b.push("remove-preset");
                    }
                }
            },
            EditKind::NewStack => {
                if !last {
                    b.push("next");
                } else if valid {
                    b.push("confirm");
                }
            }
            EditKind::HostSettings => {
                if step == "review" {
                    b.push("confirm");
                } else if !e.staged.is_empty() {
                    b.push("next");
                }
            }
            EditKind::Batch => b.push("confirm"),
            _ => b.push("next"),
        }
        b.push("close");
    }
    form.buttons = b.into_iter().map(str::to_string).collect();
    let rows = match kind {
        EditKind::Firewall => e
            .model
            .as_ref()
            .and_then(|m| m["rules"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("{}. {}{new}", i + 1, rule_summary(&r["rule"]))
            })
            .collect(),
        EditKind::HostSettings => e.data["page"]["fields"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|f| matches!(f["access"].as_str(), Some("browser" | "confirm")))
            .map(|f| {
                let key = f["key"].as_str().unwrap_or("");
                match e.staged.get(key) {
                    Some(v) => format!(
                        "{key} → {}",
                        if v.is_null() {
                            "(default)".to_string()
                        } else {
                            text_of(Some(v))
                        }
                    ),
                    None if f["set"] == true => format!("{key} = {}", key_text(f)),
                    None => format!("{key} (default {})", f["default"].as_str().unwrap_or("")),
                }
            })
            .collect(),
        EditKind::SettingsExt => e
            .model
            .as_ref()
            .and_then(|m| m["retention"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("retention {}. {}{new}", i + 1, retention_row_summary(r))
            })
            .collect(),
        EditKind::Apps => ["storage", "data_mounts", "log_files"]
            .iter()
            .flat_map(|list| {
                let summary_of: fn(&Value) -> String = match *list {
                    "storage" => storage_summary,
                    "data_mounts" => data_mount_summary,
                    _ => log_file_summary,
                };
                e.model
                    .as_ref()
                    .and_then(|m| m[*list].as_array())
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(move |(i, r)| {
                        let new = if r["origin"].is_null() { " (new)" } else { "" };
                        format!("{list} {}. {}{new}", i + 1, summary_of(r))
                    })
            })
            .collect(),
        EditKind::Latch => e
            .model
            .as_ref()
            .and_then(|m| m["latch_files"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("latch_files {}. {}{new}", i + 1, latch_file_summary(r))
            })
            .collect(),
        EditKind::Checks => ["checks", "manual", "probes"]
            .iter()
            .flat_map(|list| {
                let summary_of: fn(&Value) -> String = match *list {
                    "checks" => check_row_summary,
                    "manual" => manual_row_summary,
                    _ => probe_row_summary,
                };
                e.model
                    .as_ref()
                    .and_then(|m| m[*list].as_array())
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .map(move |(i, r)| {
                        let new = if r["origin"].is_null() { " (new)" } else { "" };
                        format!("{list} {}. {}{new}", i + 1, summary_of(r))
                    })
            })
            .collect(),
        EditKind::Tiles => e
            .model
            .as_ref()
            .and_then(|m| m["tiles"].as_array())
            .into_iter()
            .flatten()
            .enumerate()
            .map(|(i, r)| {
                let new = if r["origin"].is_null() { " (new)" } else { "" };
                format!("tiles {}. {}{new}", i + 1, tile_row_summary(r))
            })
            .collect(),
        _ => Vec::new(),
    };
    edit(form).rows = rows;
    form.fields = form.field_states();
}

/// `homelab ui row …`: the firewall's rules, host.toml's keys.
pub fn row(
    form: &mut OpenForm,
    step: &UiStep,
    op: &str,
    target: Option<&str>,
) -> Result<Applied, Refusal> {
    let Family::Edit(kind) = form.desc.family else {
        return Err(refused(
            step,
            format!("the form {} has no table", form.title),
            "row steps are for the firewall and the host settings",
        ));
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
        EditKind::Firewall => {
            if form.step != "rules" {
                return Err(refused(
                    step,
                    format!("the rules are not on screen on the step {}", form.step),
                    "press back to the rules",
                ));
            }
            let model = edit(form).model.clone().unwrap_or(Value::Null);
            let rules = model["rules"].as_array().cloned().unwrap_or_default();
            let n = rules.len();
            let index = || -> Result<usize, Refusal> {
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
            };
            let set_rules = |form: &mut OpenForm, rules: Vec<Value>| {
                if let Some(m) = edit(form).model.as_mut() {
                    m["rules"] = Value::Array(rules);
                }
            };
            match op {
                "add" | "edit" => {
                    let (target, rule) = if op == "add" {
                        if let Some(t) = target {
                            return Err(refused(
                                step,
                                format!("row add takes no number, not {t}"),
                                "homelab ui row add",
                            ));
                        }
                        (Value::Null, None)
                    } else {
                        let i = index()?;
                        (json!(i), Some(rules[i]["rule"].clone()))
                    };
                    let fields = rule_fields(rule.as_ref());
                    let values = fields
                        .iter()
                        .map(|f| (f.name.clone(), drive::start_value(f)))
                        .collect();
                    let title = match &rule {
                        Some(r) => format!("Edit rule · {}", rule_summary(r)),
                        None => "Add a rule".to_string(),
                    };
                    edit(form).sub = Some(Sub {
                        kind: "rule".into(),
                        title,
                        target,
                        fields,
                        values,
                        errors: BTreeMap::new(),
                    });
                }
                "up" | "down" => {
                    let i = index()?;
                    let j = if op == "up" {
                        i.checked_sub(1)
                    } else {
                        Some(i + 1).filter(|j| *j < n)
                    };
                    let Some(j) = j else {
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
                    let mut rules = rules;
                    rules.swap(i, j);
                    set_rules(form, rules);
                }
                "delete" => {
                    let i = index()?;
                    let mut rules = rules;
                    rules.remove(i);
                    set_rules(form, rules);
                }
                other => {
                    return Err(refused(
                        step,
                        format!("there is no row step {other}"),
                        "row add, or row edit|up|down|delete <number>",
                    ))
                }
            }
            Ok(plain())
        }
        EditKind::HostSettings => {
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
            let fields = e.data["page"]["fields"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let Some(f) = fields.iter().find(|f| f["key"] == key).cloned() else {
                return Err(refused(
                    step,
                    format!("host.toml has no key {key:?}"),
                    "the keys are the rows of homelab ui state",
                ));
            };
            let access = f["access"].as_str().unwrap_or("");
            if !matches!(access, "browser" | "confirm") {
                let who = match access {
                    "locked" => "ssh only (can cut the dashboard off)",
                    "ssh_only" => "ssh only (safety policy)",
                    _ => "ssh only (secret)",
                };
                return Err(refused(
                    step,
                    format!("{key} is changed {who}"),
                    "change it over ssh on pve, not from the dashboard",
                ));
            }
            let ty = f["kind"]["type"].as_str().unwrap_or("text");
            let slug = key.replace('_', "-");
            let ids = &es().host_key;
            let pending = e.staged.get(key);
            let start = match (pending, ty) {
                (Some(v), "bool") => json!(*v == Value::Bool(true)),
                (Some(v), _) => json!(if v.is_null() {
                    String::new()
                } else {
                    text_of(Some(v))
                }),
                (None, "bool") => json!(f["value"] == Value::Bool(true)),
                (None, _) => json!(key_text(&f)),
            };
            let mut value = Field {
                name: "value".into(),
                id: fill(&ids.id, &[("slug", &slug)]),
                kind: match ty {
                    "bool" => FieldKind::Check,
                    "table" => FieldKind::Textarea,
                    _ => FieldKind::Text,
                },
                label: f["label"].as_str().unwrap_or(key).to_string(),
                help: f["help"].as_str().unwrap_or("").to_string(),
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
            };
            value.current = Some(start);
            let mut sub_fields = vec![value];
            if access == "confirm" {
                sub_fields.push(Field {
                    name: "confirm".into(),
                    id: fill(&ids.confirm_id, &[("slug", &slug)]),
                    kind: FieldKind::Typed,
                    label: fill(&ids.confirm_label, &[("key", key)]),
                    help: String::new(),
                    required: true,
                    pattern: None,
                    placeholder: Some(key.to_string()),
                    source: None,
                    empty: None,
                    danger: true,
                    when: None,
                    expect: Some(key.to_string()),
                    min: None,
                    max: None,
                    choices: None,
                    current: None,
                    show_when: None,
                    change_when: None,
                });
            }
            let values = sub_fields
                .iter()
                .map(|f| (f.name.clone(), drive::start_value(f)))
                .collect();
            e.sub = Some(Sub {
                kind: "key".into(),
                title: format!("{} · {key}", f["label"].as_str().unwrap_or(key)),
                target: json!(key),
                fields: sub_fields,
                values,
                errors: BTreeMap::new(),
            });
            Ok(plain())
        }
        EditKind::SettingsExt
        | EditKind::Apps
        | EditKind::Latch
        | EditKind::Checks
        | EditKind::Tiles => {
            // SettingsExt's own step is "settings_ext" (its form's field
            // grouping predates `kind.slug()`'s hyphen, "settings-ext",
            // which only ever named the `homelab ui open` verb).
            let expected_step = if kind == EditKind::SettingsExt {
                "settings_ext"
            } else {
                kind.slug()
            };
            if form.step != expected_step {
                return Err(refused(
                    step,
                    format!("the rows are not on screen on the step {}", form.step),
                    "press back",
                ));
            }
            let raw_target = target.unwrap_or("");
            let (list, n) = match raw_target.split_once(':') {
                Some((l, n)) => (l, Some(n)),
                None => (raw_target, None),
            };
            let allowed: &[&str] = match kind {
                EditKind::SettingsExt => &["retention"],
                EditKind::Apps => &["storage", "data_mounts", "log_files"],
                EditKind::Checks => &["checks", "manual", "probes"],
                EditKind::Tiles => &["tiles"],
                _ => &["latch_files"],
            };
            if !allowed.contains(&list) {
                return Err(refused(
                    step,
                    format!("{raw_target:?} does not name a list of this form"),
                    format!("the lists are: {}", allowed.join(", ")),
                ));
            }
            let model = edit(form).model.clone().unwrap_or(Value::Null);
            let rows = model[list].as_array().cloned().unwrap_or_default();
            let count = rows.len();
            let index = || -> Result<usize, Refusal> {
                let t = n.unwrap_or("");
                match t.parse::<usize>() {
                    Ok(i) if (1..=count).contains(&i) => Ok(i - 1),
                    _ => Err(refused(
                        step,
                        format!("there is no row {t:?} of {list}"),
                        if count == 0 {
                            format!("there are no {list} rows yet; homelab ui row add {list}")
                        } else {
                            format!("{list} rows are numbered 1 to {count}")
                        },
                    )),
                }
            };
            let set_rows = |form: &mut OpenForm, rows: Vec<Value>| {
                if let Some(m) = edit(form).model.as_mut() {
                    m[list] = Value::Array(rows);
                }
            };
            let apps = strings(&edit(form).data["manifest"]["apps"]);
            let natives = strings(&edit(form).data["manifest"]["natives"]);
            let groups: Vec<String> = edit(form).data["manifest"]["tiles"]
                .as_object()
                .into_iter()
                .flatten()
                .filter_map(|(_, t)| t["group"].as_str().map(str::to_string))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            let fields_for = |row: Option<&Value>| -> Vec<Field> {
                match list {
                    "storage" => storage_fields(&apps, row),
                    "data_mounts" => data_mount_fields(row),
                    "log_files" => log_file_fields(row),
                    "latch_files" => latch_file_fields(&natives, row),
                    "checks" => check_row_fields(row),
                    "manual" => manual_row_fields(row),
                    "probes" => probe_row_fields(row),
                    "retention" => retention_row_fields(row),
                    _ => tile_row_fields(&groups, row),
                }
            };
            let summary_of = |r: &Value| -> String {
                match list {
                    "storage" => storage_summary(r),
                    "data_mounts" => data_mount_summary(r),
                    "log_files" => log_file_summary(r),
                    "latch_files" => latch_file_summary(r),
                    "checks" => check_row_summary(r),
                    "manual" => manual_row_summary(r),
                    "probes" => probe_row_summary(r),
                    "retention" => retention_row_summary(r),
                    _ => tile_row_summary(r),
                }
            };
            match op {
                "add" | "edit" => {
                    let (sub_target, row) = if op == "add" {
                        if n.is_some() {
                            return Err(refused(
                                step,
                                format!("row add takes {list:?}, not {raw_target:?}"),
                                format!("homelab ui row add {list}"),
                            ));
                        }
                        (Value::Null, None)
                    } else {
                        let i = index()?;
                        (json!(i), Some(rows[i].clone()))
                    };
                    let fields = fields_for(row.as_ref());
                    let values = fields
                        .iter()
                        .map(|f| (f.name.clone(), drive::start_value(f)))
                        .collect();
                    let title = match &row {
                        Some(r) => format!("Edit · {}", summary_of(r)),
                        None => format!("Add a {} row", list.trim_end_matches('s')),
                    };
                    edit(form).sub = Some(Sub {
                        kind: list.to_string(),
                        title,
                        target: sub_target,
                        fields,
                        values,
                        errors: BTreeMap::new(),
                    });
                }
                "up" | "down" => {
                    let i = index()?;
                    let j = if op == "up" {
                        i.checked_sub(1)
                    } else {
                        Some(i + 1).filter(|j| *j < count)
                    };
                    let Some(j) = j else {
                        return Err(refused(
                            step,
                            format!(
                                "row {} is the {} already",
                                i + 1,
                                if op == "up" { "first" } else { "last" }
                            ),
                            "move another row",
                        ));
                    };
                    let mut rows = rows;
                    rows.swap(i, j);
                    set_rows(form, rows);
                }
                "delete" => {
                    let i = index()?;
                    let mut rows = rows;
                    rows.remove(i);
                    set_rows(form, rows);
                }
                other => {
                    return Err(refused(
                        step,
                        format!("there is no row step {other}"),
                        format!("row add {list}, or row edit|up|down|delete {list}:<number>"),
                    ))
                }
            }
            Ok(plain())
        }
        _ => Err(refused(
            step,
            format!("the form {} has no table", form.title),
            "row steps are for the firewall and the host settings",
        )),
    }
}

/// A press in an edit form.
pub fn press(
    form: &mut OpenForm,
    step: &UiStep,
    button: &str,
    cx: &Ctx,
) -> Result<Applied, Refusal> {
    let Family::Edit(kind) = form.desc.family else {
        return Err(refused(step, "not an edit form", "homelab ui state"));
    };
    if form.sent() {
        return Err(refused(
            step,
            "this form was sent already; one press runs once",
            "homelab ui state shows what it did; close the dialog to start over",
        ));
    }
    if !form.buttons.iter().any(|b| b == button) {
        return Err(refused(
            step,
            format!("{button} is not on screen on the step {}", form.step),
            format!("the buttons on screen are: {}", form.buttons.join(", ")),
        ));
    }
    if kind.scope() > cx.scope {
        return Err(refused(
            step,
            format!("{} needs scope {:?}", kind.slug(), kind.scope()),
            "leave this press to Kenny",
        ));
    }
    // A dialog on top first.
    if let Some(sub) = form.edit.as_ref().and_then(|e| e.sub.clone()) {
        if button == "cancel" {
            edit(form).sub = None;
            return Ok(plain());
        }
        if sub.kind == "rule" {
            let mut errors = check_fields(&sub.fields, &sub.values);
            errors.extend(rule_problems(&sub.values));
            if !errors.is_empty() {
                let r = held_for(form, "save", &errors);
                if let Some(s) = edit(form).sub.as_mut() {
                    s.errors = errors;
                }
                return Ok(Applied {
                    effect: Effect::None,
                    held: Some(r),
                });
            }
            let rule = rule_from_values(&sub.values);
            let e = edit(form);
            if let Some(m) = e.model.as_mut() {
                let rules = m["rules"].as_array_mut().expect("the model has rules");
                match sub.target.as_u64() {
                    Some(i) => {
                        if let Some(x) = rules.get_mut(i as usize) {
                            x["rule"] = rule;
                        }
                    }
                    None => rules.push(json!({ "origin": null, "rule": rule })),
                }
            }
            e.sub = None;
            return Ok(plain());
        }
        if EditKind::row_list_kind(&sub.kind) {
            let list = sub.kind.as_str();
            let mut errors = check_fields(&sub.fields, &sub.values);
            errors.extend(match list {
                "data_mounts" => data_mount_problems(&sub.values),
                "latch_files" => latch_file_problems(&sub.values),
                "checks" => check_row_problems(&sub.values),
                "probes" => probe_row_problems(&sub.values),
                "tiles" => tile_row_problems(&sub.values),
                "retention" => retention_row_problems(&sub.values),
                _ => BTreeMap::new(),
            });
            if !errors.is_empty() {
                let r = held_for(form, "save", &errors);
                if let Some(s) = edit(form).sub.as_mut() {
                    s.errors = errors;
                }
                return Ok(Applied {
                    effect: Effect::None,
                    held: Some(r),
                });
            }
            let mut row = match list {
                "storage" => storage_from_values(&sub.values),
                "data_mounts" => data_mount_from_values(&sub.values),
                "log_files" => log_file_from_values(&sub.values),
                "latch_files" => latch_file_from_values(&sub.values),
                "checks" => check_row_from_values(&sub.values),
                "manual" => manual_row_from_values(&sub.values),
                "probes" => probe_row_from_values(&sub.values),
                "retention" => retention_row_from_values(&sub.values),
                _ => tile_row_from_values(&sub.values),
            };
            let e = edit(form);
            if let Some(m) = e.model.as_mut() {
                let rows = m[list].as_array_mut().expect("the model has this list");
                match sub.target.as_u64() {
                    Some(i) => {
                        if let Some(old) = rows.get(i as usize) {
                            row["origin"] = old["origin"].clone();
                        }
                        if let Some(x) = rows.get_mut(i as usize) {
                            *x = row;
                        }
                    }
                    None => {
                        row["origin"] = Value::Null;
                        rows.push(row);
                    }
                }
            }
            e.sub = None;
            return Ok(plain());
        }
        // A host.toml key: save or default.
        let key = sub.target.as_str().unwrap_or("").to_string();
        let mut errors = BTreeMap::new();
        let confirm = sub.fields.iter().any(|f| f.name == "confirm");
        if confirm && text_of(sub.values.get("confirm")).trim() != key {
            errors.insert("confirm".to_string(), say("key_confirm", &[("key", &key)]));
        }
        let value = if button == "default" {
            Ok(Value::Null)
        } else {
            let kind = edit(form).data["page"]["fields"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|f| f["key"] == key.as_str())
                .map(|f| f["kind"].clone())
                .unwrap_or(Value::Null);
            parse_key(&kind, sub.values.get("value").unwrap_or(&Value::Null))
        };
        let value = match value {
            Ok(v) if errors.is_empty() => v,
            Ok(_) => Value::Null,
            Err(why) => {
                errors.insert("value".into(), why);
                Value::Null
            }
        };
        if !errors.is_empty() {
            let r = held_for(form, button, &errors);
            if let Some(s) = edit(form).sub.as_mut() {
                s.errors = errors;
            }
            return Ok(Applied {
                effect: Effect::None,
                held: Some(r),
            });
        }
        let e = edit(form);
        if confirm {
            e.confirmed.insert(key.clone());
        }
        e.staged.insert(key, value);
        e.sub = None;
        return Ok(plain());
    }
    if button == "back" {
        form.step_index = form.step_index.saturating_sub(1);
        form.errors.clear();
        form.run_error = None;
        return Ok(plain());
    }
    let step_id = form.step.clone();
    let hold = |form: &mut OpenForm, what: &str, errors: BTreeMap<String, String>| {
        let r = held_for(form, what, &errors);
        form.errors = errors;
        Ok(Applied {
            effect: Effect::None,
            held: Some(r),
        })
    };
    match (kind, step_id.as_str(), button) {
        (EditKind::Import, "plan", "next") => {
            form.step_index += 1;
            Ok(plain())
        }
        (EditKind::Import, "commit", "confirm") => {
            if text_of(form.values.get("subject")).trim().is_empty() {
                let fields: Vec<Field> = form.desc.steps[2].fields.clone();
                let mut errors = check_fields(&fields, &form.values);
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
            let body = commit_body(&edit_body, &form.values);
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::ImportCommit { body }),
                held: None,
            })
        }
        (EditKind::Import, "bundle", "next") => {
            let mut at = form.desc.steps[0].clone();
            // The name and the number are checked as the new-stack wizard
            // checks its identity step.
            at.id = "identity".into();
            let names = strings(&edit(form).data["taken"]["names"]);
            let vmids: Vec<i64> = edit(form).data["taken"]["vmids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
            let errors = check_new_step(&at, &form.values, &names, &vmids);
            if !errors.is_empty() {
                return hold(form, "next", errors);
            }
            let body = json!({
                "bundle": text_of(form.values.get("bundle")),
                "name": text_of(form.values.get("name")).trim(),
                "vmid": js_number(&text_of(form.values.get("vmid"))),
            });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::ImportPlan { body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "next") => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let name = form.stack.clone();
            let unprivileged = edit(form).data["meta"]["unprivileged"].clone();
            let meta = preset_meta_body(&fields, &form.values, unprivileged);
            let body = json!({ "kind": "meta", "name": name, "meta": meta });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::NewPreset, "meta", "next") => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let name = text_of(form.values.get("name")).trim().to_string();
            if name.is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("name".into(), say("needed", &[("label", "Preset name")]));
                return hold(form, "next", errors);
            }
            let meta = preset_meta_body(&fields, &form.values, Value::Null);
            let body = json!({ "kind": "meta", "name": name, "meta": meta });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "save-file") => {
            let name = form.stack.clone();
            let path = text_of(form.values.get("path")).trim().to_string();
            if path.is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("path".into(), say("needed", &[("label", "Path")]));
                return hold(form, "save-file", errors);
            }
            let content = text_of(form.values.get("text"));
            let body = json!({ "kind": "file", "name": name, "path": path, "content": content });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("save-file".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "rename-file") => {
            let name = form.stack.clone();
            let from = text_of(form.values.get("file")).trim().to_string();
            let to = text_of(form.values.get("rename_to")).trim().to_string();
            let mut errors = BTreeMap::new();
            if from.is_empty() {
                errors.insert("file".into(), "pick the file to rename first".into());
            }
            if to.is_empty() {
                errors.insert("rename_to".into(), say("needed", &[("label", "New path")]));
            }
            if !errors.is_empty() {
                return hold(form, "rename-file", errors);
            }
            let body = json!({ "kind": "rename_file", "name": name, "from": from, "to": to });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("rename-file".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "delete-file") => {
            let name = form.stack.clone();
            let path = text_of(form.values.get("file")).trim().to_string();
            if path.is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("file".into(), "pick the file to delete first".into());
                return hold(form, "delete-file", errors);
            }
            let body = json!({ "kind": "remove_file", "name": name, "path": path });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("delete-file".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (EditKind::Preset, "meta", "remove-preset") => {
            let name = form.stack.clone();
            let body = json!({ "kind": "remove", "name": name });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.action = Some("remove-preset".into());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetPlan { edit: body }),
                held: None,
            })
        }
        (k, "plan", "next") if k.preset_edit() => {
            form.step_index += 1;
            Ok(plain())
        }
        (k, "commit", "confirm") if k.preset_edit() => {
            if text_of(form.values.get("subject")).trim().is_empty() {
                let fields: Vec<Field> = form.desc.steps[2].fields.clone();
                let mut errors = check_fields(&fields, &form.values);
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
            let body = commit_body(&edit_body, &form.values);
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::PresetCommit { body }),
                held: None,
            })
        }
        (k, "plan", "next") if k.stack_edit() => {
            form.step_index += 1;
            Ok(plain())
        }
        (k, "commit", "confirm") if k.stack_edit() => {
            let fields: Vec<Field> = form.desc.steps[2].fields.clone();
            if text_of(form.values.get("subject")).trim().is_empty() {
                let mut errors = check_fields(&fields, &form.values);
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let edit_body = edit(form).body.clone().unwrap_or(Value::Null);
            let body = commit_body(&edit_body, &form.values);
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Commit {
                    stack: form.stack.clone(),
                    body,
                }),
                held: None,
            })
        }
        (EditKind::Native, _, "remove") => {
            let unit = text_of(form.values.get("unit"));
            let body = json!({ "kind": "remove_native", "unit": unit });
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Plan {
                    stack: form.stack.clone(),
                    edit: body,
                }),
                held: None,
            })
        }
        (k, _, "next") if k.stack_edit() => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let body = match k {
                EditKind::Settings => {
                    let mut errors = check_fields(&fields, &form.values);
                    errors.extend(tile_problems(&form.values));
                    if !errors.is_empty() {
                        return hold(form, "next", errors);
                    }
                    let b = settings_body(&fields, &form.values);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Raw => {
                    let label_of = |name: &str| {
                        fields
                            .iter()
                            .find(|f| f.name == name)
                            .map(|f| f.label.clone())
                            .unwrap_or_else(|| name.to_string())
                    };
                    let op = text_of(form.values.get("op"));
                    match op.as_str() {
                        // feat-stacks-files: create, delete and rename, the
                        // same ops the Files card's buttons send
                        // (stackedit_files::FilesEdit) — "edit" (the
                        // default, unset `op` included) stays the
                        // original raw edit below.
                        "create" => {
                            let path = text_of(form.values.get("new_path"));
                            if path.trim().is_empty() {
                                let mut errors = BTreeMap::new();
                                errors.insert(
                                    "new_path".into(),
                                    say("needed", &[("label", &label_of("new_path"))]),
                                );
                                return hold(form, "next", errors);
                            }
                            let text = text_of(form.values.get("text"));
                            json!({ "kind": "files", "op": "create", "path": path, "content": text })
                        }
                        "delete" => {
                            let file = text_of(form.values.get("file"));
                            json!({ "kind": "files", "op": "delete", "path": file })
                        }
                        "rename" => {
                            let file = text_of(form.values.get("file"));
                            let to = text_of(form.values.get("rename_to"));
                            if to.trim().is_empty() {
                                let mut errors = BTreeMap::new();
                                errors.insert(
                                    "rename_to".into(),
                                    say("needed", &[("label", &label_of("rename_to"))]),
                                );
                                return hold(form, "next", errors);
                            }
                            json!({ "kind": "files", "op": "rename", "from": file, "to": to })
                        }
                        _ => {
                            let file = text_of(form.values.get("file"));
                            let text = text_of(form.values.get("text"));
                            if edit(form).data["texts"][&file] == json!(text) {
                                return Ok(held_msg("next", &say("raw_unchanged", &[])));
                            }
                            json!({ "kind": "raw", "path": file, "content": text })
                        }
                    }
                }
                EditKind::AddApp => {
                    let mut tiles = Map::new();
                    for f in &fields {
                        let Some(app) = f.name.strip_prefix("add-app-tile:") else {
                            continue;
                        };
                        let hostname = text_of(form.values.get(&f.name)).trim().to_string();
                        if !hostname.is_empty() {
                            tiles.insert(app.to_string(), json!(hostname));
                        }
                    }
                    let mut out = Map::new();
                    out.insert("kind".into(), json!("add_app"));
                    out.insert("preset".into(), json!(text_of(form.values.get("preset"))));
                    if !tiles.is_empty() {
                        out.insert("tiles".into(), Value::Object(tiles));
                    }
                    Value::Object(out)
                }
                EditKind::Native => native_body(&fields, &form.values),
                EditKind::AddNative => {
                    let unit = text_of(form.values.get("unit"));
                    let binary = text_of(form.values.get("binary"));
                    if unit.trim().is_empty() || binary.trim().is_empty() {
                        let mut errors = BTreeMap::new();
                        if unit.trim().is_empty() {
                            errors.insert("unit".into(), say("needed", &[("label", "Unit name")]));
                        }
                        if binary.trim().is_empty() {
                            errors.insert("binary".into(), say("needed", &[("label", "Binary")]));
                        }
                        return hold(form, "next", errors);
                    }
                    add_native_body(&fields, &form.values)
                }
                EditKind::SettingsExt => {
                    let errors = check_fields(&fields, &form.values);
                    if !errors.is_empty() {
                        return hold(form, "next", errors);
                    }
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = settings_ext_body(&fields, &form.values, &model, &data["manifest"]);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Apps => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = apps_body(&fields, &form.values, &model, &data["manifest"]);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Latch => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = latch_body(&fields, &form.values, &model, &data["manifest"]);
                    if !changes_something(&b) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Checks => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data["manifest"].clone();
                    let app = model["app"].as_str().unwrap_or("").to_string();
                    let b = checks_drive_body(&form.values, &model, &data, &app);
                    if !checks_changed(&b, &data, &app) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::Tiles => {
                    let e = edit(form);
                    let model = e.model.clone().unwrap_or(Value::Null);
                    let data = e.data.clone();
                    let b = tiles_drive_body(&model, &data);
                    if b["tiles"].as_array().is_none_or(Vec::is_empty) {
                        return Ok(held_msg("next", &say("unchanged", &[])));
                    }
                    b
                }
                EditKind::PublishApp => {
                    let errors = check_fields(&fields, &form.values);
                    if !errors.is_empty() {
                        return hold(form, "next", errors);
                    }
                    let app = edit(form)
                        .model
                        .as_ref()
                        .and_then(|m| m["app"].as_str())
                        .unwrap_or("")
                        .to_string();
                    publish_body(&form.values, &app)
                }
                _ => firewall_body(edit(form).model.as_ref().unwrap_or(&Value::Null)),
            };
            form.errors.clear();
            form.step_index = 1;
            let e = edit(form);
            e.body = Some(body.clone());
            e.plan = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Plan {
                    stack: form.stack.clone(),
                    edit: body,
                }),
                held: None,
            })
        }
        (EditKind::NewStack, "plan", "confirm") => {
            if text_of(form.values.get("subject")).trim().is_empty() {
                let mut errors = BTreeMap::new();
                errors.insert("subject".into(), say("subject", &[]));
                return hold(form, "confirm", errors);
            }
            let follow = match form.values.get("follow") {
                Some(Value::String(s)) if !s.is_empty() => s.clone(),
                _ => "none".into(),
            };
            let mut body = Map::new();
            body.insert("stack".into(), new_stack_body(&form.values));
            body.insert(
                "subject".into(),
                json!(text_of(form.values.get("subject")).trim()),
            );
            let note = text_of(form.values.get("note")).trim().to_string();
            if !note.is_empty() {
                body.insert("note".into(), json!(note));
            }
            if follow != "none" {
                body.insert("follow".into(), json!(follow));
            }
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::NewCommit {
                    body: Value::Object(body),
                }),
                held: None,
            })
        }
        (EditKind::NewStack, s, "next") => {
            let at = form.desc.steps[form.step_index].clone();
            let names = strings(&edit(form).data["taken"]["names"]);
            let vmids: Vec<i64> = edit(form).data["taken"]["vmids"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
                .collect();
            let errors = check_new_step(&at, &form.values, &names, &vmids);
            if !errors.is_empty() {
                return hold(form, "next", errors);
            }
            form.errors.clear();
            form.step_index += 1;
            let next = form.desc.steps[form.step_index].id.clone();
            let _ = s;
            let effect = match next.as_str() {
                "data" => Effect::Edit(EditCall::Appdata {
                    body: json!({
                        "preset": text_of(form.values.get("preset")),
                        "name": text_of(form.values.get("name")).trim(),
                        "vmid": js_number(&text_of(form.values.get("vmid"))),
                    }),
                }),
                "plan" => {
                    edit(form).plan = None;
                    Effect::Edit(EditCall::NewPlan {
                        body: new_stack_body(&form.values),
                    })
                }
                _ => Effect::None,
            };
            Ok(Applied { effect, held: None })
        }
        (EditKind::HostSettings, "keys", "next") => {
            form.step_index = 1;
            Ok(plain())
        }
        (EditKind::HostSettings, "review", "confirm") => {
            let e = edit(form);
            let fields = e.data["page"]["fields"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let body = host_settings_body(
                e.data["page"]["sha256"].as_str().unwrap_or(""),
                &fields,
                &e.staged,
                &e.confirmed,
            );
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::HostWrite { body }),
                held: None,
            })
        }
        (EditKind::Batch, _, "confirm") => {
            let stacks = edit(form).stacks.clone();
            let action = edit(form).batch_action.clone().unwrap_or_default();
            let mut errors = BTreeMap::new();
            let fields: Vec<Field> = form.desc.fields().cloned().collect();
            for f in fields.iter().filter(|f| f.name.starts_with("confirm:")) {
                let s = &f.name["confirm:".len()..];
                if text_of(form.values.get(&f.name)).trim() != s {
                    errors.insert(f.name.clone(), say("batch_confirm", &[("stack", s)]));
                }
            }
            if !errors.is_empty() {
                return hold(form, "confirm", errors);
            }
            let mut args = Map::new();
            let mut confirms = Map::new();
            for f in &fields {
                let v = form.values.get(&f.name);
                if let Some(s) = f.name.strip_prefix("confirm:") {
                    let t = text_of(v).trim().to_string();
                    if !t.is_empty() {
                        confirms.insert(s.to_string(), json!(t));
                    }
                } else if f.kind == FieldKind::Check {
                    if v == Some(&Value::Bool(true)) {
                        args.insert(f.name.clone(), json!(true));
                    }
                } else {
                    let t = text_of(v).trim().to_string();
                    if !t.is_empty() {
                        args.insert(f.name.clone(), json!(t));
                    }
                }
            }
            let mut body = Map::new();
            body.insert("action".into(), json!(action));
            body.insert("stacks".into(), json!(stacks));
            body.insert("args".into(), Value::Object(args));
            if fields.iter().any(|f| f.name.starts_with("confirm:")) {
                body.insert("confirms".into(), Value::Object(confirms));
            }
            form.run_error = None;
            Ok(Applied {
                effect: Effect::Edit(EditCall::Batch {
                    body: Value::Object(body),
                }),
                held: None,
            })
        }
        (EditKind::Rollback, _, "next") => {
            let commit = text_of(form.values.get("commit"));
            let unit = text_of(form.values.get("unit"));
            let (action, preset) = if !commit.is_empty() {
                (ActionKind::DeployCommit, ("commit", commit))
            } else if !unit.is_empty() {
                (ActionKind::RollbackNative, ("unit", unit))
            } else {
                return Ok(held_msg("next", &say("rollback_choose", &[])));
            };
            if action.scope() > cx.scope {
                return Err(refused(
                    step,
                    format!("{} needs scope {:?}", action.slug(), action.scope()),
                    "leave it to Kenny",
                ));
            }
            let data = edit(form).data.clone();
            let sources = Sources {
                units: strings(&data["native_units"]),
                commits: data["commits"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["commit"].as_str().map(str::to_string))
                    .collect(),
                ..Sources::default()
            };
            let mut next = OpenForm::new(drive::action_form(action, &form.stack), &sources);
            next.values.insert(preset.0.into(), json!(preset.1));
            next.refresh();
            let effect = if next.step == drive::REVIEW {
                Effect::Preview
            } else {
                Effect::None
            };
            *form = next;
            Ok(Applied { effect, held: None })
        }
        _ => Err(refused(
            step,
            format!("{button} does nothing on the step {step_id}"),
            format!("the buttons on screen are: {}", form.buttons.join(", ")),
        )),
    }
}

/// The plan in short, for the tabs and `homelab ui state`.
pub fn plan_summary(p: &Value) -> Value {
    let files: Vec<Value> = p["files"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|f| {
            json!({ "path": f["path"], "status": f["status"], "added": f["added"], "removed": f["removed"] })
        })
        .collect();
    let effects: Vec<Value> = p["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| e["what"].clone())
        .collect();
    json!({
        "valid": p["valid"],
        "unchanged": p["unchanged"],
        "problems": p["problems"],
        "subject": p["subject"],
        "follow_ups": p["follow_ups"],
        "files": files,
        "effects": effects,
        "restarts_dashboard": p["restarts_dashboard"],
    })
}

/// What the shell's call answered, into the form.
pub fn done(form: &mut OpenForm, call: &EditCall, outcome: Result<Value, Refusal>) {
    match (call, outcome) {
        (
            EditCall::Plan { .. }
            | EditCall::NewPlan { .. }
            | EditCall::ImportPlan { .. }
            | EditCall::PresetPlan { .. },
            Ok(plan),
        ) => {
            let follow_ups = strings(&plan["follow_ups"]);
            let fields = commit_fields(&follow_ups, plan["subject"].as_str().unwrap_or(""));
            // A preset's own steps are the same `plan_steps()` shape a
            // stack edit's are (`[first, "plan", "commit"]`), so its plan
            // lands on "commit" the same way `EditCall::Plan`'s does.
            let at = if matches!(
                call,
                EditCall::Plan { .. } | EditCall::ImportPlan { .. } | EditCall::PresetPlan { .. }
            ) {
                "commit"
            } else {
                "plan"
            };
            for f in &fields {
                form.values.insert(f.name.clone(), drive::start_value(f));
            }
            if let Some(s) = form.desc.steps.iter_mut().find(|s| s.id == at) {
                s.fields = fields;
            }
            form.run_error = None;
            edit(form).plan = Some(plan_summary(&plan));
        }
        (
            EditCall::Plan { .. }
            | EditCall::NewPlan { .. }
            | EditCall::ImportPlan { .. }
            | EditCall::PresetPlan { .. },
            Err(r),
        ) => {
            edit(form).plan = None;
            form.run_error = Some(r);
        }
        (EditCall::Appdata { .. }, outcome) => {
            let paths = outcome.map(|v| strings(&v["appdata"])).unwrap_or_default();
            let fields = data_fields(&paths);
            form.values.retain(|k, _| {
                !k.starts_with("nodata:") || paths.iter().any(|p| k == &format!("nodata:{p}"))
            });
            for f in &fields {
                form.values
                    .entry(f.name.clone())
                    .or_insert(Value::Bool(false));
            }
            if let Some(s) = form.desc.steps.iter_mut().find(|s| s.id == "data") {
                s.fields = fields;
            }
        }
        (_, Ok(v)) => {
            form.run_error = None;
            edit(form).result = Some(v);
        }
        (_, Err(r)) => form.run_error = Some(r),
    }
    form.refresh();
}
