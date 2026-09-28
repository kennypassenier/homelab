//! feat-stacks-2, feat-firewall-1: edits of a stack file that keep its
//! comments.
//!
//! The stack files are written by hand and their comments carry the
//! reasons (which measurement, which form, which finding). A round trip
//! through serde would drop every one of them, so an edit here works on the
//! text: it finds the lines of one key or one list item by their
//! indentation, and replaces exactly those. The comment on a changed line
//! stays; the comment lines above a list item stay with the item.
//!
//! Every edit is checked afterwards: the new text is parsed and must equal
//! the old text's parsed value with the same change made to it. A layout
//! this editor misreads therefore fails loudly ([`EditError::Drift`]) and
//! the page offers the raw file instead; it never writes something other
//! than what was asked. Pure: text in, text out.

use serde_yaml::{Mapping, Value};

/// One step of a path into a YAML document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seg {
    Key(String),
    Index(usize),
}

/// `"firewall.rules.3.dport"` as segments: a number is a list index.
pub fn path(spec: &str) -> Vec<Seg> {
    spec.split('.')
        .filter(|s| !s.is_empty())
        .map(|s| match s.parse::<usize>() {
            Ok(i) => Seg::Index(i),
            Err(_) => Seg::Key(s.to_string()),
        })
        .collect()
}

fn path_text(p: &[Seg]) -> String {
    p.iter()
        .map(|s| match s {
            Seg::Key(k) => k.clone(),
            Seg::Index(i) => i.to_string(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// What cannot be done to the text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    #[error("the file does not parse as YAML: {0}")]
    Parse(String),
    #[error("{0} is not in the file")]
    NotFound(String),
    #[error("{0} is written in a form this editor does not change; edit the raw file")]
    Unsupported(String),
    #[error(
        "the edit would change more than asked (the file's layout was misread); \
         edit the raw file instead"
    )]
    Drift,
}

/// One list item of a rebuilt list.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// The old item `i`, its text and its comments as they were.
    Keep(usize),
    /// The old item `i`'s comment lines above it, with a new value.
    Retext(usize, Value),
    /// A new item.
    New(Value),
}

/// One change.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Set a key of a mapping (added at the mapping's end when absent).
    Set { path: Vec<Seg>, value: Value },
    /// Remove a key of a mapping.
    Remove { path: Vec<Seg> },
    /// Rebuild a list from old items and new ones, in the order given.
    Seq { path: Vec<Seg>, items: Vec<Item> },
}

/// Apply `ops` to `text`, in order, and check the result (module doc).
pub fn edit(text: &str, ops: &[Op]) -> Result<String, EditError> {
    let mut expected: Value =
        serde_yaml::from_str(text).map_err(|e| EditError::Parse(e.to_string()))?;
    let mut doc = Doc::parse(text);
    for op in ops {
        apply_value(&mut expected, op)?;
        doc.apply(op)?;
    }
    let out = doc.text();
    let got: Value = serde_yaml::from_str(&out).map_err(|_| EditError::Drift)?;
    if got != expected {
        return Err(EditError::Drift);
    }
    Ok(out)
}

/// The parsed value at `p`, if there is one.
pub fn value_at<'a>(root: &'a Value, p: &[Seg]) -> Option<&'a Value> {
    let mut v = root;
    for s in p {
        v = match (s, v) {
            (Seg::Key(k), Value::Mapping(m)) => m.get(Value::String(k.clone()))?,
            (Seg::Index(i), Value::Sequence(q)) => q.get(*i)?,
            _ => return None,
        };
    }
    Some(v)
}

fn value_at_mut<'a>(root: &'a mut Value, p: &[Seg]) -> Option<&'a mut Value> {
    let mut v = root;
    for s in p {
        v = match (s, v) {
            (Seg::Key(k), Value::Mapping(m)) => m.get_mut(Value::String(k.clone()))?,
            (Seg::Index(i), Value::Sequence(q)) => q.get_mut(*i)?,
            _ => return None,
        };
    }
    Some(v)
}

fn apply_value(root: &mut Value, op: &Op) -> Result<(), EditError> {
    match op {
        Op::Set { path, value } => {
            let (last, parent) = split_key(path)?;
            match value_at_mut(root, parent) {
                Some(Value::Mapping(m)) => {
                    m.insert(Value::String(last.to_string()), value.clone());
                    Ok(())
                }
                _ => Err(EditError::NotFound(path_text(parent))),
            }
        }
        Op::Remove { path } => {
            let (last, parent) = split_key(path)?;
            match value_at_mut(root, parent) {
                Some(Value::Mapping(m)) => m
                    .remove(Value::String(last.to_string()))
                    .map(|_| ())
                    .ok_or_else(|| EditError::NotFound(path_text(path))),
                _ => Err(EditError::NotFound(path_text(parent))),
            }
        }
        Op::Seq { path, items } => {
            let slot =
                value_at_mut(root, path).ok_or_else(|| EditError::NotFound(path_text(path)))?;
            let old: Vec<Value> = match slot {
                Value::Sequence(q) => q.clone(),
                Value::Null => Vec::new(),
                _ => return Err(EditError::Unsupported(path_text(path))),
            };
            let mut new = Vec::new();
            for it in items {
                new.push(match it {
                    Item::Keep(i) => old
                        .get(*i)
                        .cloned()
                        .ok_or_else(|| EditError::NotFound(format!("{}.{}", path_text(path), i)))?,
                    Item::Retext(i, v) => {
                        if *i >= old.len() {
                            return Err(EditError::NotFound(format!("{}.{}", path_text(path), i)));
                        }
                        v.clone()
                    }
                    Item::New(v) => v.clone(),
                });
            }
            *slot = Value::Sequence(new);
            Ok(())
        }
    }
}

fn split_key(p: &[Seg]) -> Result<(&str, &[Seg]), EditError> {
    match p.split_last() {
        Some((Seg::Key(k), parent)) => Ok((k.as_str(), parent)),
        _ => Err(EditError::Unsupported(path_text(p))),
    }
}

// ── the text side ───────────────────────────────────────────────────────

struct Doc {
    lines: Vec<String>,
    newline_at_end: bool,
}

/// A mapping's lines: `[start, end)`, keys at column `col`. When `dash` is
/// set, that line is a list item's first line and its key sits after "- ".
#[derive(Debug, Clone, Copy)]
struct Map {
    start: usize,
    end: usize,
    col: usize,
    dash: Option<usize>,
}

/// A block list's lines, its dashes at column `col`.
#[derive(Debug, Clone, Copy)]
struct Seq {
    start: usize,
    end: usize,
    col: usize,
}

/// Where a key is: its line, where its inline value starts and ends (the
/// comment after it excluded), and the lines of its block value.
#[derive(Debug, Clone, Copy)]
struct KeyAt {
    line: usize,
    col: usize,
    value_start: usize,
    value_end: usize,
    child_start: usize,
    child_end: usize,
}

/// One list item: the comment lines above it, its own lines, and the
/// blank or comment lines after it up to the next item's comments (its
/// chunk), so keeping every item keeps the text exactly.
#[derive(Debug, Clone, Copy)]
struct ItemAt {
    lead: usize,
    start: usize,
    end: usize,
    chunk_end: usize,
}

fn indent(l: &str) -> usize {
    l.len() - l.trim_start_matches(' ').len()
}

fn quiet(l: &str) -> bool {
    let t = l.trim();
    t.is_empty() || t.starts_with('#')
}

fn is_dash(l: &str, col: usize) -> bool {
    indent(l) == col && (l[col..].starts_with("- ") || l[col..].trim_end() == "-")
}

/// The value part and the comment part of the text after a key's colon.
fn split_comment(s: &str) -> (usize, usize) {
    let bytes = s.as_bytes();
    let (mut single, mut double) = (false, false);
    let mut cut = s.len();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'#' if !single
                && !double
                && (i == 0 || bytes[i - 1] == b' ' || bytes[i - 1] == b'\t') =>
            {
                cut = i;
                break;
            }
            _ => {}
        }
    }
    let value = &s[..cut];
    let start = value.len() - value.trim_start().len();
    let end = value.trim_end().len();
    (start, end.max(start))
}

impl Doc {
    fn parse(text: &str) -> Doc {
        let newline_at_end = text.ends_with('\n');
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Doc {
            lines,
            newline_at_end,
        }
    }

    fn text(&self) -> String {
        let mut s = self.lines.join("\n");
        if self.newline_at_end {
            s.push('\n');
        }
        s
    }

    fn root(&self) -> Map {
        Map {
            start: 0,
            end: self.lines.len(),
            col: self
                .lines
                .iter()
                .find(|l| !quiet(l))
                .map(|l| indent(l))
                .unwrap_or(0),
            dash: None,
        }
    }

    /// The last line of `[start, end)` that is not blank or a comment, + 1.
    fn trim_end(&self, start: usize, end: usize) -> usize {
        let mut e = end;
        while e > start && quiet(&self.lines[e - 1]) {
            e -= 1;
        }
        e
    }

    fn find_key(&self, m: Map, key: &str) -> Option<KeyAt> {
        for i in m.start..m.end {
            let l = &self.lines[i];
            let (col, rest) = if m.dash == Some(i) {
                (m.col, l.get(m.col..).unwrap_or(""))
            } else {
                if quiet(l) {
                    continue;
                }
                (indent(l), l.trim_start())
            };
            if col != m.col {
                continue;
            }
            let Some(after) = rest.strip_prefix(key) else {
                continue;
            };
            let Some(tail) = after.strip_prefix(':') else {
                continue;
            };
            if !(tail.is_empty() || tail.starts_with(' ')) {
                continue;
            }
            let colon = l.len() - tail.len();
            let (vs, ve) = split_comment(tail);
            let inline = tail[vs..ve].trim();
            let block_scalar = inline.starts_with('|') || inline.starts_with('>');
            let mut last = i + 1;
            let mut j = i + 1;
            while j < m.end {
                let lj = &self.lines[j];
                if lj.trim().is_empty() {
                    j += 1;
                    continue;
                }
                let deeper = indent(lj) > col;
                if block_scalar {
                    if !deeper {
                        break;
                    }
                    last = j + 1;
                } else if quiet(lj) {
                    // A comment inside the value counts only if content
                    // follows it.
                } else if deeper || (inline.is_empty() && is_dash(lj, col)) {
                    last = j + 1;
                } else {
                    break;
                }
                j += 1;
            }
            return Some(KeyAt {
                line: i,
                col,
                value_start: colon + vs,
                value_end: colon + ve,
                child_start: i + 1,
                child_end: last,
            });
        }
        None
    }

    fn inline<'a>(&'a self, k: &KeyAt) -> &'a str {
        &self.lines[k.line][k.value_start..k.value_end]
    }

    /// The mapping a key's block value is.
    fn child_map(&self, k: &KeyAt, what: &[Seg]) -> Result<Map, EditError> {
        if !self.inline(k).is_empty() {
            return Err(EditError::Unsupported(path_text(what)));
        }
        let first = (k.child_start..k.child_end)
            .find(|&j| !quiet(&self.lines[j]))
            .ok_or_else(|| EditError::NotFound(path_text(what)))?;
        let col = indent(&self.lines[first]);
        if is_dash(&self.lines[first], col) {
            return Err(EditError::Unsupported(path_text(what)));
        }
        Ok(Map {
            start: k.child_start,
            end: k.child_end,
            col,
            dash: None,
        })
    }

    /// The block list a key's value is; None when the key has no block
    /// value (an empty or flow value).
    fn child_seq(&self, k: &KeyAt) -> Option<Seq> {
        let first = (k.child_start..k.child_end).find(|&j| !quiet(&self.lines[j]))?;
        let col = indent(&self.lines[first]);
        is_dash(&self.lines[first], col).then_some(Seq {
            start: k.child_start,
            end: k.child_end,
            col,
        })
    }

    fn items(&self, s: Seq) -> Vec<ItemAt> {
        let dashes: Vec<usize> = (s.start..s.end)
            .filter(|&j| is_dash(&self.lines[j], s.col))
            .collect();
        let mut out: Vec<ItemAt> = Vec::new();
        for (n, &d) in dashes.iter().enumerate() {
            let next = dashes.get(n + 1).copied().unwrap_or(s.end);
            let end = self.trim_end(d + 1, next).max(d + 1);
            let floor = out.last().map(|i| i.end).unwrap_or(s.start);
            let mut lead = d;
            while lead > floor && self.lines[lead - 1].trim_start().starts_with('#') {
                lead -= 1;
            }
            if let Some(prev) = out.last_mut() {
                prev.chunk_end = lead;
            }
            out.push(ItemAt {
                lead,
                start: d,
                end,
                chunk_end: end,
            });
        }
        out
    }

    /// The mapping a list item is.
    fn item_map(&self, s: Seq, it: ItemAt, what: &[Seg]) -> Result<Map, EditError> {
        let l = &self.lines[it.start];
        if l[s.col..].trim_end() == "-" {
            let first = (it.start + 1..it.end)
                .find(|&j| !quiet(&self.lines[j]))
                .ok_or_else(|| EditError::Unsupported(path_text(what)))?;
            return Ok(Map {
                start: it.start + 1,
                end: it.end,
                col: indent(&self.lines[first]),
                dash: None,
            });
        }
        Ok(Map {
            start: it.start,
            end: it.end,
            col: s.col + 2,
            dash: Some(it.start),
        })
    }

    /// Walk `p` to a mapping.
    fn map_at(&self, p: &[Seg]) -> Result<Map, EditError> {
        let mut m = self.root();
        let mut i = 0;
        while i < p.len() {
            let Seg::Key(k) = &p[i] else {
                return Err(EditError::Unsupported(path_text(&p[..=i])));
            };
            let at = self
                .find_key(m, k)
                .ok_or_else(|| EditError::NotFound(path_text(&p[..=i])))?;
            match p.get(i + 1) {
                Some(Seg::Index(n)) => {
                    let s = self
                        .child_seq(&at)
                        .ok_or_else(|| EditError::Unsupported(path_text(&p[..=i])))?;
                    let items = self.items(s);
                    let it = *items
                        .get(*n)
                        .ok_or_else(|| EditError::NotFound(path_text(&p[..i + 2])))?;
                    m = self.item_map(s, it, &p[..i + 2])?;
                    i += 2;
                }
                _ => {
                    m = self.child_map(&at, &p[..=i])?;
                    i += 1;
                }
            }
        }
        Ok(m)
    }

    fn apply(&mut self, op: &Op) -> Result<(), EditError> {
        match op {
            Op::Set { path, value } => self.set(path, value),
            Op::Remove { path } => {
                let (last, parent) = split_key(path)?;
                let m = self.map_at(parent)?;
                let at = self
                    .find_key(m, last)
                    .ok_or_else(|| EditError::NotFound(path_text(path)))?;
                if m.dash == Some(at.line) {
                    // The item's first key: the item would lose its dash.
                    return Err(EditError::Unsupported(path_text(path)));
                }
                self.lines.drain(at.line..at.child_end);
                Ok(())
            }
            Op::Seq { path, items } => self.rebuild(path, items),
        }
    }

    fn set(&mut self, p: &[Seg], value: &Value) -> Result<(), EditError> {
        let (last, parent) = split_key(p)?;
        let m = self.map_at(parent)?;
        match self.find_key(m, last) {
            Some(at) => {
                let (inline, children) = render_value(value, at.col);
                let line = &self.lines[at.line];
                let mut new = format!("{}{}", &line[..at.value_start], inline);
                let rest = &line[at.value_end..];
                if inline.is_empty() {
                    new = new.trim_end().to_string();
                    new.push_str(rest);
                } else {
                    if at.value_start == at.value_end && !line[..at.value_start].ends_with(' ') {
                        new = format!("{} {}", &line[..at.value_start], inline);
                    }
                    new.push_str(rest);
                }
                self.lines[at.line] = new.trim_end().to_string();
                self.lines.splice(at.child_start..at.child_end, children);
            }
            None => {
                let end = self
                    .trim_end(m.start, m.end)
                    .max(m.start + usize::from(m.dash.is_some()));
                let (inline, children) = render_value(value, m.col);
                let mut add = vec![format!(
                    "{}{}:{}{}",
                    " ".repeat(m.col),
                    last,
                    if inline.is_empty() { "" } else { " " },
                    inline
                )];
                add.extend(children);
                self.lines.splice(end..end, add);
            }
        }
        Ok(())
    }

    fn rebuild(&mut self, p: &[Seg], items: &[Item]) -> Result<(), EditError> {
        let (last, parent) = split_key(p)?;
        let m = self.map_at(parent)?;
        let at = self
            .find_key(m, last)
            .ok_or_else(|| EditError::NotFound(path_text(p)))?;
        let mut inline = self.inline(&at).to_string();
        let all_scalar = items.iter().all(|it| match it {
            Item::Keep(_) => true,
            Item::Retext(_, v) | Item::New(v) => scalar(v).is_some(),
        });
        if inline == "[]" && !all_scalar {
            // An empty flow list that gets a table becomes a block list.
            inline = "null".into();
        } else if inline.starts_with('[') {
            return self.rebuild_flow(p, &at, &inline, items);
        }
        if !inline.is_empty() && inline != "null" && inline != "~" {
            return Err(EditError::Unsupported(path_text(p)));
        }
        let (seq, old): (Seq, Vec<ItemAt>) = match self.child_seq(&at) {
            Some(s) => (s, self.items(s)),
            None => {
                if (at.child_start..at.child_end).any(|j| !quiet(&self.lines[j])) {
                    return Err(EditError::Unsupported(path_text(p)));
                }
                (
                    Seq {
                        start: at.child_start,
                        end: at.child_start,
                        col: at.col + 2,
                    },
                    Vec::new(),
                )
            }
        };
        let mut out: Vec<String> = Vec::new();
        for it in items {
            match it {
                Item::Keep(i) => {
                    let o = old
                        .get(*i)
                        .ok_or_else(|| EditError::NotFound(format!("{}.{}", path_text(p), i)))?;
                    out.extend(self.lines[o.lead..o.chunk_end].iter().cloned());
                }
                Item::Retext(i, v) => {
                    let o = old
                        .get(*i)
                        .ok_or_else(|| EditError::NotFound(format!("{}.{}", path_text(p), i)))?;
                    out.extend(self.lines[o.lead..o.start].iter().cloned());
                    out.extend(render_item(v, seq.col));
                    out.extend(self.lines[o.end..o.chunk_end].iter().cloned());
                }
                Item::New(v) => out.extend(render_item(v, seq.col)),
            }
        }
        // What sat between the items (blank lines, loose comments) goes;
        // what followed the last one (a trailing comment block) stays.
        let (from, to) = match (old.first(), old.last()) {
            (Some(f), Some(l)) => (f.lead, l.chunk_end),
            _ => (seq.start, seq.end),
        };
        if inline == "null" || inline == "~" {
            let line = &self.lines[at.line];
            self.lines[at.line] = format!("{}{}", &line[..at.value_start], &line[at.value_end..])
                .trim_end()
                .to_string();
        }
        if out.is_empty() && old.is_empty() {
            return Ok(());
        }
        if out.is_empty() {
            // An empty list keeps its key: `key: []`.
            let line = &self.lines[at.line];
            self.lines[at.line] = format!(
                "{} []{}",
                line[..at.value_start].trim_end(),
                &line[at.value_end..]
            );
        }
        self.lines.splice(from..to, out);
        Ok(())
    }

    fn rebuild_flow(
        &mut self,
        p: &[Seg],
        at: &KeyAt,
        inline: &str,
        items: &[Item],
    ) -> Result<(), EditError> {
        let old: Vec<Value> = match serde_yaml::from_str::<Value>(inline) {
            Ok(Value::Sequence(q)) => q,
            _ => return Err(EditError::Unsupported(path_text(p))),
        };
        let mut parts = Vec::new();
        for it in items {
            let v = match it {
                Item::Keep(i) => old
                    .get(*i)
                    .cloned()
                    .ok_or_else(|| EditError::NotFound(format!("{}.{}", path_text(p), i)))?,
                Item::Retext(_, v) | Item::New(v) => v.clone(),
            };
            match scalar(&v) {
                Some(s) => parts.push(s),
                None => return Err(EditError::Unsupported(path_text(p))),
            }
        }
        let line = &self.lines[at.line];
        self.lines[at.line] = format!(
            "{}[{}]{}",
            &line[..at.value_start],
            parts.join(", "),
            &line[at.value_end..]
        );
        Ok(())
    }
}

/// A scalar on one line, as YAML writes it; None for anything that needs
/// more than one line.
fn scalar(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some("null".into()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.contains('\n') => serde_yaml::to_string(v)
            .ok()
            .map(|t| t.trim_end_matches('\n').to_string()),
        Value::Sequence(q) if q.is_empty() => Some("[]".into()),
        Value::Mapping(m) if m.is_empty() => Some("{}".into()),
        _ => None,
    }
}

/// A value after `key:` at column `col`: what goes on the key's line, and
/// the lines below it.
fn render_value(v: &Value, col: usize) -> (String, Vec<String>) {
    if let Some(s) = scalar(v) {
        return (s, Vec::new());
    }
    let pad = " ".repeat(col + 2);
    match v {
        Value::String(s) => {
            let (head, body) = match s.strip_suffix('\n') {
                Some(b) if !b.ends_with('\n') => ("|", b),
                Some(_) => return (quoted(s), Vec::new()),
                None => ("|-", s.as_str()),
            };
            if body.lines().any(|l| l.starts_with(' ')) {
                return (quoted(s), Vec::new());
            }
            let lines = body
                .split('\n')
                .map(|l| {
                    if l.is_empty() {
                        String::new()
                    } else {
                        format!("{pad}{l}")
                    }
                })
                .collect();
            (head.into(), lines)
        }
        Value::Mapping(m) => (String::new(), render_entries(m, col + 2)),
        Value::Sequence(q) => (
            String::new(),
            q.iter().flat_map(|x| render_item(x, col + 2)).collect(),
        ),
        Value::Tagged(t) => render_value(&t.value, col),
        _ => (String::new(), Vec::new()),
    }
}

fn quoted(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn render_entries(m: &Mapping, col: usize) -> Vec<String> {
    let pad = " ".repeat(col);
    let mut out = Vec::new();
    for (k, v) in m {
        let key = scalar(k).unwrap_or_default();
        let (inline, children) = render_value(v, col);
        out.push(if inline.is_empty() {
            format!("{pad}{key}:")
        } else {
            format!("{pad}{key}: {inline}")
        });
        out.extend(children);
    }
    out
}

/// A list item at column `col`: a mapping starts on the dash line.
fn render_item(v: &Value, col: usize) -> Vec<String> {
    let pad = " ".repeat(col);
    match v {
        Value::Mapping(m) if !m.is_empty() => {
            let mut lines = render_entries(m, col + 2);
            if let Some(first) = lines.first_mut() {
                *first = format!("{pad}- {}", &first[col + 2..]);
            }
            lines
        }
        _ => {
            let (inline, children) = render_value(v, col);
            let mut lines = vec![format!("{pad}- {inline}").trim_end().to_string()];
            lines.extend(children);
            lines
        }
    }
}
