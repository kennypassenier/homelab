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

/// One entry: the app it belongs to, the hostname its route matches, and
/// the LAN address the route forwards to.
///
/// The backend was deliberately dropped when this was first written — the
/// front page shows the names Kenny types, not internal addresses. A WIDGET
/// needs the internal one though, and for the reason he wrote in his own
/// file: a widget that goes out through Cloudflare Access gets the login
/// page instead of an answer. So the backend is carried, and used for
/// widgets only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub app: String,
    pub host: String,
    /// e.g. `http://10.10.10.6:8096` — None when the fragment names no
    /// server for this router.
    pub backend: Option<String>,
}

/// Pull `(router-name, hostname)` out of a Traefik route fragment.
///
/// A line scanner rather than a yaml parse, for the same reason the
/// `/appdata` check next to it is one: these files are written by this
/// program from a template, and the two agree about what a route looks like.
pub fn entries_from_route(content: &str) -> Vec<Entry> {
    // Two passes: the routers give (name, hostname), the services give
    // (name, backend url). A router names its service explicitly, and in
    // every fragment this orchestrator writes the two share a name.
    let mut backends: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current = String::new();
    let mut in_services = false;
    for line in content.lines() {
        let t = line.trim();
        if t == "services:" {
            in_services = true;
            continue;
        }
        if t == "routers:" {
            in_services = false;
            continue;
        }
        if !in_services || t.starts_with('#') || t.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 4 && t.ends_with(':') && !t.contains(' ') {
            current = t.trim_end_matches(':').to_string();
            continue;
        }
        if let Some(rest) = t.strip_prefix("- url:") {
            let url = rest.trim().trim_matches('"').trim_matches('\'').to_string();
            if !current.is_empty() && !url.is_empty() {
                backends.entry(current.clone()).or_insert(url);
            }
        }
    }

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
                        backend: backends.get(&app).cloned(),
                    });
                }
            }
        }
    }
    out
}

/// V6b (Kenny, 2026-09-02): "kan dat niet automatisch?"
///
/// A Homepage widget needs three things, and all three turned out to be
/// readable without anyone typing them:
///
/// * **type** — what Homepage calls the app. Equal to the route name for
///   seven of the nine widgets Kenny had; the other two are aliases and are
///   spelled out below.
/// * **url** — the LAN address. It is the backend the route already
///   forwards to, which is exactly the address a widget must use: through
///   the public name it would meet Cloudflare Access and get a login page.
/// * **key** — an API key. NOT a thing to store beside the app: every one
///   of these applications already keeps its own key on disk, in the config
///   directory this orchestrator mounts and backs up. Reading it there is
///   also the only reading that cannot go stale — the same lesson as F32,
///   where a key copied into an `.env` had been dead for an unknown length
///   of time while everything reported fine.
///
/// This table is knowledge about third-party software rather than a second
/// copy of anything in this fleet, which is why it is allowed to be written
/// down (Kenny's rule, 2026-09-02: never write what the system already
/// knows). It sits in code so a test can reach it.
pub struct WidgetSpec {
    /// The route name, which is also the container's service name.
    pub app: &'static str,
    /// What Homepage calls this widget.
    pub kind: &'static str,
    /// Shell that prints the API key and nothing else, run INSIDE the
    /// container that owns the app. `{dir}` is its config directory.
    /// None = the widget needs a credential the application does not store
    /// for us, and it comes from latch through Homepage's own `.env`.
    pub key_cmd: Option<&'static str>,
    /// Extra lines for the widget block, verbatim.
    pub extra: &'static [&'static str],
}

pub const KNOWN_WIDGETS: &[WidgetSpec] = &[
    WidgetSpec {
        app: "jellyfin",
        kind: "jellyfin",
        // The same reading the busy check already does, for the same reason:
        // ask the application which keys it accepts.
        key_cmd: Some("sqlite3 {dir}/data/jellyfin.db 'select AccessToken from ApiKeys limit 1'"),
        extra: &["enableNowPlaying: true", "enableBlocks: true"],
    },
    WidgetSpec {
        app: "sonarr",
        kind: "sonarr",
        key_cmd: Some("sed -n 's|.*<ApiKey>\\(.*\\)</ApiKey>.*|\\1|p' {dir}/config.xml"),
        extra: &[],
    },
    WidgetSpec {
        app: "radarr",
        kind: "radarr",
        key_cmd: Some("sed -n 's|.*<ApiKey>\\(.*\\)</ApiKey>.*|\\1|p' {dir}/config.xml"),
        extra: &[],
    },
    WidgetSpec {
        app: "prowlarr",
        kind: "prowlarr",
        key_cmd: Some("sed -n 's|.*<ApiKey>\\(.*\\)</ApiKey>.*|\\1|p' {dir}/config.xml"),
        extra: &[],
    },
    WidgetSpec {
        app: "bazarr",
        kind: "bazarr",
        key_cmd: Some("sed -n 's/^ *apikey: *//p' {dir}/config/config.yaml | head -1"),
        extra: &[],
    },
    // Route name and widget name differ: the route is `seerr`, the software
    // Homepage knows is `jellyseerr`.
    WidgetSpec {
        app: "seerr",
        kind: "jellyseerr",
        key_cmd: Some(
            "sed -n 's/.*\"apiKey\": *\"\\([^\"]*\\)\".*/\\1/p' {dir}/settings.json | head -1",
        ),
        extra: &[],
    },
    WidgetSpec {
        app: "paperless",
        kind: "paperlessngx",
        // Paperless issues tokens per user through its API rather than
        // keeping one on disk; Homepage reads it from its own .env.
        key_cmd: None,
        extra: &[],
    },
];

pub fn widget_for(app: &str) -> Option<&'static WidgetSpec> {
    KNOWN_WIDGETS.iter().find(|w| w.app == app)
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
    /// Keep this route off the front page. Not every route is a front door:
    /// `uptime-kuma-alt` is a second name for a service already listed, and
    /// `homepage` is the page you are looking at.
    pub hide: bool,
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
        } else if let Some(v) = t.strip_prefix("hide:") {
            b.hide = v.trim() == "true";
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
/// The widget block for one entry, or nothing when this app has none.
///
/// A widget without its key is emitted anyway: Homepage then draws the tile
/// and reports its own error, which is visible. Leaving the widget out
/// entirely would look exactly like an app that has no widget, and that is
/// the difference between "broken" and "nothing to show" disappearing again.
fn widget_lines(e: &Entry, keys: &std::collections::HashMap<String, String>) -> Vec<String> {
    let Some(spec) = widget_for(&e.app) else {
        return Vec::new();
    };
    let Some(url) = e.backend.as_deref() else {
        return Vec::new();
    };
    let mut out = vec![
        "widget:".to_string(),
        format!("  type: {}", spec.kind),
        format!("  url: {}", url),
    ];
    match keys.get(&e.app) {
        Some(k) if !k.is_empty() => out.push(format!("  key: {}", k)),
        // No key on disk: fall back to Homepage's own variable, which is
        // how the credentials that no application stores for us arrive.
        _ => out.push(format!(
            "  key: {{{{HOMEPAGE_VAR_{}}}}}",
            e.app.to_uppercase().replace('-', "_")
        )),
    }
    for x in spec.extra {
        out.push(format!("  {}", x));
    }
    out
}

pub fn services_yaml(
    stacks: &[(String, Vec<Entry>)],
    overlay: Option<&Overlay>,
    widget_keys: &std::collections::HashMap<String, String>,
) -> String {
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
                for l in widget_lines(e, widget_keys) {
                    out.push_str(&format!("        {}\n", l));
                }
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
    // Two routers may forward to the same door — `almanac` and
    // `almanac-block-metrics` both answer on almanac.kp-soft.dev. The page
    // lists destinations, not routers, so the second one is not a second
    // tile.
    let mut seen_href: Vec<String> = Vec::new();
    for (stack, entries) in stacks {
        for e in entries {
            let href = format!("https://{}/", e.host);
            if seen_href.contains(&href_key(&href)) {
                continue;
            }
            seen_href.push(href_key(&href));
            let blk = ov.blocks.iter().find(|b| b.href == href_key(&href));
            if blk.is_some_and(|b| b.hide) {
                used.push(href_key(&href));
                continue;
            }
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
            let w = widget_lines(e, widget_keys);
            match blk {
                Some(b) if !b.extra.is_empty() => {
                    // The overlay's own lines first — icon, description and
                    // anything Homepage understands that this code does not.
                    // A widget it spells out by hand wins: that is a choice,
                    // and a generated one must not silently overrule it.
                    let hand_widget = b.extra.iter().any(|l| l.trim() == "widget:");
                    for l in &b.extra {
                        if l.trim().is_empty() {
                            continue;
                        }
                        body.push_str(&format!("    {}\n", l));
                    }
                    if !hand_widget {
                        for l in &w {
                            body.push_str(&format!("        {}\n", l));
                        }
                    }
                }
                // No overlay entry: a plain link plus the reachability dot,
                // which is all the orchestrator can honestly say about a
                // service nobody has described yet.
                _ => {
                    body.push_str(&format!("        siteMonitor: https://{}/\n", e.host));
                    for l in &w {
                        body.push_str(&format!("        {}\n", l));
                    }
                }
            }
            push(&mut groups, &group, body);
        }
    }

    // Overlay blocks that joined nothing are deliberate manual links — the
    // one http:// entry on the page, and a deep link into Grafana for a
    // service that has no page of its own. Dropping them because no route
    // matched would delete a decision, not a stale entry.
    for b in &ov.blocks {
        if used.contains(&b.href) || b.hide {
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
