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
    pub commit: Vec<EditFieldDef>,
    pub raw: Vec<EditFieldDef>,
    pub add_app: Vec<EditFieldDef>,
    pub firewall: Vec<EditFieldDef>,
    pub rule: Vec<EditFieldDef>,
    pub new_stack: Vec<EditStepDef>,
    pub new_defaults: NewDefaults,
    pub nodata: EditFieldDef,
    pub host_key: HostKeyIds,
    pub rollback: Vec<EditFieldDef>,
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
}

impl EditKind {
    pub const ALL: [EditKind; 8] = [
        EditKind::Settings,
        EditKind::Raw,
        EditKind::AddApp,
        EditKind::Firewall,
        EditKind::NewStack,
        EditKind::HostSettings,
        EditKind::Batch,
        EditKind::Rollback,
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
            EditKind::Settings | EditKind::Raw | EditKind::AddApp | EditKind::Firewall
        )
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

/// `settingsBody`: only what differs from now.
pub fn settings_body(fields: &[Field], values: &Values) -> Value {
    let mut out = Map::new();
    out.insert("kind".into(), json!("settings"));
    let mut images = Map::new();
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
        } else if !t.is_empty() {
            out.insert(f.name.clone(), js_number(t));
        }
    }
    if !images.is_empty() {
        out.insert("images".into(), Value::Object(images));
    }
    Value::Object(out)
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
        EditKind::Settings | EditKind::Raw | EditKind::AddApp | EditKind::Firewall => {
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
                        if f.name == "file" {
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
                        } else {
                            f.current = Some(texts.get(&first).cloned().unwrap_or(json!("")));
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
                    let mut fields: Vec<Field> =
                        es().add_app.iter().map(EditFieldDef::field).collect();
                    for f in &mut fields {
                        f.choices = Some(
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
                        f.current = Some(presets[0]["name"].clone());
                    }
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
                _ => {
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
            let list: Vec<String> = target
                .unwrap_or("")
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect();
            if list.is_empty() {
                return Err(refused(
                    step,
                    "a batch needs its stacks",
                    format!("homelab ui open batch {action} <stack>,<stack>"),
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
            _ if kind.stack_edit() => match step.as_str() {
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
        (k, _, "next") if k.stack_edit() => {
            let fields: Vec<Field> = form.desc.steps[0].fields.clone();
            let body = match k {
                EditKind::Settings => {
                    let errors = check_fields(&fields, &form.values);
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
                    let file = text_of(form.values.get("file"));
                    let text = text_of(form.values.get("text"));
                    if edit(form).data["texts"][&file] == json!(text) {
                        return Ok(held_msg("next", &say("raw_unchanged", &[])));
                    }
                    json!({ "kind": "raw", "path": file, "content": text })
                }
                EditKind::AddApp => {
                    json!({ "kind": "add_app", "preset": text_of(form.values.get("preset")) })
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
        (EditCall::Plan { .. } | EditCall::NewPlan { .. }, Ok(plan)) => {
            let follow_ups = strings(&plan["follow_ups"]);
            let fields = commit_fields(&follow_ups, plan["subject"].as_str().unwrap_or(""));
            let at = if matches!(call, EditCall::Plan { .. }) {
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
        (EditCall::Plan { .. } | EditCall::NewPlan { .. }, Err(r)) => {
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
