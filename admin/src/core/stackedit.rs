//! feat-stacks-2, feat-firewall-1, feat-stacks-3: what the browser asks to
//! change in a stack's files, and the exact new file texts that follow. The
//! texts are written by the comment-keeping editor (`yamledit`); the shell
//! then validates them with homelab-core, shows the plan and commits them.
//! Pure: the current texts come in, the new texts go out.

use std::collections::BTreeMap;

use homelab_proto::StackManifest;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use homelab_core::manifest::{FirewallRule, FirewallSpec, FwAction, FwDir, FwProto};

use super::actions::{valid_stack_name, Refusal};
use super::yamledit::{self, path, EditError, Item, Op};

/// The stack file every stack has.
pub const MANIFEST: &str = "lxc-compose.yml";

/// Largest file the raw editor takes.
pub const RAW_MAX: usize = 512 * 1024;

/// What the browser sends to `…/plan` and `…/commit`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum StackEdit {
    /// feat-stacks-2: the stack's settings form.
    Settings(SettingsEdit),
    /// feat-firewall-1: the whole firewall as it should be.
    Firewall(FirewallEdit),
    /// feat-stacks-2: one file of the stack, typed in whole.
    Raw { path: String, content: String },
    /// feat-stacks-3: a preset's app added to this stack.
    AddApp { preset: String },
}

impl StackEdit {
    pub fn kind(&self) -> &'static str {
        match self {
            StackEdit::Settings(_) => "settings",
            StackEdit::Firewall(_) => "firewall",
            StackEdit::Raw { .. } => "raw",
            StackEdit::AddApp { .. } => "add_app",
        }
    }

    /// The feature a commit of this edit names (standing rule 4).
    pub fn feature(&self) -> &'static str {
        match self {
            StackEdit::Firewall(_) => "feat-firewall-1",
            StackEdit::AddApp { .. } => "feat-stacks-3",
            _ => "feat-stacks-2",
        }
    }
}

/// feat-stacks-2: the settings form. A field left out is left as it is.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cores: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_mb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_gb: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub onboot: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protection: Option<bool>,
    /// `<app>/<service>` → image, e.g. `kp-soft/kp-soft` →
    /// `ghcr.io/kennypassenier/kp-soft:1.4.0`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub images: BTreeMap<String, String>,
}

/// feat-firewall-1: the firewall as the editor holds it.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FirewallEdit {
    pub enabled: bool,
    #[serde(default)]
    pub comment: Option<String>,
    pub policy_in: FwAction,
    pub policy_out: FwAction,
    #[serde(default)]
    pub management_open: Option<String>,
    pub rules: Vec<RuleEdit>,
}

/// One rule, and which rule of the file it was (None: new). A rule that is
/// unchanged keeps its text and its comments exactly.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuleEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    pub rule: FirewallRule,
}

/// A file's text before and after; None = the file does not exist on that
/// side. `path` is relative to the repository (`stacks/<stack>/…`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileChange {
    pub path: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// The stack's files as read from the working copy: path relative to the
/// stack's directory → text.
pub type StackTexts = BTreeMap<String, String>;

fn refusal(stack: &str, why: impl Into<String>, fix: impl Into<String>) -> Refusal {
    Refusal::new(format!("the edit of {stack}"), why, fix)
}

fn from_edit_error(stack: &str, file: &str, e: EditError) -> Refusal {
    let fix = match e {
        EditError::Parse(_) => format!("fix stacks/{stack}/{file} first (the raw editor opens it)"),
        _ => format!("make this change in the raw editor of stacks/{stack}/{file}"),
    };
    refusal(stack, format!("stacks/{stack}/{file}: {e}"), fix)
}

/// The parsed manifest of a stack file's text (the fields a plan needs;
/// the shell validates the whole file with the client's own parser).
pub fn parse_manifest(text: &str) -> Result<StackManifest, String> {
    // The manifest ignores the stack file's own keys (routes, latch).
    serde_yaml::from_str::<StackManifest>(text).map_err(|e| e.to_string())
}

/// `<app>/<service>` → image, for every app's compose file.
pub fn images(texts: &StackTexts) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (p, text) in texts {
        let Some(app) = p.strip_suffix("/docker-compose.yml") else {
            continue;
        };
        if app.contains('/') {
            continue;
        }
        let Ok(v) = serde_yaml::from_str::<Value>(text) else {
            continue;
        };
        if let Some(Value::Mapping(services)) = v.get("services") {
            for (name, s) in services {
                if let (Some(name), Some(image)) =
                    (name.as_str(), s.get("image").and_then(Value::as_str))
                {
                    out.insert(format!("{app}/{name}"), image.to_string());
                }
            }
        }
    }
    out
}

/// A docker image reference: `name[:tag][@sha256:…]`, no spaces.
pub fn valid_image(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 255
        && !s.starts_with(['-', ':', '@', '/'])
        && s.bytes().all(|b| {
            b.is_ascii_lowercase()
                || b.is_ascii_uppercase()
                || b.is_ascii_digit()
                || matches!(b, b'.' | b'-' | b'_' | b'/' | b':' | b'@')
        })
}

/// A path the raw editor may write: inside the stack's directory, a file
/// the stack already has, never a secret.
pub fn raw_path_problem(path: &str, texts: &StackTexts) -> Option<String> {
    if path.is_empty()
        || path.starts_with('/')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Some(format!("{path:?} is not a path inside the stack"));
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == ".env" || name.ends_with(".env") || name.starts_with(".env") {
        return Some(format!(
            "{path} holds secrets; secrets are changed through latch, never in the repository"
        ));
    }
    if !texts.contains_key(path) {
        return Some(format!(
            "{path} is not a file of this stack; the raw editor changes existing files"
        ));
    }
    None
}

/// A firewall rule as the file writes it: the comment first, then the
/// fields in the order Proxmox reads them, nothing that is unset.
pub fn rule_value(r: &FirewallRule) -> Value {
    let mut m = Mapping::new();
    let mut put = |k: &str, v: Option<Value>| {
        if let Some(v) = v {
            m.insert(Value::from(k), v);
        }
    };
    put("comment", r.comment.clone().map(Value::from));
    put(
        "dir",
        Some(Value::from(match r.dir {
            FwDir::In => "in",
            FwDir::Out => "out",
        })),
    );
    put("action", Some(Value::from(action_word(r.action))));
    put("source", r.source.clone().map(Value::from));
    put("dest", r.dest.clone().map(Value::from));
    put(
        "proto",
        r.proto.map(|p| {
            Value::from(match p {
                FwProto::Tcp => "tcp",
                FwProto::Udp => "udp",
                FwProto::Icmp => "icmp",
            })
        }),
    );
    put("dport", r.dport.clone().map(Value::from));
    put("note", r.note.clone().map(Value::from));
    Value::Mapping(m)
}

pub fn action_word(a: FwAction) -> &'static str {
    match a {
        FwAction::Accept => "ACCEPT",
        FwAction::Drop => "DROP",
        FwAction::Reject => "REJECT",
    }
}

/// Trim a rule's text fields; an empty one is unset.
pub fn tidy_rule(mut r: FirewallRule) -> FirewallRule {
    let t = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    r.source = t(r.source);
    r.dest = t(r.dest);
    r.dport = t(r.dport).map(|s| s.replace(' ', ""));
    r.note = t(r.note);
    r.comment = r
        .comment
        .map(|s| s.trim_end().to_string())
        .filter(|s| !s.trim().is_empty());
    r
}

fn firewall_value(f: &FirewallEdit) -> Value {
    let mut m = Mapping::new();
    m.insert("enabled".into(), Value::from(f.enabled));
    if let Some(c) = &f.comment {
        m.insert("comment".into(), Value::from(c.as_str()));
    }
    m.insert("policy_in".into(), Value::from(action_word(f.policy_in)));
    m.insert("policy_out".into(), Value::from(action_word(f.policy_out)));
    if let Some(r) = &f.management_open {
        m.insert("management_open".into(), Value::from(r.as_str()));
    }
    m.insert(
        "rules".into(),
        Value::Sequence(
            f.rules
                .iter()
                .map(|r| rule_value(&tidy_rule(r.rule.clone())))
                .collect(),
        ),
    );
    Value::Mapping(m)
}

/// The ops that turn the file's firewall into `want`.
pub fn firewall_ops(old: Option<&FirewallSpec>, want: &FirewallEdit) -> Result<Vec<Op>, String> {
    let Some(old) = old else {
        return Ok(vec![Op::Set {
            path: path("firewall"),
            value: firewall_value(want),
        }]);
    };
    let mut ops = Vec::new();
    let set = |ops: &mut Vec<Op>, k: &str, v: Value| {
        ops.push(Op::Set {
            path: path(&format!("firewall.{k}")),
            value: v,
        })
    };
    if old.enabled != want.enabled {
        set(&mut ops, "enabled", Value::from(want.enabled));
    }
    if old.policy_in != want.policy_in {
        set(
            &mut ops,
            "policy_in",
            Value::from(action_word(want.policy_in)),
        );
    }
    if old.policy_out != want.policy_out {
        set(
            &mut ops,
            "policy_out",
            Value::from(action_word(want.policy_out)),
        );
    }
    let text = |v: &Option<String>| {
        v.as_deref()
            .map(str::trim_end)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    };
    for (k, before, after) in [
        ("comment", text(&old.comment), text(&want.comment)),
        (
            "management_open",
            text(&old.management_open),
            text(&want.management_open),
        ),
    ] {
        if before != after {
            match after {
                Some(v) => set(&mut ops, k, Value::from(v)),
                None => ops.push(Op::Remove {
                    path: path(&format!("firewall.{k}")),
                }),
            }
        }
    }
    let mut used = std::collections::BTreeSet::new();
    let mut items = Vec::new();
    for r in &want.rules {
        let rule = tidy_rule(r.rule.clone());
        match r.origin {
            Some(i) if i >= old.rules.len() => {
                return Err(format!(
                    "rule {} of the edit points at rule {} of the file, which has {}",
                    items.len() + 1,
                    i + 1,
                    old.rules.len()
                ))
            }
            Some(i) if used.insert(i) => {
                if old.rules[i] == rule {
                    items.push(Item::Keep(i));
                } else {
                    items.push(Item::Retext(i, rule_value(&rule)));
                }
            }
            _ => items.push(Item::New(rule_value(&rule))),
        }
    }
    let unchanged = items.len() == old.rules.len()
        && items
            .iter()
            .enumerate()
            .all(|(n, it)| matches!(it, Item::Keep(i) if *i == n));
    if !unchanged {
        ops.push(Op::Seq {
            path: path("firewall.rules"),
            items,
        });
    }
    Ok(ops)
}

/// The settings form's ops on the stack file.
fn settings_ops(m: &StackManifest, s: &SettingsEdit) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut num = |key: &str, now: u64, want: Option<u64>| {
        if let Some(w) = want.filter(|w| *w != now) {
            ops.push(Op::Set {
                path: path(key),
                value: Value::from(w),
            });
        }
    };
    num(
        "resources.cores",
        m.resources.cores.into(),
        s.cores.map(u64::from),
    );
    num(
        "resources.memory_mb",
        m.resources.memory_mb.into(),
        s.memory_mb.map(u64::from),
    );
    num(
        "resources.swap_mb",
        m.resources.swap_mb.into(),
        s.swap_mb.map(u64::from),
    );
    num(
        "resources.disk_gb",
        m.resources.disk_gb.into(),
        s.disk_gb.map(u64::from),
    );
    if let Some(o) = s.order.filter(|o| m.boot.order != Some(*o)) {
        ops.push(Op::Set {
            path: path("boot.order"),
            value: Value::from(o),
        });
    }
    if let Some(b) = s.onboot.filter(|b| *b != m.boot.onboot) {
        ops.push(Op::Set {
            path: path("boot.onboot"),
            value: Value::from(b),
        });
    }
    if let Some(b) = s.protection.filter(|b| *b != m.lxc.protection) {
        ops.push(Op::Set {
            path: path("lxc.protection"),
            value: Value::from(b),
        });
    }
    ops
}

/// Every range the settings form keeps to, as the manifest validator and
/// Proxmox would; checked before any text is touched.
pub fn settings_problems(s: &SettingsEdit) -> Vec<String> {
    let mut out = Vec::new();
    let range = |out: &mut Vec<String>, what: &str, v: Option<u64>, lo: u64, hi: u64| {
        if let Some(v) = v {
            if !(lo..=hi).contains(&v) {
                out.push(format!("{what} must be from {lo} to {hi}, not {v}"));
            }
        }
    };
    range(&mut out, "cores", s.cores.map(u64::from), 1, 64);
    range(&mut out, "memory", s.memory_mb.map(u64::from), 128, 262_144);
    range(&mut out, "swap", s.swap_mb.map(u64::from), 0, 65_536);
    range(&mut out, "disk", s.disk_gb.map(u64::from), 2, 4096);
    range(&mut out, "boot order", s.order.map(u64::from), 0, 9999);
    for (k, image) in &s.images {
        if !valid_image(image) {
            out.push(format!(
                "{k}: {image:?} is not an image reference like name:tag"
            ));
        }
    }
    out
}

/// The new texts for one edit (only the files that change).
pub fn changes(
    stack: &str,
    texts: &StackTexts,
    edit: &StackEdit,
    add_app: Option<&AddAppFiles>,
) -> Result<Vec<FileChange>, Refusal> {
    if !valid_stack_name(stack) {
        return Err(refusal(
            stack,
            "not a stack name",
            "use the name the fleet page shows",
        ));
    }
    let manifest_text = texts.get(MANIFEST).ok_or_else(|| {
        refusal(
            stack,
            format!("stacks/{stack} has no {MANIFEST}"),
            "a native-only stack described by service.yml alone is edited in the raw editor",
        )
    })?;
    let full = |rel: &str| format!("stacks/{stack}/{rel}");
    let mut out: Vec<FileChange> = Vec::new();
    let mut push = |rel: &str, old: Option<&String>, new: String| {
        if old != Some(&new) {
            out.push(FileChange {
                path: full(rel),
                old: old.cloned(),
                new: Some(new),
            });
        }
    };
    match edit {
        StackEdit::Raw { path: p, content } => {
            if let Some(why) = raw_path_problem(p, texts) {
                return Err(refusal(
                    stack,
                    why,
                    "pick one of the files the editor lists",
                ));
            }
            if content.len() > RAW_MAX {
                return Err(refusal(
                    stack,
                    format!(
                        "{p} would be {} bytes; the editor takes at most {RAW_MAX}",
                        content.len()
                    ),
                    "change a file that large in a workstation clone",
                ));
            }
            push(p, texts.get(p), content.clone());
        }
        StackEdit::Settings(s) => {
            let problems = settings_problems(s);
            if !problems.is_empty() {
                return Err(refusal(
                    stack,
                    problems.join("; "),
                    "correct the values in the form",
                ));
            }
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            let ops = settings_ops(&m, s);
            if !ops.is_empty() {
                let new = yamledit::edit(manifest_text, &ops)
                    .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                push(MANIFEST, Some(manifest_text), new);
            }
            let now = images(texts);
            for (key, image) in &s.images {
                let Some(current) = now.get(key) else {
                    return Err(refusal(
                        stack,
                        format!("{key} is not a service of this stack"),
                        "pick a service the form lists",
                    ));
                };
                if current == image {
                    continue;
                }
                let (app, service) = key.split_once('/').unwrap_or((key, key));
                let rel = format!("{app}/docker-compose.yml");
                let base = out
                    .iter()
                    .find(|c| c.path == full(&rel))
                    .and_then(|c| c.new.clone())
                    .or_else(|| texts.get(&rel).cloned())
                    .unwrap_or_default();
                let new = yamledit::edit(
                    &base,
                    &[Op::Set {
                        path: vec![
                            yamledit::Seg::Key("services".into()),
                            yamledit::Seg::Key(service.into()),
                            yamledit::Seg::Key("image".into()),
                        ],
                        value: Value::from(image.as_str()),
                    }],
                )
                .map_err(|e| from_edit_error(stack, &rel, e))?;
                out.retain(|c| c.path != full(&rel));
                out.push(FileChange {
                    path: full(&rel),
                    old: texts.get(&rel).cloned(),
                    new: Some(new),
                });
            }
        }
        StackEdit::Firewall(f) => {
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            let ops = firewall_ops(m.firewall.as_ref(), f).map_err(|why| {
                refusal(stack, why, "reload the firewall editor and redo the change")
            })?;
            if !ops.is_empty() {
                let new = yamledit::edit(manifest_text, &ops)
                    .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                push(MANIFEST, Some(manifest_text), new);
            }
        }
        StackEdit::AddApp { preset } => {
            let files = add_app.ok_or_else(|| {
                refusal(
                    stack,
                    format!("the preset {preset:?} was not read"),
                    "pick a preset the wizard lists",
                )
            })?;
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            for app in &files.apps {
                if m.apps.contains(app) || texts.keys().any(|k| k.starts_with(&format!("{app}/"))) {
                    return Err(refusal(
                        stack,
                        format!("{stack} already has an app called {app}"),
                        "pick another preset, or edit the existing app",
                    ));
                }
            }
            let mut apps: Vec<Item> = (0..m.apps.len()).map(Item::Keep).collect();
            apps.extend(
                files
                    .apps
                    .iter()
                    .map(|a| Item::New(Value::from(a.as_str()))),
            );
            let mut ops = vec![Op::Seq {
                path: path("apps"),
                items: apps,
            }];
            let declared: Vec<&str> = m.storage.iter().map(|s| s.host_path.as_str()).collect();
            let new_mounts: Vec<Value> = files
                .appdata
                .iter()
                .filter(|p| !declared.contains(&p.as_str()))
                .map(|p| mount_value(stack, p, files.owner_uid))
                .collect();
            if !new_mounts.is_empty() {
                if m.storage.is_empty() && !manifest_text.lines().any(|l| l.starts_with("storage:"))
                {
                    ops.push(Op::Set {
                        path: path("storage"),
                        value: Value::Sequence(new_mounts),
                    });
                } else {
                    let mut items: Vec<Item> = (0..m.storage.len()).map(Item::Keep).collect();
                    items.extend(new_mounts.into_iter().map(Item::New));
                    ops.push(Op::Seq {
                        path: path("storage"),
                        items,
                    });
                }
            }
            let new = yamledit::edit(manifest_text, &ops)
                .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
            push(MANIFEST, Some(manifest_text), new);
            for (rel, content) in &files.files {
                out.push(FileChange {
                    path: full(rel),
                    old: None,
                    new: Some(content.clone()),
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// feat-stacks-3: a preset's apps as they would land in a stack: the files
/// (paths relative to the stack's directory, placeholders filled in), the
/// app names, and the `/appdata` paths their compose files bind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AddAppFiles {
    pub apps: Vec<String>,
    pub files: BTreeMap<String, String>,
    pub appdata: Vec<String>,
    pub owner_uid: u32,
}

/// A storage entry for an `/appdata` path, owned by its app when the path
/// is `/appdata/<stack>/<app>-config` (D25).
pub fn mount_value(stack: &str, host_path: &str, owner_uid: u32) -> Value {
    let mut m = Mapping::new();
    m.insert("host_path".into(), Value::from(host_path));
    m.insert("mount_point".into(), Value::from(host_path));
    m.insert("host_owner_uid".into(), Value::from(owner_uid));
    let app = host_path
        .strip_prefix(&format!("/appdata/{stack}/"))
        .and_then(|rest| rest.strip_suffix("-config"))
        .filter(|a| !a.is_empty() && !a.contains('/'));
    if let Some(app) = app {
        m.insert("app".into(), Value::from(app));
    }
    Value::Mapping(m)
}

/// Every path a set of changes touches outside `stacks/<stack>/`; the
/// commit refuses when this is not empty (arch-push-credential).
pub fn outside_stack<'a>(stack: &str, paths: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let prefix = format!("stacks/{stack}/");
    paths
        .into_iter()
        .filter(|p| {
            !p.starts_with(&prefix)
                || p.contains("/../")
                || p.ends_with("/..")
                || p.len() == prefix.len()
        })
        .map(str::to_string)
        .collect()
}

/// The change in a few words, for the commit subject: what the form
/// changed, or how the firewall's rules moved. Empty when there is nothing
/// to say beyond the file names.
pub fn describe(edit: &StackEdit, old: Option<&StackManifest>) -> String {
    match edit {
        StackEdit::Settings(s) => {
            let Some(m) = old else { return String::new() };
            let mut parts = Vec::new();
            let mb = |v: u32| {
                if v >= 1024 && v.is_multiple_of(1024) {
                    format!("{} GB", v / 1024)
                } else {
                    format!("{v} MB")
                }
            };
            if let Some(v) = s.cores.filter(|v| *v != m.resources.cores) {
                parts.push(format!("{v} cores"));
            }
            if let Some(v) = s.memory_mb.filter(|v| *v != m.resources.memory_mb) {
                parts.push(format!("memory {}", mb(v)));
            }
            if let Some(v) = s.swap_mb.filter(|v| *v != m.resources.swap_mb) {
                parts.push(format!("swap {}", mb(v)));
            }
            if let Some(v) = s.disk_gb.filter(|v| *v != m.resources.disk_gb) {
                parts.push(format!("disk {v} GB"));
            }
            if let Some(v) = s.onboot.filter(|v| *v != m.boot.onboot) {
                parts.push(format!("start on boot {}", if v { "on" } else { "off" }));
            }
            if let Some(v) = s.order.filter(|v| m.boot.order != Some(*v)) {
                parts.push(format!("boot order {v}"));
            }
            if let Some(v) = s.protection.filter(|v| *v != m.lxc.protection) {
                parts.push(format!("protection {}", if v { "on" } else { "off" }));
            }
            for (k, image) in &s.images {
                parts.push(format!("{k} → {image}"));
            }
            parts.join(", ")
        }
        StackEdit::Firewall(f) => {
            let old_rules = old
                .and_then(|m| m.firewall.as_ref())
                .map(|f| f.rules.clone());
            let Some(old_rules) = old_rules else {
                return format!("firewall declared with {} rule(s)", f.rules.len());
            };
            let kept: std::collections::BTreeSet<usize> =
                f.rules.iter().filter_map(|r| r.origin).collect();
            let added: Vec<&RuleEdit> = f
                .rules
                .iter()
                .filter(|r| r.origin.is_none_or(|i| i >= old_rules.len()))
                .collect();
            let removed = (0..old_rules.len()).filter(|i| !kept.contains(i)).count();
            let changed = f
                .rules
                .iter()
                .filter(|r| {
                    r.origin.is_some_and(|i| {
                        old_rules
                            .get(i)
                            .is_some_and(|o| *o != tidy_rule(r.rule.clone()))
                    })
                })
                .count();
            let mut parts = Vec::new();
            match added.as_slice() {
                [] => {}
                [one] => parts.push(format!("add {}", rule_words(&tidy_rule(one.rule.clone())))),
                many => parts.push(format!("add {} rules", many.len())),
            }
            if removed > 0 {
                parts.push(format!("remove {removed} rule(s)"));
            }
            if changed > 0 {
                parts.push(format!("change {changed} rule(s)"));
            }
            if parts.is_empty() {
                parts.push("options or rule order".into());
            }
            format!("firewall: {}", parts.join(", "))
        }
        StackEdit::Raw { path, .. } => format!("{path} edited in the dashboard"),
        StackEdit::AddApp { preset } => format!("add the {preset} preset's app"),
    }
}

/// `IN ACCEPT from 10.10.10.10 tcp 8090`.
pub fn rule_words(r: &FirewallRule) -> String {
    let (dir, peer, word) = match r.dir {
        FwDir::In => ("IN", r.source.as_deref(), "from"),
        FwDir::Out => ("OUT", r.dest.as_deref(), "to"),
    };
    let mut s = format!(
        "{dir} {} {word} {}",
        action_word(r.action),
        peer.unwrap_or("anywhere")
    );
    if let Some(p) = r.proto {
        s.push_str(match p {
            FwProto::Tcp => " tcp",
            FwProto::Udp => " udp",
            FwProto::Icmp => " icmp",
        });
    }
    if let Some(d) = &r.dport {
        s.push(' ');
        s.push_str(d);
    }
    s
}
