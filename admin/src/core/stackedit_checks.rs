//! Checks form (dashboard gap: the stack page's Checks tab only read
//! `checks.yml`; everything in it had to be typed by hand). One app's whole
//! `checks.yml` (`homelab_core::checks::ServiceChecks`) as the browser edits
//! it: the checks, the manual list, the probes, the busy check and the link.
//!
//! The file sits beside its app the same way `docker-compose.yml` does
//! (`<app>/checks.yml`), so an app that has never had one yet is created by
//! this form rather than edited — `stackedit::changes` tells the two apart
//! by whether the path is already in the stack's texts.
//!
//! Existing items are matched back to the file by `origin` (the
//! rule-editing pattern `stackedit::RuleEdit` already uses for the
//! firewall), so a check that is sent back unchanged keeps its own text and
//! comments; only a changed, removed or new one touches the file.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use homelab_core::checks::{BusyCheck, Check, Layer, ManualCheck, Probe, ServiceChecks};

use super::stackedit::StackTexts;
use super::yamledit::{Item, Op, path};

/// The browser's read side (`GET …/edit`): each of the stack's apps, and
/// its `checks.yml` as parsed JSON — an app with no `checks.yml` yet reads
/// as an empty, ready-to-fill one, so the form always has something to
/// show. An app whose file does not parse reads as `Unreadable`, the same
/// "fix it in the raw editor first" fallback the settings and firewall
/// forms already use for a broken `lxc-compose.yml`.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ChecksView {
    Read {
        checks: Vec<Check>,
        manual: Vec<ManualCheck>,
        probes: Vec<Probe>,
        #[serde(skip_serializing_if = "Option::is_none")]
        busy_check: Option<BusyCheck>,
        #[serde(skip_serializing_if = "Option::is_none")]
        url: Option<String>,
    },
    Unreadable {
        error: String,
    },
}

/// Every app's `checks.yml`, for the stack edit read.
pub fn checks_by_app(texts: &StackTexts, apps: &[String]) -> BTreeMap<String, ChecksView> {
    apps.iter()
        .map(|app| {
            let rel = format!("{app}/checks.yml");
            let view = match texts.get(&rel) {
                None => ChecksView::Read {
                    checks: Vec::new(),
                    manual: Vec::new(),
                    probes: Vec::new(),
                    busy_check: None,
                    url: None,
                },
                Some(text) => match serde_yaml::from_str::<ServiceChecks>(text) {
                    Ok(sc) => ChecksView::Read {
                        checks: sc.checks,
                        manual: sc.manual,
                        probes: sc.probes,
                        busy_check: sc.busy_check,
                        url: sc.url,
                    },
                    Err(e) => ChecksView::Unreadable {
                        error: e.to_string(),
                    },
                },
            };
            (app.clone(), view)
        })
        .collect()
}

/// feat-checks-1: one app's `checks.yml` as the editor holds it. The three
/// lists are each `Option`: the browser only sends one when its own box
/// was actually touched (the form is three separate JSON-textareas, one
/// per list, so a box left alone must not read as "this list is now
/// empty") — the same `Option<Vec<_>>` patch convention
/// `stackedit_apps::AppsEdit` already uses for storage/data_mounts/
/// log_files. `busy_check` and `url` are always sent (cheap scalars, no
/// ambiguity to guard against), `None` meaning "no busy check"/"no link".
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChecksEdit {
    /// The app this `checks.yml` belongs to (`<app>/checks.yml`).
    pub app: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<CheckEdit>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual: Option<Vec<ManualEdit>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probes: Option<Vec<ProbeEdit>>,
    /// The busy-check command; `None` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub busy_check: Option<String>,
    /// The link this app opens at; `None` removes it (then the client
    /// falls back to the route file, per `ServiceChecks::url`'s own doc).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// One measured check, and which check of the file it was (`None`: new).
/// `check`'s fields are flattened (the browser sends one flat JSON object
/// per row, `origin` alongside `name`/`command`/…, the same shape the
/// generic `originJson`/`parseJsonList` list editor already uses for
/// storage entries and log files) — `deny_unknown_fields` cannot sit on a
/// struct with a `flatten` field, so this type is left open and `Check`'s
/// own fields are what actually bound it.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CheckEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    #[serde(flatten)]
    pub check: Check,
}

/// One manual question, and which one of the file's `manual:` list it was.
///
/// fix-182-dashboard-edits: `id`/`replaces` round-trip the `ManualCheck`
/// fields of the same name — the gap this closes is that the dashboard's
/// checks form used to edit only `text`/`once`, so the moment Kenny edited
/// either one here, `as_check` rebuilt the item from scratch and silently
/// dropped its `id` (`ManualCheck`'s own doc explains why a dropped `id`
/// throws away any answer already recorded). The browser now sends back
/// whatever `id`/`replaces` the item was loaded with, for every item in
/// the list, not only the one actually touched — `checks_ops`'s `seq_ops`
/// reserializes the whole list it was handed, so an item whose `id` goes
/// missing here would lose it even if nothing about it changed.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    pub text: String,
    #[serde(default)]
    pub once: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replaces: Vec<String>,
}

/// One nightly probe, and which probe of the file it was. `probe`'s fields
/// are flattened, same reason as `CheckEdit`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ProbeEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    #[serde(flatten)]
    pub probe: Probe,
}

impl ManualEdit {
    fn as_check(&self) -> ManualCheck {
        // fix-182-dashboard-edits: `id`/`replaces` now round-trip (the
        // struct doc says why). `Text` has no `id` field at all, so an
        // item that carries an explicit `id` or a `replaces` bridge must
        // use `Detailed` even when `once` is false — otherwise the id
        // this same edit may just have been given by
        // `fill_new_manual_ids` would be thrown away right back out.
        if self.once || self.id.is_some() || !self.replaces.is_empty() {
            ManualCheck::Detailed {
                text: self.text.clone(),
                once: self.once,
                id: self.id.clone(),
                replaces: self.replaces.clone(),
            }
        } else {
            ManualCheck::Text(self.text.clone())
        }
    }
}

/// fix-182-dashboard-edits: assign a stable `id` to a manual check that
/// does not have one yet, in exactly the two cases where the gap this
/// closes would otherwise bite:
///
/// - a brand new check (`origin: None`) — it has never had a hash-based
///   identity to lose, so it is cheaper to give it a real one from birth
///   than to let it start life id-less.
/// - an edit of an old, id-less check whose `text` or `once` actually
///   changed — exactly the case `ManualCheck`'s own doc warns about: the
///   moment the text changes is the moment the hash fallback (`hash(stack/
///   app/text)`) would quietly start a new identity and lose whatever was
///   already answered. Kenny's choice (dashboard-edits-id-choice): assign
///   an id here rather than leave it id-less, so this exact loss can never
///   happen to the same check twice.
///
/// An item that already carries its own `id` (round-tripped from what the
/// browser loaded) is left alone. `taken` seeds from every id already in
/// the file plus every id already sitting on another edit in this same
/// save, so two ids generated in the same request cannot collide with
/// each other either.
fn fill_new_manual_ids(app: &str, old: &[ManualCheck], edits: &mut [ManualEdit]) {
    let mut taken: std::collections::BTreeSet<String> = old
        .iter()
        .filter_map(|m| m.id())
        .map(str::to_string)
        .collect();
    for e in edits.iter() {
        if let Some(id) = &e.id {
            taken.insert(id.clone());
        }
    }
    for e in edits.iter_mut() {
        if e.id.is_some() || e.text.trim().is_empty() {
            continue;
        }
        let needs_id = match e.origin {
            None => true,
            Some(i) => match old.get(i) {
                Some(old_check) if old_check.id().is_none() => {
                    old_check.text() != e.text || old_check.once() != e.once
                }
                _ => false,
            },
        };
        if needs_id {
            e.id = Some(generate_manual_id(app, &e.text, &mut taken));
        }
    }
}

/// A fresh, stable, unique-within-`taken` manual-check id: the same
/// `{app}-{text slug}` shape every `checks.yml` in the repo already
/// carries by hand (fix-182's own migration), so a check this form
/// assigns an id to reads exactly like one a person wrote. Scoped to one
/// app's own list, which is also unique within the stack (an app name is
/// its directory name) — together that is unique within the stack, which
/// is what `checks::id_problems` enforces.
fn generate_manual_id(
    app: &str,
    text: &str,
    taken: &mut std::collections::BTreeSet<String>,
) -> String {
    let app_slug = slugify(app);
    let text_slug = slugify(text);
    // A handful of words is plenty to keep a manual check's id
    // recognisable without carrying the whole question into the id.
    let short_slug: String = text_slug.split('-').take(6).collect::<Vec<_>>().join("-");
    let base = if short_slug.is_empty() {
        app_slug.clone()
    } else {
        format!("{app_slug}-{short_slug}")
    };
    let mut id = base.clone();
    let mut n = 2;
    while id.is_empty() || taken.contains(&id) {
        id = format!("{base}-{n}");
        n += 1;
    }
    taken.insert(id.clone());
    id
}

/// Lowercase ascii-alphanumeric words joined by single dashes — leading,
/// trailing and repeated separators collapse away, so `"Kijk of het werkt?"`
/// becomes `"kijk-of-het-werkt"`.
fn slugify(s: &str) -> String {
    let mut out = String::new();
    for ch in s.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn to_value<T: Serialize>(v: &T) -> Value {
    // Every type here is one of `ServiceChecks`' own field types: it always
    // serializes, so a failure would be a bug in this module, not bad input
    // from the browser.
    serde_yaml::to_value(v).expect("checks.yml types always serialize")
}

/// checks-form: what the checks form keeps to before any text is touched —
/// the same "required below Application" rule `Check::blind_spot`'s own doc
/// states, enforced here because the type itself only says the field is
/// optional.
pub fn checks_problems(edit: &ChecksEdit) -> Vec<String> {
    let mut out = Vec::new();
    if edit.app.trim().is_empty() {
        out.push("an app must be picked".to_string());
    }
    for (n, c) in edit.checks.iter().flatten().enumerate() {
        let who = if c.check.name.trim().is_empty() {
            format!("check {}", n + 1)
        } else {
            c.check.name.clone()
        };
        if c.check.name.trim().is_empty() {
            out.push(format!("check {}: needs a name", n + 1));
        }
        if c.check.command.trim().is_empty() {
            out.push(format!("{who}: needs a command"));
        }
        if c.check.layer < Layer::Application && c.check.blind_spot.is_none() {
            out.push(format!(
                "{who}: a check below Application layer needs a blind spot"
            ));
        }
    }
    for (n, m) in edit.manual.iter().flatten().enumerate() {
        if m.text.trim().is_empty() {
            out.push(format!("manual check {}: needs text", n + 1));
        }
    }
    for (n, p) in edit.probes.iter().flatten().enumerate() {
        let who = if p.probe.name.trim().is_empty() {
            format!("probe {}", n + 1)
        } else {
            p.probe.name.clone()
        };
        if p.probe.name.trim().is_empty() {
            out.push(format!("probe {}: needs a name", n + 1));
        }
        if p.probe.command.trim().is_empty() {
            out.push(format!("{who}: needs a command"));
        }
    }
    if let Some(c) = &edit.busy_check
        && c.trim().is_empty()
    {
        out.push("the busy check needs a command, or remove it".to_string());
    }
    out
}

/// A freshly created `checks.yml`: only the sections the edit actually
/// uses, nothing empty.
pub fn checks_value(edit: &ChecksEdit) -> Value {
    let mut m = Mapping::new();
    let checks: Vec<&CheckEdit> = edit.checks.iter().flatten().collect();
    if !checks.is_empty() {
        m.insert(
            Value::from("checks"),
            Value::Sequence(checks.into_iter().map(|c| to_value(&c.check)).collect()),
        );
    }
    let mut manual: Vec<ManualEdit> = edit.manual.iter().flatten().cloned().collect();
    if !manual.is_empty() {
        // There is no old file yet, so every item here is "new" by
        // definition — an `origin` the browser may still have sent
        // (stale, from whatever this app's form last read) cannot point
        // at anything real.
        for e in &mut manual {
            e.origin = None;
        }
        fill_new_manual_ids(&edit.app, &[], &mut manual);
        m.insert(
            Value::from("manual"),
            Value::Sequence(
                manual
                    .into_iter()
                    .map(|e| to_value(&e.as_check()))
                    .collect(),
            ),
        );
    }
    let probes: Vec<&ProbeEdit> = edit.probes.iter().flatten().collect();
    if !probes.is_empty() {
        m.insert(
            Value::from("probes"),
            Value::Sequence(probes.into_iter().map(|p| to_value(&p.probe)).collect()),
        );
    }
    if let Some(cmd) = &edit.busy_check {
        m.insert(
            Value::from("busy_check"),
            to_value(&BusyCheck {
                command: cmd.clone(),
            }),
        );
    }
    if let Some(url) = &edit.url {
        m.insert(Value::from("url"), Value::from(url.as_str()));
    }
    Value::Mapping(m)
}

/// The ops that turn an existing `checks.yml` (parsed as `old`) into what
/// `want` describes.
pub fn checks_ops(old: &ServiceChecks, want: &ChecksEdit) -> Result<Vec<Op>, String> {
    let mut ops = Vec::new();
    // Each list is only touched when the browser actually sent it — an
    // untouched textarea (`None`) must leave that part of the file alone,
    // not be read as "now empty" (the struct doc explains why).
    if let Some(checks) = &want.checks {
        seq_ops(
            &mut ops,
            "checks",
            &old.checks.iter().map(to_value).collect::<Vec<_>>(),
            checks.iter().map(|c| (c.origin, to_value(&c.check))),
        )?;
    }
    if let Some(manual) = &want.manual {
        let mut manual = manual.clone();
        fill_new_manual_ids(&want.app, &old.manual, &mut manual);
        seq_ops(
            &mut ops,
            "manual",
            &old.manual.iter().map(to_value).collect::<Vec<_>>(),
            manual.iter().map(|e| (e.origin, to_value(&e.as_check()))),
        )?;
    }
    if let Some(probes) = &want.probes {
        seq_ops(
            &mut ops,
            "probes",
            &old.probes.iter().map(to_value).collect::<Vec<_>>(),
            probes.iter().map(|p| (p.origin, to_value(&p.probe))),
        )?;
    }
    let old_busy = old.busy_check.as_ref().map(|b| b.command.clone());
    if old_busy.as_deref().map(str::trim) != want.busy_check.as_deref().map(str::trim) {
        match &want.busy_check {
            // The whole mapping, not `busy_check.command` alone: the file
            // may have no `busy_check:` key yet at all, and `yamledit`'s
            // `Op::Set` cannot create a mapping a nested path's parent
            // segment does not already find (`map_at` only ever descends
            // into a key that is there).
            Some(cmd) if !cmd.trim().is_empty() => ops.push(Op::Set {
                path: path("busy_check"),
                value: to_value(&BusyCheck {
                    command: cmd.clone(),
                }),
            }),
            _ if old.busy_check.is_some() => ops.push(Op::Remove {
                path: path("busy_check"),
            }),
            _ => {}
        }
    }
    let old_url = old.url.as_deref().map(str::trim);
    let want_url = want.url.as_deref().map(str::trim);
    if old_url != want_url {
        match want_url {
            Some(u) if !u.is_empty() => ops.push(Op::Set {
                path: path("url"),
                value: Value::from(u),
            }),
            _ if old.url.is_some() => ops.push(Op::Remove { path: path("url") }),
            _ => {}
        }
    }
    Ok(ops)
}

/// Rebuild one of the three lists (`checks`, `manual`, `probes`) as one
/// `Op::Seq`, keeping an unchanged item's exact text and comments (the same
/// scheme `stackedit::firewall_ops` uses for firewall rules): an edit item
/// whose `origin` points at an old item with the identical value becomes
/// `Item::Keep`; a changed one is `Item::Retext`; one with no `origin` is
/// `Item::New`. Nothing is pushed when the list did not change at all.
fn seq_ops(
    ops: &mut Vec<Op>,
    key: &str,
    old: &[Value],
    edits: impl Iterator<Item = (Option<usize>, Value)>,
) -> Result<(), String> {
    let mut used = std::collections::BTreeSet::new();
    let mut items = Vec::new();
    for (n, (origin, value)) in edits.enumerate() {
        match origin {
            Some(i) if i >= old.len() => {
                return Err(format!(
                    "{key} {} of the edit points at item {} of the file, which has {}",
                    n + 1,
                    i + 1,
                    old.len()
                ));
            }
            Some(i) if used.insert(i) => {
                if old[i] == value {
                    items.push(Item::Keep(i));
                } else {
                    items.push(Item::Retext(i, value));
                }
            }
            Some(i) => {
                return Err(format!(
                    "{key} {} of the edit points at item {} of the file twice",
                    n + 1,
                    i + 1
                ));
            }
            None => items.push(Item::New(value)),
        }
    }
    let unchanged = items.len() == old.len()
        && items
            .iter()
            .enumerate()
            .all(|(n, it)| matches!(it, Item::Keep(i) if *i == n));
    if !unchanged {
        ops.push(Op::Seq {
            path: path(key),
            items,
        });
    }
    Ok(())
}

impl ChecksEdit {
    /// The change in a few words, for the commit subject. Self-contained
    /// (no before-text needed): what is new is exactly what has no
    /// `origin`, whatever else changed about an existing item is folded
    /// into "edited".
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();
        let added = self
            .checks
            .iter()
            .flatten()
            .filter(|c| c.origin.is_none())
            .count();
        if added > 0 {
            parts.push(format!("{added} check(s) added"));
        }
        let probes_added = self
            .probes
            .iter()
            .flatten()
            .filter(|p| p.origin.is_none())
            .count();
        if probes_added > 0 {
            parts.push(format!("{probes_added} probe(s) added"));
        }
        let manual_added = self
            .manual
            .iter()
            .flatten()
            .filter(|m| m.origin.is_none())
            .count();
        if manual_added > 0 {
            parts.push(format!("{manual_added} manual check(s) added"));
        }
        let existing = self
            .checks
            .iter()
            .flatten()
            .filter(|c| c.origin.is_some())
            .count()
            + self
                .probes
                .iter()
                .flatten()
                .filter(|p| p.origin.is_some())
                .count()
            + self
                .manual
                .iter()
                .flatten()
                .filter(|m| m.origin.is_some())
                .count();
        if parts.is_empty() && existing > 0 {
            parts.push(format!("{existing} item(s) edited or kept"));
        }
        if self.busy_check.is_some() {
            parts.push("busy check set".to_string());
        }
        if self.url.is_some() {
            parts.push("link set".to_string());
        }
        if parts.is_empty() {
            parts.push("checks.yml edited".to_string());
        }
        format!("{}: {}", self.app, parts.join(", "))
    }
}
