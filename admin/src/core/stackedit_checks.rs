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
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManualEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    pub text: String,
    #[serde(default)]
    pub once: bool,
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
        // fix-182: the dashboard form does not yet expose `id`/`replaces` —
        // it only edits text and `once`. An item the browser sends back
        // unchanged never reaches here (`checks_ops`'s `seq_ops` compares
        // serialized values by `origin` and only touches a changed one), so
        // this only drops a manual check's `id` the moment Kenny actually
        // edits that same check's text or `once` flag here, which is also
        // the moment a stable id is least likely to still describe the
        // question — not silently, on every save.
        if self.once {
            ManualCheck::Detailed {
                text: self.text.clone(),
                once: true,
                id: None,
                replaces: Vec::new(),
            }
        } else {
            ManualCheck::Text(self.text.clone())
        }
    }
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
    let manual: Vec<&ManualEdit> = edit.manual.iter().flatten().collect();
    if !manual.is_empty() {
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
