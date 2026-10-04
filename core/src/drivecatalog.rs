//! drive-reach (Kenny, 2026-10-03: "Claude must always be able to reach every
//! control, also after a control is renamed or moved"): the Live view control
//! catalog, read by both ends of `homelab ui`.
//!
//! The catalog is `admin/web/js/drivecatalog.json`, generated from the
//! dashboard's own declarations (`drivable.js` `declare`) and router
//! (`router.js`) by `admin/web/scripts/drivecatalog.mjs`, never written by
//! hand. The dashboard embeds it and serves it; the client fetches it from
//! the running dashboard and checks a `ui click` or `ui goto` against it
//! before anything is sent, so a guessed name is answered here, with the
//! closest real ones, instead of on Kenny's screen. Pure: parsing, lookup,
//! the closest names and an address's new home.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The catalog's shape this build reads (`schema` in the file). A catalog
/// of another schema is one this side cannot read: the client refuses a
/// step against it rather than send it unchecked (review H2).
pub const SCHEMA: u32 = 1;

/// The hash a dashboard names its catalog by in every `ui state` answer
/// (review M3): the client keeps the catalog by it and fetches it again only
/// when it changes. SHA-256 of the exact text, hex.
pub fn hash_text(text: &str) -> String {
    let d = Sha256::digest(text.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The whole catalog, as the generator writes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Catalog {
    /// The file's shape ([`SCHEMA`]); 0 when absent.
    #[serde(default)]
    pub schema: u32,
    pub controls: Vec<Control>,
    /// The page fields a redesign renamed, each with its old ids (a page
    /// field is otherwise found by its element id, undeclared).
    #[serde(default)]
    pub fields: Vec<Field>,
    /// Every address the router knows, without the leading `/`, retired
    /// ones included.
    pub addresses: Vec<String>,
    /// A retired address (a key of `addresses`, or `stacks/{stack}/<tab>`)
    /// and the address the browser's own redirect sends it to, for the
    /// address with no query string.
    pub redirects: BTreeMap<String, String>,
    /// The placeholder `redirects` names a stack by (`{stack}`).
    #[serde(default)]
    pub stack_slot: String,
    /// A stack hub's tabs (`/stacks/<name>/<tab>`), the retired ones (which
    /// redirect) included: the router's own list (review M8, L1).
    #[serde(default)]
    pub stack_tabs: Vec<String>,
    /// The longest a tab spends on one page-control step (navigation, the
    /// search for the control, the dialog it opens), in ms: pagedrive.js's
    /// own budget, below the dashboard's wait for the tab's answer.
    #[serde(default)]
    pub tab_budget_ms: u64,
}

/// One declared page control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Control {
    pub id: String,
    /// The page module that declares it.
    pub page: String,
    pub what: String,
    /// `dialog`, `run` or `view`.
    pub opens: String,
    /// A row whose press does something else than `opens`, by the row's
    /// last part or the whole row (redesign-final-42: the stack hub's
    /// Update goes to the Update flow, a `view`).
    #[serde(default, rename = "rowOpens")]
    pub row_opens: std::collections::BTreeMap<String, String>,
    /// The row's shape when it repeats per row (`<stack>/<app>`).
    #[serde(default)]
    pub row: Option<String>,
    /// Where it lives now, a row or stack as its placeholder.
    #[serde(default)]
    pub href: Option<String>,
    /// The ids it had before.
    #[serde(default)]
    pub was: Vec<Was>,
    /// The state it is on screen in, when only in one.
    #[serde(default)]
    pub shows: Option<String>,
    /// The steps that bring it on screen from its page.
    #[serde(default)]
    pub reach: Vec<serde_json::Value>,
    /// Drawn more than once without rows on purpose (a drawer's x and its
    /// Cancel), each press doing the same (review M6).
    #[serde(default)]
    pub twins: bool,
}

impl Control {
    /// What pressing this control on `row` does: the row's own
    /// `rowOpens` (by the whole row, then its last part), else `opens`.
    pub fn opens_for(&self, row: Option<&str>) -> &str {
        row.and_then(|r| {
            self.row_opens
                .get(r)
                .or_else(|| self.row_opens.get(r.rsplit('/').next().unwrap_or(r)))
        })
        .map_or(self.opens.as_str(), String::as_str)
    }
}

/// A declared page field (`ui type/pick/check/edit <field>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    pub page: String,
    pub what: String,
    /// The ids it had before.
    #[serde(default)]
    pub was: Vec<String>,
    /// Set when the field repeats per row (a host.toml key): its element id
    /// is `<id>-<row>`, and this is the row's shape.
    #[serde(default)]
    pub row: Option<String>,
}

/// A button the open page-level dialog offers, as the tab reported it:
/// its Live view name (empty when it has none) and its visible label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogControl {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub label: String,
}

/// What a field name a driver used means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldName<'a> {
    /// A declared field, by its id (`row` set for a field per row).
    Known(&'a Field),
    /// An old id: the field's id now.
    Renamed { field: &'a Field, now: String },
    /// No page field has or had that name: why, and the fix to print.
    Unknown { why: String, fix: String },
}

/// An old id of a control, and the press that picks it from the control's
/// menu when it became one of its items.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Was {
    pub id: String,
    #[serde(default)]
    pub press: Option<String>,
}

/// What a name a driver clicked means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click<'a> {
    /// A control by its own id.
    Known(&'a Control),
    /// An old id: the control it is now, and the press that picks it.
    Renamed {
        control: &'a Control,
        was: String,
        press: Option<String>,
    },
    /// No control has or had that name: why, and the fix to print.
    Unknown { why: String, fix: String },
}

/// Where a `goto` lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landing {
    /// The address the browser ends on (its query string kept).
    pub path: String,
    /// Set when `path` is a retired address: its new home, in words.
    pub note: Option<String>,
}

impl Catalog {
    /// The catalog from its JSON text.
    pub fn parse(text: &str) -> Result<Catalog, String> {
        serde_json::from_str(text).map_err(|e| format!("the control catalog does not read: {e}"))
    }

    pub fn control(&self, id: &str) -> Option<&Control> {
        self.controls.iter().find(|c| c.id == id)
    }

    /// The field an old field id became, if a declaration keeps that id.
    pub fn renamed_field(&self, name: &str) -> Option<&Field> {
        self.fields.iter().find(|f| f.was.iter().any(|w| w == name))
    }

    /// A page field name, checked (review M5): a declared field (or one row
    /// of a field per row), an old id of one, or nothing with the closest.
    pub fn field(&self, name: &str) -> FieldName<'_> {
        if let Some(f) = self.fields.iter().find(|f| {
            f.id == name
                || (f.row.is_some()
                    && name
                        .strip_prefix(f.id.as_str())
                        .and_then(|r| r.strip_prefix('-'))
                        .is_some_and(|r| !r.is_empty()))
        }) {
            return FieldName::Known(f);
        }
        if let Some(f) = self.renamed_field(name) {
            return FieldName::Renamed {
                field: f,
                now: f.id.clone(),
            };
        }
        let mut near: Vec<(usize, &Field)> = self
            .fields
            .iter()
            .map(|f| (distance(name, &f.id), f))
            .filter(|(d, f)| {
                *d <= (name.chars().count() / 3).max(3)
                    || words(name).iter().any(|w| words(&f.id).contains(w))
            })
            .collect();
        near.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.id.cmp(&b.1.id)));
        let fix = if near.is_empty() {
            "`homelab ui list fields` lists every page field".to_string()
        } else {
            format!(
                "the closest: {}; `homelab ui list fields` lists every page field",
                near.iter()
                    .take(4)
                    .map(|(_, f)| match &f.row {
                        Some(r) => format!("{}-{r} (on {}: {})", f.id, f.page, f.what),
                        None => format!("{} (on {}: {})", f.id, f.page, f.what),
                    })
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        };
        FieldName::Unknown {
            why: format!("no page declares a field {name}"),
            fix,
        }
    }

    /// `/stacks/<name>[/<tab>]` checked against the router's own shape
    /// (review L1): one or two parts after `stacks`, the tab one the hub
    /// has or had. Whether the stack exists is the dashboard's to say.
    /// `Ok(Some(note))`: a retired tab, and where it lands now.
    pub fn stack_path(&self, path: &str) -> Result<Option<String>, String> {
        let p = path.split(['?', '#']).next().unwrap_or("");
        let rest = p.trim_start_matches('/').trim_end_matches('/');
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.first() != Some(&"stacks") || !(2..=3).contains(&parts.len()) {
            return Err(format!(
                "there is no page at {path}; a stack's page is /stacks/<name>[/<tab>]"
            ));
        }
        if parts[1].is_empty() {
            return Err(format!("{path} names no stack"));
        }
        let Some(tab) = parts.get(2) else {
            return Ok(None);
        };
        if !self.stack_tabs.is_empty() && !self.stack_tabs.iter().any(|t| t == tab) {
            return Err(format!(
                "a stack page has no tab {tab}; its tabs are {}",
                self.stack_tabs.join(", ")
            ));
        }
        let slot = if self.stack_slot.is_empty() {
            "{stack}"
        } else {
            self.stack_slot.as_str()
        };
        Ok(self
            .redirects
            .get(&format!("stacks/{slot}/{tab}"))
            .map(|to| {
                let to = to.replace(slot, &encode_segment(parts[1]));
                format!("{p} is an old address; it is {to} now")
            }))
    }

    /// A clicked name, checked: a control, an old name of one, or nothing
    /// with the closest real names and the line that clicks the best one.
    pub fn click(&self, name: &str) -> Click<'_> {
        if let Some(c) = self.control(name) {
            return Click::Known(c);
        }
        for c in &self.controls {
            if let Some(w) = c.was.iter().find(|w| w.id == name) {
                return Click::Renamed {
                    control: c,
                    was: name.to_string(),
                    press: w.press.clone(),
                };
            }
        }
        let near = self.closest(name, 4);
        let fix = match near.first() {
            Some(best) => format!(
                "the closest: {}; e.g. {}; `homelab ui list <words>` lists every control",
                near.iter()
                    .map(|c| format!("{} (on {}: {})", c.id, c.page, c.what))
                    .collect::<Vec<_>>()
                    .join("; "),
                click_line(best, None)
            ),
            None => {
                "`homelab ui list` lists every control, its page and the line that clicks it".into()
            }
        };
        Click::Unknown {
            why: format!("no page declares a control {name}"),
            fix,
        }
    }

    /// The declared controls closest to `name`, best first: by spelling (an
    /// id a typo or two away) and by words (a word of the name in a
    /// control's id, its old ids or what it does). The same scoring as
    /// drivable.js `closest`, so the tab and the client name the same ones.
    pub fn closest(&self, name: &str, n: usize) -> Vec<&Control> {
        let want = words(name);
        let mut scored: Vec<(i64, &Control)> = self
            .controls
            .iter()
            .map(|c| {
                let names: Vec<&str> = std::iter::once(c.id.as_str())
                    .chain(c.was.iter().map(|w| w.id.as_str()))
                    .collect();
                let typo = names
                    .iter()
                    .map(|x| distance(name, x))
                    .min()
                    .unwrap_or(usize::MAX);
                let own: Vec<String> = names.iter().flat_map(|x| words(x)).collect();
                let said = words(&c.what);
                let mut hits = 0i64;
                for w in &want {
                    if own.contains(w) {
                        hits += 2;
                    } else if said.contains(w)
                        || own
                            .iter()
                            .any(|o| o.starts_with(w.as_str()) || w.starts_with(o.as_str()))
                    {
                        hits += 1;
                    }
                }
                let near = typo <= (name.chars().count() / 4).max(2);
                let score = hits * 10 + if near { 30 - typo as i64 } else { 0 };
                (score, c)
            })
            .filter(|(s, _)| *s > 0)
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        scored.into_iter().take(n).map(|(_, c)| c).collect()
    }

    /// The controls a `homelab ui list` filter picks: every one for an empty
    /// filter; else those on the named page (by its module or its address's
    /// first part), else those whose id, old ids, page or words hold every
    /// word of the filter.
    pub fn list(&self, filter: &[String]) -> Vec<&Control> {
        if filter.is_empty() {
            return self.controls.iter().collect();
        }
        if let [one] = filter {
            let one = one.trim_start_matches('/');
            let on_page: Vec<&Control> = self
                .controls
                .iter()
                .filter(|c| c.page == one || first_segment(c.href.as_deref()) == Some(one))
                .collect();
            if !on_page.is_empty() {
                return on_page;
            }
        }
        let want: Vec<String> = filter.iter().flat_map(|f| words(f)).collect();
        self.controls
            .iter()
            .filter(|c| {
                let mut hay = words(&c.id);
                hay.extend(c.was.iter().flat_map(|w| words(&w.id)));
                hay.extend(words(&c.page));
                hay.extend(words(&c.what));
                want.iter()
                    .all(|w| hay.iter().any(|h| h.starts_with(w.as_str())))
            })
            .collect()
    }

    /// Where `goto <path>` lands, through the same redirect table the
    /// browser's router uses (generated from it): a current address as it
    /// is, a retired one at its new home, the query string the driver gave
    /// kept and the new home's own set over it. `stacks`: the fleet's stack
    /// names; a retired address that names a stack takes the first, as the
    /// router does. `Err`: the address is not one the router knows.
    pub fn land(&self, path: &str, stacks: &[String]) -> Result<Landing, String> {
        let (p, query) = match path.split_once('?') {
            Some((p, q)) => (p, q),
            None => (path, ""),
        };
        let p = p.split('#').next().unwrap_or("");
        if !p.starts_with('/') {
            return Err(format!("{path} is not an absolute path"));
        }
        let rest = p.trim_start_matches('/').trim_end_matches('/');
        let slot = if self.stack_slot.is_empty() {
            "{stack}"
        } else {
            self.stack_slot.as_str()
        };
        let fill = |to: &str, stack: &str| to.replace(slot, &encode_segment(stack));
        if let Some(to) = self.redirects.get(rest) {
            let first = stacks.first().map(String::as_str);
            let (to, note) = match first {
                Some(s) if to.contains(slot) => {
                    let to = fill(to, s);
                    let note =
                        format!("{p} is an old address; it lands on the first stack's page, {to}");
                    (to, note)
                }
                Some(_) => (to.clone(), format!("{p} is an old address; it is {to} now")),
                // Not known here (the client has no fleet): the router
                // sends it to the first stack's page, or to the stack list
                // when there is none (review L1: it is never just /stacks).
                None if to.contains(slot) => (
                    "/stacks".to_string(),
                    format!(
                        "{p} is an old address; it lands on the first stack's page, {} (the stack list when there is no stack)",
                        to.replace(slot, "<first stack>")
                    ),
                ),
                None => (to.clone(), format!("{p} is an old address; it is {to} now")),
            };
            return Ok(Landing {
                note: Some(note),
                path: merge_query(&to, query),
            });
        }
        if self.addresses.iter().any(|a| a == rest) {
            return Ok(Landing {
                path: merge_query(&format!("/{rest}"), query),
                note: None,
            });
        }
        let parts: Vec<&str> = rest.split('/').collect();
        if parts.first() == Some(&"stacks") && (parts.len() == 2 || parts.len() == 3) {
            let name = parts[1];
            if !stacks.iter().any(|s| s == name) {
                return Err(format!("the fleet has no stack {name}"));
            }
            if let Some(tab) = parts.get(2) {
                let key = format!("stacks/{slot}/{tab}");
                if let Some(to) = self.redirects.get(&key) {
                    let to = fill(to, name);
                    return Ok(Landing {
                        note: Some(format!("{p} is an old address; it is {to} now")),
                        path: merge_query(&to, query),
                    });
                }
            }
            return Ok(Landing {
                path: merge_query(&format!("/{rest}"), query),
                note: None,
            });
        }
        let near = self.closest_addresses(rest, 3);
        Err(format!(
            "there is no page at {path}{}",
            if near.is_empty() {
                String::new()
            } else {
                format!("; the closest: /{}", near.join(", /"))
            }
        ))
    }

    /// The router's addresses closest to `rest` by spelling.
    pub fn closest_addresses(&self, rest: &str, n: usize) -> Vec<&str> {
        let mut v: Vec<(usize, &str)> = self
            .addresses
            .iter()
            .filter(|a| !a.is_empty())
            .map(|a| (distance(rest, a), a.as_str()))
            .filter(|(d, a)| *d <= (rest.len().max(a.len()) / 2).max(2))
            .collect();
        v.sort();
        v.into_iter().take(n).map(|(_, a)| a).collect()
    }
}

/// A name clicked or pressed inside the open page-level dialog, checked
/// against the buttons the tab reported it offers (review H3): by its Live
/// view name (an old name of a declared control counts as the one it
/// became), or by its visible label when exactly one button wears it, the
/// way the tab's `pick` finds it. `Err`: why and the fix, for a refusal.
pub fn dialog_click(
    offer: &[DialogControl],
    title: &str,
    name: &str,
    catalog: &Catalog,
) -> Result<(), (String, String)> {
    let now = match catalog.click(name) {
        Click::Renamed { control, .. } => control.id.as_str(),
        _ => name,
    };
    if offer
        .iter()
        .any(|c| !c.id.is_empty() && (c.id == name || c.id == now))
    {
        return Ok(());
    }
    let want = name.trim().to_lowercase();
    let labelled = offer
        .iter()
        .filter(|c| c.label.trim().to_lowercase() == want)
        .count();
    if labelled == 1 {
        return Ok(());
    }
    if drawn_later(offer, catalog, |c| match catalog.click(name) {
        Click::Known(k) | Click::Renamed { control: k, .. } => k.page == c.page,
        _ => false,
    }) {
        return Ok(());
    }
    let mut names: Vec<String> = offer
        .iter()
        .map(|c| {
            if c.id.is_empty() {
                format!("\"{}\"", c.label.trim())
            } else {
                c.id.clone()
            }
        })
        .filter(|n| n != "\"\"")
        .collect();
    names.sort();
    names.dedup();
    Err((
        if labelled > 1 {
            format!("{labelled} buttons in {title} are labelled {name}")
        } else {
            format!("there is no control {name} in {title}")
        },
        if names.is_empty() {
            format!("{title} offers no button; homelab ui close")
        } else {
            format!("its buttons are: {}", names.join(", "))
        },
    ))
}

/// redesign-integrate-8: a page drawn as a dialog (Stacks' Deploy all
/// changes) draws its declared controls and fields after its own read,
/// after the snapshot of the dialog the tab reported when it opened. A
/// declared control or field of the same page module as a control the
/// dialog offers is the tab's to find (it waits for one as for a page's),
/// never refused from that stale snapshot.
fn drawn_later(
    offer: &[DialogControl],
    catalog: &Catalog,
    same_page: impl Fn(&Control) -> bool,
) -> bool {
    offer
        .iter()
        .filter_map(|o| catalog.control(&o.id))
        .any(same_page)
}

/// A field typed into, picked or ticked inside the open page-level dialog,
/// checked against the field ids the tab reported it holds (review M5/H3),
/// or a declared field of the page the dialog draws (`drawn_later`).
pub fn dialog_field(
    fields: &[String],
    offer: &[DialogControl],
    title: &str,
    name: &str,
    catalog: &Catalog,
) -> Result<(), (String, String)> {
    let now = catalog
        .renamed_field(name)
        .map(|f| f.id.as_str())
        .unwrap_or(name);
    if fields.iter().any(|f| f == name || f == now) {
        return Ok(());
    }
    if let FieldName::Known(f) | FieldName::Renamed { field: f, .. } = catalog.field(name)
        && drawn_later(offer, catalog, |c| c.page == f.page)
    {
        return Ok(());
    }
    Err((
        format!("{title} has no field {name}"),
        if fields.is_empty() {
            format!("{title} has no field")
        } else {
            format!("its fields are: {}", fields.join(", "))
        },
    ))
}

/// The exact `homelab ui` line that clicks `c`, the row as its declared
/// placeholder when none is given and it repeats per row.
pub fn click_line(c: &Control, row: Option<&str>) -> String {
    match (&c.row, row) {
        (_, Some(r)) => format!("homelab ui click {} {r}", c.id),
        (Some(shape), None) => format!("homelab ui click {} {shape}", c.id),
        (None, None) => format!("homelab ui click {}", c.id),
    }
}

/// A reach step as the `homelab ui` line that takes it.
pub fn reach_line(step: &serde_json::Value) -> String {
    let s = |k: &str| step[k].as_str().unwrap_or("").to_string();
    let verb = s("do");
    let rest: Vec<String> = match verb.as_str() {
        "click" => vec![s("control"), s("row")],
        "press" => vec![s("button")],
        _ => vec![s("field"), s("text")],
    };
    let mut line = format!("homelab ui {verb}");
    for w in rest.into_iter().filter(|w| !w.is_empty()) {
        line.push(' ');
        line.push_str(if w == "*" { "<row>" } else { &w });
    }
    line
}

fn first_segment(href: Option<&str>) -> Option<&str> {
    href?.trim_start_matches('/').split(['/', '?']).next()
}

/// `to` with the driver's own query string `query` kept under it: the new
/// home's parameters win (the router's `setParams` over the old query).
fn merge_query(to: &str, query: &str) -> String {
    if query.is_empty() {
        return to.to_string();
    }
    let (path, own) = to.split_once('?').unwrap_or((to, ""));
    let own_keys: Vec<&str> = own
        .split('&')
        .filter(|x| !x.is_empty())
        .map(|x| x.split('=').next().unwrap_or(""))
        .collect();
    let mut parts: Vec<&str> = query
        .split('&')
        .filter(|x| !x.is_empty() && !own_keys.contains(&x.split('=').next().unwrap_or("")))
        .collect();
    parts.extend(own.split('&').filter(|x| !x.is_empty()));
    if parts.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", parts.join("&"))
    }
}

/// A stack name as one path segment (the router's `encodeURIComponent`
/// for the names a stack may have: letters, digits, `-`, `_`, `.`).
fn encode_segment(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The words of an id or a sentence, lower case.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

/// Levenshtein distance, for a mistyped name.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            cur[j] = (prev[j] + 1)
                .min(cur[j - 1] + 1)
                .min(prev[j - 1] + usize::from(a[i - 1] != b[j - 1]));
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redesign_final_42_a_row_declares_its_own_kind_of_press() {
        let c: Control = serde_json::from_str(
            r#"{"id": "stack-head", "page": "stack", "what": "x", "opens": "dialog",
                "rowOpens": {"update": "view"}, "row": "<stack>/<backup|update|deploy>"}"#,
        )
        .unwrap();
        assert_eq!(c.opens_for(Some("kp-soft/update")), "view");
        assert_eq!(c.opens_for(Some("update")), "view");
        assert_eq!(c.opens_for(Some("kp-soft/backup")), "dialog");
        assert_eq!(c.opens_for(None), "dialog");
        // The shipped catalog says the same of the stack hub's header.
        let shipped = Catalog::parse(include_str!("../../admin/web/js/drivecatalog.json")).unwrap();
        let head = shipped
            .control("stack-head")
            .expect("stack-head is declared");
        assert_eq!(head.opens_for(Some("kp-soft/update")), "view");
        assert_eq!(head.opens_for(Some("kp-soft/deploy")), "dialog");
    }

    fn cat() -> Catalog {
        Catalog::parse(
            r#"{
            "controls": [
              {"id": "schedule-menu", "page": "schedules", "what": "open one schedule's menu", "opens": "dialog",
               "row": "<schedule id>", "href": "/activity?view=planned",
               "was": [{"id": "edit-schedule", "press": "edit"}, {"id": "delete-schedule", "press": "delete"}]},
              {"id": "new-schedule", "page": "schedules", "what": "open the New schedule drawer", "opens": "dialog",
               "href": "/activity?view=planned"},
              {"id": "reveal-secret", "page": "secrets", "what": "show one secret's value", "opens": "run",
               "row": "<stack>/<secret>", "href": "/stacks/<stack>/settings?section=secrets"}
            ],
            "addresses": ["", "apps", "inbox", "activity", "stacks", "jobs", "status", "secrets"],
            "redirects": {"jobs": "/activity?view=running", "status": "/inbox",
                          "secrets": "/stacks/{stack}/settings?section=secrets",
                          "stacks/{stack}/checks": "/stacks/{stack}",
                          "stacks/{stack}/firewall": "/stacks/{stack}/settings?section=firewall"},
            "stack_slot": "{stack}"
          }"#,
        )
        .unwrap()
    }

    #[test]
    fn drive_reach_an_old_name_is_the_control_it_became_with_its_press() {
        let c = cat();
        match c.click("edit-schedule") {
            Click::Renamed { control, press, .. } => {
                assert_eq!(control.id, "schedule-menu");
                assert_eq!(press.as_deref(), Some("edit"));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(c.click("new-schedule"), Click::Known(_)));
    }

    #[test]
    fn drive_reach_an_unknown_name_is_refused_with_the_closest_and_their_page() {
        let c = cat();
        let Click::Unknown { why, fix } = c.click("new-schedul") else {
            panic!("a typo was taken")
        };
        assert!(why.contains("no page declares a control new-schedul"));
        assert!(
            fix.starts_with("the closest: new-schedule (on schedules"),
            "{fix}"
        );
        assert!(fix.contains("homelab ui click new-schedule"), "{fix}");
        let Click::Unknown { fix, .. } = c.click("show-secret") else {
            panic!()
        };
        assert!(fix.contains("reveal-secret"), "{fix}");
    }

    #[test]
    fn drive_reach_goto_lands_where_the_router_redirects() {
        let c = cat();
        let stacks = vec!["gateway".to_string(), "films".to_string()];
        let l = c.land("/status", &stacks).unwrap();
        assert_eq!(l.path, "/inbox");
        assert!(l.note.unwrap().contains("old address"));
        assert_eq!(
            c.land("/jobs", &stacks).unwrap().path,
            "/activity?view=running"
        );
        assert_eq!(
            c.land("/jobs?stack=films", &stacks).unwrap().path,
            "/activity?stack=films&view=running"
        );
        assert_eq!(
            c.land("/secrets", &stacks).unwrap().path,
            "/stacks/gateway/settings?section=secrets"
        );
        assert_eq!(
            c.land("/stacks/films/firewall", &stacks).unwrap().path,
            "/stacks/films/settings?section=firewall"
        );
        assert_eq!(
            c.land("/stacks/films/checks", &stacks).unwrap().path,
            "/stacks/films"
        );
        assert_eq!(
            c.land("/stacks/films/logs", &stacks).unwrap().path,
            "/stacks/films/logs"
        );
        assert_eq!(
            c.land("/apps", &stacks).unwrap(),
            Landing {
                path: "/apps".into(),
                note: None
            }
        );
        assert_eq!(c.land("/", &stacks).unwrap().path, "/");
        let e = c.land("/inbx", &stacks).unwrap_err();
        assert!(e.contains("the closest: /inbox"), "{e}");
        assert!(
            c.land("/stacks/nope", &stacks)
                .unwrap_err()
                .contains("no stack nope")
        );
    }

    /// review M7: the tab's `closest` (drivable.js) and this one name the
    /// same controls in the same order, over one shared fixture with names
    /// whose UTF-16 length is not their length in chars.
    #[test]
    fn drive_reach_closest_matches_the_tab_on_the_shared_fixture() {
        let fx: serde_json::Value = serde_json::from_str(include_str!(
            "../../admin/web/test/fixtures/drive-closest.json"
        ))
        .unwrap();
        let controls: Vec<Control> = fx["controls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let mut c = c.clone();
                c["opens"] = serde_json::json!("run");
                serde_json::from_value(c).unwrap()
            })
            .collect();
        let cat = Catalog {
            controls,
            ..Catalog::default()
        };
        for case in fx["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let n = case["n"].as_u64().unwrap() as usize;
            let got: Vec<&str> = cat.closest(name, n).iter().map(|c| c.id.as_str()).collect();
            let want: Vec<&str> = case["want"]
                .as_array()
                .unwrap()
                .iter()
                .map(|w| w.as_str().unwrap())
                .collect();
            assert_eq!(got, want, "{name}");
        }
    }

    /// review M5/L1/H2: fields per row, a stack address's shape, the hash.
    #[test]
    fn drive_reach_fields_stack_paths_and_the_hash() {
        let mut c = cat();
        c.fields = vec![
            Field {
                id: "key".into(),
                page: "settings".into(),
                what: "a host.toml key".into(),
                was: vec![],
                row: Some("<key>".into()),
            },
            Field {
                id: "shell-target".into(),
                page: "shell".into(),
                what: "the container".into(),
                was: vec!["shell-vmid".into()],
                row: None,
            },
        ];
        c.stack_tabs = vec!["overview".into(), "logs".into(), "checks".into()];
        assert!(matches!(c.field("key-ask-timeout"), FieldName::Known(f) if f.id == "key"));
        assert!(matches!(c.field("key"), FieldName::Known(_)));
        assert!(matches!(c.field("key-"), FieldName::Unknown { .. }));
        assert!(
            matches!(c.field("shell-vmid"), FieldName::Renamed { now, .. } if now == "shell-target")
        );
        let FieldName::Unknown { fix, .. } = c.field("shell-targt") else {
            panic!()
        };
        assert!(fix.contains("shell-target"), "{fix}");
        assert_eq!(c.stack_path("/stacks/films/logs"), Ok(None));
        assert!(
            c.stack_path("/stacks/films/nope")
                .unwrap_err()
                .contains("no tab nope")
        );
        assert!(
            c.stack_path("/stacks/films/checks")
                .unwrap()
                .unwrap()
                .contains("it is /stacks/films now")
        );
        assert!(c.stack_path("/stacks/a/b/c").is_err());
        assert_eq!(hash_text("abc").len(), 64);
        // A retired address that names a stack, with no fleet known here.
        let note = c.land("/secrets", &[]).unwrap().note.unwrap();
        assert!(note.contains("the first stack's page"), "{note}");
    }

    #[test]
    fn drive_reach_list_filters_by_page_or_words() {
        let c = cat();
        assert_eq!(c.list(&[]).len(), 3);
        let ids = |v: Vec<&Control>| v.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        assert_eq!(
            ids(c.list(&["schedules".into()])),
            ["schedule-menu", "new-schedule"]
        );
        assert_eq!(ids(c.list(&["stacks".into()])), ["reveal-secret"]);
        assert_eq!(ids(c.list(&["secret".into()])), ["reveal-secret"]);
        assert_eq!(
            ids(c.list(&["edit".into(), "schedule".into()])),
            ["schedule-menu"]
        );
    }

    /// redesign-integrate-8: Stacks' Deploy all changes is a page drawn as
    /// a dialog; its plan's controls come after its own read, after the
    /// snapshot the tab sent when it opened. A declared control of the same
    /// page as one the dialog offers goes to the tab (which waits for it);
    /// one of another page is still refused from the snapshot.
    #[test]
    fn redesign_integrate_8_a_dialog_page_s_later_controls_are_the_tab_s_to_find() {
        let c = cat();
        let offer = [DialogControl {
            id: "new-schedule".into(),
            label: "New schedule".into(),
        }];
        assert_eq!(dialog_click(&offer, "Planned", "schedule-menu", &c), Ok(()));
        assert_eq!(dialog_click(&offer, "Planned", "edit-schedule", &c), Ok(()));
        let (why, _) = dialog_click(&offer, "Planned", "reveal-secret", &c).unwrap_err();
        assert!(
            why.contains("there is no control reveal-secret in Planned"),
            "{why}"
        );
    }
}
