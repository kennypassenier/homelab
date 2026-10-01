//! TUI parity (`homelab templates`, C5): the host's `ListTemplates` answer,
//! read into the two lists the page and the template-build form show. Pure.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Templates {
    /// Golden templates a deploy clones: (vmid, name).
    pub clones: Vec<(u16, String)>,
    /// OS templates a build can bake from (`local:vztmpl/…`).
    pub os: Vec<String>,
}

/// The answer is two headed lists: `clone:<vmid>  <name>` lines, then
/// `  <vztmpl>` lines under "OS templates".
pub fn parse(text: &str) -> Templates {
    let mut out = Templates::default();
    let mut in_os = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with("OS templates") {
            in_os = true;
            continue;
        }
        if t.is_empty() || t.ends_with(':') || t.starts_with('(') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("clone:") {
            let mut parts = rest.split_whitespace();
            if let (Some(v), name) = (parts.next(), parts.next())
                && let Ok(vmid) = v.parse()
            {
                out.clones.push((vmid, name.unwrap_or("").to_string()));
            }
        } else if in_os && let Some(name) = t.split_whitespace().next() {
            out.os.push(name.to_string());
        }
    }
    out
}
