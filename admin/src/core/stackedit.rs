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

use super::actions::{Refusal, valid_stack_name};
use super::yamledit::{self, EditError, Item, Op, Seg, path};

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
    /// feat-stacks-3: a preset's app added to this stack. feat-tiles-3:
    /// each app's own optional tile, keyed by app, applied to the SAME
    /// staged manifest as the app itself — one commit, not the app's
    /// commit followed by a second, best-effort `StackEdit::Tiles` one.
    AddApp {
        preset: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        tiles: BTreeMap<String, String>,
    },
    /// feat-stacks-files: create, delete or rename a file of the stack
    /// (editing an existing one's content stays `Raw`, above).
    Files(super::stackedit_files::FilesEdit),
    /// feat-checks-1: one app's whole `checks.yml`.
    Checks(super::stackedit_checks::ChecksEdit),
    /// feat-tiles-1: the stack's `tiles:` map, created/renamed/removed.
    Tiles(super::stackedit_tiles::TilesEdit),
    /// feat-stacks-9: network, lxc flags, resources.storage, on_demand,
    /// retention — the settings form's second page.
    SettingsExt(super::stackedit_settings_ext::SettingsExtEdit),
    /// feat-stacks-10: the Apps tab — remove/add-blank an app, storage,
    /// data_mounts, log_files.
    Apps(super::stackedit_apps::AppsEdit),
    /// feat-stacks-11: `latch_secrets` and `latch_files`.
    Latch(super::stackedit_latch::LatchEdit),
    /// feat-native-1: one native unit's `service.yml` fields.
    Native(super::stackedit_native::NativeEdit),
    /// feat-native-1: a new native unit — `service.yml`, its systemd unit
    /// and `natives:`.
    AddNative(super::stackedit_native::AddNativeEdit),
    /// feat-native-1: remove a native unit — its files and `natives:`.
    RemoveNative { unit: String },
    /// feat-publish-1: an app's hostname and port published through the
    /// gateway, and optionally a tile for it, in one commit.
    PublishApp(super::stackedit_publish::PublishAppEdit),
}

impl StackEdit {
    pub fn kind(&self) -> &'static str {
        match self {
            StackEdit::Settings(_) => "settings",
            StackEdit::Firewall(_) => "firewall",
            StackEdit::Raw { .. } => "raw",
            StackEdit::AddApp { .. } => "add_app",
            StackEdit::Files(_) => "files",
            StackEdit::Checks(_) => "checks",
            StackEdit::Tiles(_) => "tiles",
            StackEdit::SettingsExt(_) => "settings_ext",
            StackEdit::Apps(_) => "apps",
            StackEdit::Latch(_) => "latch",
            StackEdit::Native(_) => "native",
            StackEdit::AddNative(_) => "add_native",
            StackEdit::RemoveNative { .. } => "remove_native",
            StackEdit::PublishApp(_) => "publish_app",
        }
    }

    /// The feature a commit of this edit names (standing rule 4).
    pub fn feature(&self) -> &'static str {
        match self {
            StackEdit::Firewall(_) => "feat-firewall-1",
            StackEdit::AddApp { .. } => "feat-stacks-3",
            StackEdit::Files(_) => "feat-stacks-files",
            StackEdit::Checks(_) => "feat-checks-1",
            StackEdit::Tiles(_) => "feat-tiles-1",
            StackEdit::SettingsExt(_) => "feat-stacks-9",
            StackEdit::Apps(_) => "feat-stacks-10",
            StackEdit::Latch(_) => "feat-stacks-11",
            StackEdit::Native(_) | StackEdit::AddNative(_) | StackEdit::RemoveNative { .. } => {
                "feat-native-1"
            }
            StackEdit::PublishApp(_) => "feat-publish-1",
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
    /// owner remark 2026-09-30 ("de uptime-check tijd … in de wizard"): a
    /// tile's own watch seconds, keyed by its `tiles:` key. A field left
    /// out of a tile's edit is left as it is; blank in the browser means
    /// the fleet default (host.toml `watch_interval_s` /
    /// `watch_down_after_s`) applies.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tiles: BTreeMap<String, TileEdit>,
}

/// One tile's watch override (owner remark 2026-09-30). Both optional;
/// only the ones given are written.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TileEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub watch_every: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down_after: Option<u64>,
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

pub(crate) fn refusal(stack: &str, why: impl Into<String>, fix: impl Into<String>) -> Refusal {
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
                ));
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
    // owner remark 2026-09-30: a tile's own watch seconds, written into its
    // `tiles:` entry. `path()` splits on '.', which would break a tile key
    // that is itself a hostname (dots in it), so the path is built with
    // `Seg::Key` directly rather than through `path()`.
    for (key, edit) in &s.tiles {
        let Some(tile) = m.tiles.get(key) else {
            continue;
        };
        if let Some(w) = edit.watch_every.filter(|w| tile.watch_every != Some(*w)) {
            ops.push(Op::Set {
                path: vec![
                    Seg::Key("tiles".into()),
                    Seg::Key(key.clone()),
                    Seg::Key("watch_every".into()),
                ],
                value: Value::from(w),
            });
        }
        if let Some(d) = edit.down_after.filter(|d| tile.down_after != Some(*d)) {
            ops.push(Op::Set {
                path: vec![
                    Seg::Key("tiles".into()),
                    Seg::Key(key.clone()),
                    Seg::Key("down_after".into()),
                ],
                value: Value::from(d),
            });
        }
    }
    ops
}

/// Every range the settings form keeps to, as the manifest validator and
/// Proxmox would; checked before any text is touched.
pub fn settings_problems(s: &SettingsEdit) -> Vec<String> {
    let mut out = Vec::new();
    let range = |out: &mut Vec<String>, what: &str, v: Option<u64>, lo: u64, hi: u64| {
        if let Some(v) = v
            && !(lo..=hi).contains(&v)
        {
            out.push(format!("{what} must be from {lo} to {hi}, not {v}"));
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
    for (key, t) in &s.tiles {
        if let Some(w) = t.watch_every
            && w < 10
        {
            out.push(format!("{key}: check every must be at least 10 s, not {w}"));
        }
        if let Some(d) = t.down_after {
            if d < 10 {
                out.push(format!("{key}: down after must be at least 10 s, not {d}"));
            }
            if let Some(w) = t.watch_every
                && d < w
            {
                out.push(format!(
                    "{key}: down after ({d} s) must be at least check every ({w} s)"
                ));
            }
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
            for key in s.tiles.keys() {
                if !m.tiles.contains_key(key) {
                    return Err(refusal(
                        stack,
                        format!("{key} is not a tile of this stack"),
                        "pick a tile the form lists",
                    ));
                }
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
        StackEdit::AddApp { preset, tiles } => {
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
            // feat-tiles-3: a tile may only be given to an app this edit
            // itself adds, and its hostname must be free — applied to the
            // SAME staged manifest below, in the SAME commit.
            for app in tiles.keys() {
                if !files.apps.iter().any(|a| a == app) {
                    return Err(refusal(
                        stack,
                        format!("{app} is not one of this preset's apps"),
                        "pick an app the wizard lists",
                    ));
                }
            }
            let mut seen_hostnames = std::collections::BTreeSet::new();
            for hostname in tiles.values() {
                if m.tiles.contains_key(hostname) {
                    return Err(refusal(
                        stack,
                        format!("{hostname} is already a tile of this stack"),
                        "pick another hostname",
                    ));
                }
                if !seen_hostnames.insert(hostname.as_str()) {
                    return Err(refusal(
                        stack,
                        format!("{hostname} is used for two apps' tiles in this edit"),
                        "give each app its own hostname",
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
            // redesign-stacks-8: an empty stack (`apps: []`, `no_apps_yet`)
            // gets its list written whole, and stops waiting for apps.
            let mut ops = if m.apps.is_empty() {
                vec![Op::Set {
                    path: path("apps"),
                    value: Value::Sequence(
                        files.apps.iter().map(|a| Value::from(a.as_str())).collect(),
                    ),
                }]
            } else {
                vec![Op::Seq {
                    path: path("apps"),
                    items: apps,
                }]
            };
            if m.no_apps_yet {
                ops.push(Op::Remove {
                    path: path("no_apps_yet"),
                });
            }
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
            // `tiles:` may not be in the file yet; `set_tile_ops` only
            // needs to create it ahead of the first tile — checked once
            // against the original text, since every op here lands in the
            // same `yamledit::edit` call below (a second "create tiles: {}"
            // op, from a naive per-tile call, would wipe out the first
            // tile's own Set that already ran).
            let mut tiles_created = super::stackedit_tiles::has_tiles(manifest_text);
            for (app, hostname) in tiles {
                let fields = super::stackedit_tiles::TileFields {
                    name: app.clone(),
                    group: "Own".into(),
                    ..Default::default()
                };
                if tiles_created {
                    ops.push(Op::Set {
                        path: vec![Seg::Key("tiles".into()), Seg::Key(hostname.clone())],
                        value: super::stackedit_tiles::tile_value_for(&fields),
                    });
                } else {
                    ops.extend(super::stackedit_tiles::set_tile_ops(
                        manifest_text,
                        hostname,
                        &fields,
                    ));
                    tiles_created = true;
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
        StackEdit::Files(f) => {
            out.extend(super::stackedit_files::changes(stack, texts, f)?);
        }
        StackEdit::Checks(c) => {
            let problems = super::stackedit_checks::checks_problems(c);
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
            if !m.apps.contains(&c.app) {
                return Err(refusal(
                    stack,
                    format!("{} is not an app of this stack", c.app),
                    "pick an app the form lists",
                ));
            }
            let rel = format!("{}/checks.yml", c.app);
            match texts.get(&rel) {
                None => {
                    let new = serde_yaml::to_string(&super::stackedit_checks::checks_value(c))
                        .map_err(|e| refusal(stack, e.to_string(), "reload the checks editor"))?;
                    push(&rel, None, new);
                }
                Some(text) => {
                    let old: homelab_core::checks::ServiceChecks = serde_yaml::from_str(text)
                        .map_err(|e| {
                            refusal(
                                stack,
                                format!("stacks/{stack}/{rel} does not read: {e}"),
                                "fix it in the raw editor first",
                            )
                        })?;
                    let ops = super::stackedit_checks::checks_ops(&old, c).map_err(|why| {
                        refusal(stack, why, "reload the checks editor and redo the change")
                    })?;
                    if !ops.is_empty() {
                        let new = yamledit::edit(text, &ops)
                            .map_err(|e| from_edit_error(stack, &rel, e))?;
                        push(&rel, Some(text), new);
                    }
                }
            }
        }
        StackEdit::Tiles(t) => {
            let problems = super::stackedit_tiles::tiles_problems(t);
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
            let ops = super::stackedit_tiles::tiles_ops(&m, manifest_text, t).map_err(|why| {
                refusal(stack, why, "reload the tiles editor and redo the change")
            })?;
            if !ops.is_empty() {
                let new = yamledit::edit(manifest_text, &ops)
                    .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                push(MANIFEST, Some(manifest_text), new);
            }
        }
        StackEdit::SettingsExt(s) => {
            let problems = super::stackedit_settings_ext::problems(s);
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
            let ops = super::stackedit_settings_ext::ops(&m, s);
            if !ops.is_empty() {
                let new = yamledit::edit(manifest_text, &ops)
                    .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                push(MANIFEST, Some(manifest_text), new);
            }
        }
        StackEdit::Apps(a) => {
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            let problems = super::stackedit_apps::problems(&m, a);
            if !problems.is_empty() {
                return Err(refusal(
                    stack,
                    problems.join("; "),
                    "correct the values in the form",
                ));
            }
            let (ops, files) = super::stackedit_apps::apply(&m, texts, a)
                .map_err(|why| refusal(stack, why, "reload the apps editor and redo the change"))?;
            if !ops.is_empty() {
                let new = yamledit::edit(manifest_text, &ops)
                    .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                push(MANIFEST, Some(manifest_text), new);
            }
            for f in &files {
                out.push(FileChange {
                    path: full(&f.rel),
                    old: texts.get(&f.rel).cloned(),
                    new: f.content.clone(),
                });
            }
        }
        StackEdit::Latch(l) => {
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            let problems = super::stackedit_latch::problems(&m, l);
            if !problems.is_empty() {
                return Err(refusal(
                    stack,
                    problems.join("; "),
                    "correct the values in the form",
                ));
            }
            let ops = super::stackedit_latch::ops(manifest_text, l);
            if !ops.is_empty() {
                let new = yamledit::edit(manifest_text, &ops)
                    .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                push(MANIFEST, Some(manifest_text), new);
            }
        }
        StackEdit::Native(n) => {
            let Some(p) = super::stackedit_native::native_path(texts, &n.unit) else {
                return Err(refusal(
                    stack,
                    format!("{} is not a native unit of {stack}", n.unit),
                    "pick a unit the form lists",
                ));
            };
            let text = texts.get(&p).expect("native_path found it in texts");
            let m: homelab_proto::NativeServiceManifest =
                serde_yaml::from_str(text).map_err(|e| {
                    refusal(
                        stack,
                        format!("stacks/{stack}/{p} does not read: {e}"),
                        "fix it in the raw editor first",
                    )
                })?;
            let ops = super::stackedit_native::native_ops(&m, n);
            if !ops.is_empty() {
                let new = yamledit::edit(text, &ops).map_err(|e| from_edit_error(stack, &p, e))?;
                push(&p, Some(text), new);
            }
        }
        StackEdit::AddNative(a) => {
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            let unit = a.unit.trim();
            if unit.is_empty() {
                return Err(refusal(
                    stack,
                    "the unit needs a name",
                    "name the systemd unit",
                ));
            }
            if m.natives.iter().any(|u| u == unit)
                || texts.contains_key(&format!("{unit}/service.yml"))
            {
                return Err(refusal(
                    stack,
                    format!("{stack} already has a native unit called {unit}"),
                    "pick another unit name",
                ));
            }
            let (yml, unit_file) = super::stackedit_native::add_native_files(&m, a);
            push(&format!("{unit}/service.yml"), None, yml);
            push(&format!("{unit}/{unit}.service"), None, unit_file);
            let mut items: Vec<Item> = (0..m.natives.len()).map(Item::Keep).collect();
            items.push(Item::New(Value::from(unit)));
            let new = yamledit::edit(
                manifest_text,
                &[Op::Seq {
                    path: path("natives"),
                    items,
                }],
            )
            .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
            push(MANIFEST, Some(manifest_text), new);
        }
        StackEdit::RemoveNative { unit } => {
            let m = parse_manifest(manifest_text).map_err(|e| {
                refusal(
                    stack,
                    format!("stacks/{stack}/{MANIFEST} does not read: {e}"),
                    "fix it in the raw editor first",
                )
            })?;
            let Some(idx) = m.natives.iter().position(|u| u == unit) else {
                return Err(refusal(
                    stack,
                    format!("{unit} is not a native unit of {stack}"),
                    "pick a unit the form lists",
                ));
            };
            let Some(p) = super::stackedit_native::native_path(texts, unit) else {
                return Err(refusal(
                    stack,
                    format!("{unit}'s service.yml could not be found"),
                    "use the raw editor",
                ));
            };
            let items: Vec<Item> = (0..m.natives.len())
                .filter(|&i| i != idx)
                .map(Item::Keep)
                .collect();
            let new = yamledit::edit(
                manifest_text,
                &[Op::Seq {
                    path: path("natives"),
                    items,
                }],
            )
            .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
            push(MANIFEST, Some(manifest_text), new);
            out.push(FileChange {
                path: full(&p),
                old: texts.get(&p).cloned(),
                new: None,
            });
            let uf = super::stackedit_native::unit_file_path(unit);
            if let Some(t) = texts.get(&uf) {
                out.push(FileChange {
                    path: full(&uf),
                    old: Some(t.clone()),
                    new: None,
                });
            }
        }
        StackEdit::PublishApp(p) => {
            let problems = super::stackedit_publish::publish_problems(p);
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
            if !m.apps.contains(&p.app) {
                return Err(refusal(
                    stack,
                    format!("{} is not an app of this stack", p.app),
                    "pick an app the form lists",
                ));
            }
            let gateway_vmid = homelab_core::safety::SafetyConfig::default().gateway_vmid;
            let (has_gateway_route, extra_route_count) =
                super::stackedit_publish::gateway_route_state(manifest_text);
            let write = super::stackedit_publish::plan_gateway_write(
                &m,
                gateway_vmid,
                stack,
                p,
                has_gateway_route,
                extra_route_count,
            );
            match write {
                super::stackedit_publish::GatewayWrite::Primary {
                    filename,
                    route_file,
                } => {
                    let ip = super::stackedit_publish::container_ip(&m);
                    let backend = format!("http://{ip}:{}", p.port);
                    let op = super::stackedit_publish::primary_op(
                        gateway_vmid,
                        &filename,
                        p.external,
                        &backend,
                    );
                    let new = yamledit::edit(manifest_text, std::slice::from_ref(&op))
                        .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                    push(MANIFEST, Some(manifest_text), new);
                    push(
                        "traefik-routes.yml",
                        texts.get("traefik-routes.yml"),
                        route_file,
                    );
                }
                super::stackedit_publish::GatewayWrite::Extend { ops } => {
                    let existing = texts.get("traefik-routes.yml").ok_or_else(|| {
                        refusal(
                            stack,
                            "gateway_route is set but traefik-routes.yml is missing",
                            "use the raw editor to restore it first",
                        )
                    })?;
                    let new = yamledit::edit(existing, &ops)
                        .map_err(|e| from_edit_error(stack, "traefik-routes.yml", e))?;
                    push("traefik-routes.yml", Some(existing), new);
                }
                super::stackedit_publish::GatewayWrite::Extra {
                    filename,
                    route_file,
                    ops,
                } => {
                    let new = yamledit::edit(manifest_text, &ops)
                        .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                    push(MANIFEST, Some(manifest_text), new);
                    let rel = format!("routes/{filename}");
                    push(&rel, texts.get(&rel), route_file);
                }
            }
            if let Some(tile) = &p.tile {
                let key = p.hostname.clone();
                if !m.tiles.contains_key(&key) {
                    let base = out
                        .iter()
                        .find(|c| c.path == full(MANIFEST))
                        .and_then(|c| c.new.clone())
                        .unwrap_or_else(|| manifest_text.to_string());
                    let ops = super::stackedit_tiles::set_tile_ops(&base, &key, tile);
                    let new = yamledit::edit(&base, &ops)
                        .map_err(|e| from_edit_error(stack, MANIFEST, e))?;
                    out.retain(|c| c.path != full(MANIFEST));
                    out.push(FileChange {
                        path: full(MANIFEST),
                        old: Some(manifest_text.to_string()),
                        new: Some(new),
                    });
                }
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
            for (key, t) in &s.tiles {
                if let Some(w) = t.watch_every {
                    parts.push(format!("{key} checked every {w}s"));
                }
                if let Some(d) = t.down_after {
                    parts.push(format!("{key} down after {d}s"));
                }
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
        StackEdit::AddApp { preset, tiles } => {
            if tiles.is_empty() {
                format!("add the {preset} preset's app")
            } else {
                format!(
                    "add the {preset} preset's app, with {} tile(s)",
                    tiles.len()
                )
            }
        }
        StackEdit::Files(f) => f.describe(),
        StackEdit::Checks(c) => c.describe(),
        StackEdit::Tiles(t) => t.describe(),
        StackEdit::SettingsExt(s) => {
            let Some(m) = old else { return String::new() };
            super::stackedit_settings_ext::describe(s, m).join(", ")
        }
        StackEdit::Apps(a) => super::stackedit_apps::describe(a).join(", "),
        StackEdit::Latch(l) => super::stackedit_latch::describe(l).join(", "),
        StackEdit::Native(n) => format!("{}: service.yml edited", n.unit),
        StackEdit::AddNative(a) => format!("add the native unit {}", a.unit),
        StackEdit::RemoveNative { unit } => format!("remove the native unit {unit}"),
        StackEdit::PublishApp(p) => format!(
            "publish {} at {}{}",
            p.app,
            p.hostname,
            if p.tile.is_some() { " with a tile" } else { "" }
        ),
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
