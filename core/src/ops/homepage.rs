//! T51: the front page is a consequence of the fleet, not an administration.
//!
//! Homepage listed every service by hand in `services.yaml`, and on
//! 2026-09-01 that file was zero bytes — every config file under
//! `homepage-config/` held not a single entry. The page answered 200 and
//! showed nothing, which is why nobody noticed: an empty dashboard looks
//! exactly like a dashboard with nothing wrong.
//!
//! The same shape as T1 and T2: the orchestrator already knows every stack
//! and every route it wrote, so the page is rendered rather than maintained,
//! and a new stack cannot be missing from it.
//!
//! What is listed is the ROUTES — the names Kenny actually types — rather
//! than internal addresses. A service without a route is not on the front
//! page, which is correct: the front page is the front door.

/// One entry: the app it belongs to and the hostname its route matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub app: String,
    pub host: String,
}

/// Pull `(router-name, hostname)` out of a Traefik route fragment.
///
/// A line scanner rather than a yaml parse, for the same reason the
/// `/appdata` check next to it is one: these files are written by this
/// program from a template, and the two agree about what a route looks like.
pub fn entries_from_route(content: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut app = String::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        // `    name:` under routers — four spaces, no space in the key.
        let indent = line.len() - line.trim_start().len();
        if indent == 4 && t.ends_with(':') && !t.contains(' ') {
            app = t.trim_end_matches(':').to_string();
            continue;
        }
        if let Some(rest) = t.strip_prefix("rule:") {
            if let Some(h) = rest.split('`').nth(1) {
                if !app.is_empty() && h.contains('.') {
                    out.push(Entry {
                        app: app.clone(),
                        host: h.to_string(),
                    });
                }
            }
        }
    }
    out
}

/// V6 (Kenny, 2026-09-02): what the orchestrator cannot derive from a route.
///
/// The generated list knows every service that has a gateway route and
/// nothing else — no icon, no description, no widget, and no opinion about
/// which services belong together. Kenny had all of that in a hand-written
/// `services.yaml`, which the compose then mounted read-only over the top of
/// the generated one, so the generated half had never been visible (F188).
///
/// Rather than pick a winner, the two are joined on `href`, which both sides
/// already carry. An exact key on purpose: matching display names was
/// measured against the real pair of files and would have guessed wrong
/// eight times out of twenty-six ("actual" vs "Actual Budget", "stirling"
/// vs "Stirling PDF"). Twenty-two of his twenty-four entries join exactly;
/// the two that do not are deliberate manual links with no route at all, and
/// they are kept rather than dropped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// Groups rendered first, in this order. Anything else follows,
    /// alphabetically, so a new stack lands somewhere predictable.
    pub group_order: Vec<String>,
    pub blocks: Vec<OverlayBlock>,
}

/// One overlay entry. `extra` is passed through to Homepage verbatim, so a
/// field this orchestrator has never heard of still works.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayBlock {
    pub href: String,
    pub group: Option<String>,
    pub name: Option<String>,
    pub extra: Vec<String>,
}

/// Trailing slashes are noise on a join key: a route yields
/// `https://fin.kp-soft.dev/` and a hand-written file may or may not have
/// written the slash.
fn href_key(h: &str) -> String {
    h.trim().trim_end_matches('/').to_string()
}

/// A line scanner, for the same reason `entries_from_route` is one: this
/// file has a shape the orchestrator itself documents, and a yaml dependency
/// in core has been declined before.
pub fn parse_overlay(text: &str) -> Overlay {
    let mut ov = Overlay::default();
    let mut cur: Option<OverlayBlock> = None;
    let mut in_extra = false;

    for line in text.lines() {
        if in_extra {
            // Extra continues while the line is indented deeper than the
            // block's own keys, or blank.
            if line.trim().is_empty() {
                if let Some(b) = cur.as_mut() {
                    b.extra.push(String::new());
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("    ") {
                if let Some(b) = cur.as_mut() {
                    b.extra.push(rest.to_string());
                }
                continue;
            }
            in_extra = false;
        }

        let t = line.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        if let Some(rest) = t.strip_prefix("group_order:") {
            ov.group_order = rest
                .trim()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            continue;
        }
        if let Some(href) = t.strip_prefix("- href:") {
            if let Some(b) = cur.take() {
                ov.blocks.push(b);
            }
            cur = Some(OverlayBlock {
                href: href_key(href),
                ..Default::default()
            });
            continue;
        }
        let Some(b) = cur.as_mut() else { continue };
        if let Some(v) = t.strip_prefix("group:") {
            b.group = Some(v.trim().to_string());
        } else if let Some(v) = t.strip_prefix("name:") {
            b.name = Some(v.trim().to_string());
        } else if t == "extra: |" {
            in_extra = true;
        }
    }
    if let Some(b) = cur.take() {
        ov.blocks.push(b);
    }
    // A block with no href joins nothing and would render as a nameless
    // entry — drop it rather than emit something nobody can act on.
    ov.blocks.retain(|b| !b.href.is_empty());
    ov
}

/// Render Homepage's `services.yaml` for the whole fleet.
///
/// One group per stack, in the order given, and the apps within a stack in
/// the order their routes appear. Stable output, so a deploy that changes
/// nothing writes a byte-identical file and the push reports "unchanged".
///
/// A stack with no routes is skipped entirely rather than rendered as an
/// empty group: an empty heading on a dashboard reads as "this is broken"
/// when it means "this has no front door".
pub fn services_yaml(stacks: &[(String, Vec<Entry>)], overlay: Option<&Overlay>) -> String {
    let mut out = String::from(
        "# Generated by the homelab orchestrator — every stack that has a\n\
         # gateway route appears here, joined on href with the overlay in\n\
         # services-overlay.yml for icons, descriptions, widgets and\n\
         # grouping. Edit the overlay or the stack's route file; edits here\n\
         # are overwritten on the next deploy.\n\
         #\n\
         # T51: this file was 0 bytes until 2026-09-01, so the front page\n\
         # rendered and listed nothing. V6: until 2026-09-02 the compose then\n\
         # mounted a hand-written copy read-only over the top of it, so this\n\
         # file was generated and never seen.\n",
    );

    // No overlay: the plain list, which is what the first version of T51
    // wrote and what a fleet without an overlay still gets.
    let Some(ov) = overlay else {
        for (stack, entries) in stacks {
            if entries.is_empty() {
                continue;
            }
            out.push_str(&format!("\n- {}:\n", stack));
            for e in entries {
                out.push_str(&format!(
                    "    - {}:\n        href: https://{}/\n        siteMonitor: https://{}/\n",
                    e.app, e.host, e.host
                ));
            }
        }
        return out;
    };

    // group -> rendered service blocks, in the order they are produced.
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    let push = |groups: &mut Vec<(String, Vec<String>)>, group: &str, body: String| match groups
        .iter_mut()
        .find(|(g, _)| g == group)
    {
        Some((_, v)) => v.push(body),
        None => groups.push((group.to_string(), vec![body])),
    };

    let mut used: Vec<String> = Vec::new();
    for (stack, entries) in stacks {
        for e in entries {
            let href = format!("https://{}/", e.host);
            let blk = ov.blocks.iter().find(|b| b.href == href_key(&href));
            if let Some(b) = blk {
                used.push(b.href.clone());
            }
            let group = blk
                .and_then(|b| b.group.clone())
                .unwrap_or_else(|| stack.clone());
            let name = blk
                .and_then(|b| b.name.clone())
                .unwrap_or_else(|| e.app.clone());
            let mut body = format!("    - {}:\n        href: {}\n", name, href);
            match blk {
                Some(b) if !b.extra.is_empty() => {
                    for l in &b.extra {
                        if l.trim().is_empty() {
                            continue;
                        }
                        body.push_str(&format!("    {}\n", l));
                    }
                }
                // No overlay entry: a plain link plus the reachability dot,
                // which is all the orchestrator can honestly say about a
                // service nobody has described yet.
                _ => body.push_str(&format!("        siteMonitor: https://{}/\n", e.host)),
            }
            push(&mut groups, &group, body);
        }
    }

    // Overlay blocks that joined nothing are deliberate manual links — the
    // one http:// entry on the page, and a deep link into Grafana for a
    // service that has no page of its own. Dropping them because no route
    // matched would delete a decision, not a stale entry.
    for b in &ov.blocks {
        if used.contains(&b.href) {
            continue;
        }
        let group = b.group.clone().unwrap_or_else(|| "Overig".into());
        let name = b.name.clone().unwrap_or_else(|| b.href.clone());
        let mut body = format!("    - {}:\n        href: {}/\n", name, b.href);
        for l in &b.extra {
            if l.trim().is_empty() {
                continue;
            }
            body.push_str(&format!("    {}\n", l));
        }
        push(&mut groups, &group, body);
    }

    // Named groups first in the order the overlay gives, then the rest
    // alphabetically so a new stack lands somewhere predictable.
    groups.sort_by_key(|(g, _)| {
        (
            ov.group_order
                .iter()
                .position(|x| x == g)
                .unwrap_or(usize::MAX),
            g.clone(),
        )
    });
    for (g, bodies) in groups {
        out.push_str(&format!("\n- {}:\n", g));
        for b in bodies {
            out.push_str(&b);
        }
    }
    out
}
