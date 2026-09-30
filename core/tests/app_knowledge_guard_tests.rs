//! app-knowledge (Kenny, 2026-09-30: "de code moet niks afweten van de apps
//! die ze beheert" — "ALLES staat in de declaratieve files vanaf nu").
//!
//! Everything homelab knows about an app lives in that app's stack files:
//! its tiles (`homepage_widgets`), its busy check, its probes, its secret
//! files, its restore note. This guard reads every stack and app name the
//! repository declares and fails when one appears in the code, outside
//! comments and tests. A new app then needs its files, never a code change.
//!
//! PLATFORM names the code may use: the software homelab itself feeds or
//! reads for every stack, configured in host.toml, and its own dashboard.

const PLATFORM: &[(&str, &str)] = &[
    ("admin", "the dashboard's own identity and settings table"),
    ("home", "the house's public address, and $HOME"),
    ("drill", "the restore drill, a platform operation"),
    (
        "actual",
        "an ordinary English word in code (expected vs actual)",
    ),
    (
        "gateway",
        "the edge platform: route files, gateway_vmid in host.toml",
    ),
    (
        "metrics",
        "the platform's metrics targets, metrics_targets_dir",
    ),
    (
        "registry",
        "the registry cache homelab pulls through, host.toml",
    ),
    (
        "uptime",
        "the monitor list homelab writes, kuma_monitors_file",
    ),
    ("loki", "the log store homelab queries, loki_url"),
    (
        "grafana",
        "the dashboards homelab writes, grafana_dashboards_dir",
    ),
    (
        "prometheus",
        "the metrics store homelab queries, prometheus_url",
    ),
    ("traefik", "the route format homelab writes for every stack"),
    (
        "homepage",
        "the front-page file homelab writes, homepage_services_file",
    ),
    (
        "alertmanager",
        "the alert webhook format the dashboard receives",
    ),
];

fn declared_names() -> Vec<String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../stacks");
    let mut out = Vec::new();
    for stack in std::fs::read_dir(&root).unwrap().flatten() {
        if !stack.path().is_dir() {
            continue;
        }
        out.push(stack.file_name().to_string_lossy().to_string());
        for app in std::fs::read_dir(stack.path()).unwrap().flatten() {
            let name = app.file_name().to_string_lossy().to_string();
            if app.path().is_dir() && name != "routes" && name != "rootfs" {
                out.push(name);
            }
        }
    }
    out.sort();
    out.dedup();
    out.retain(|n| !PLATFORM.iter().any(|(p, _)| p == n));
    out
}

fn sources(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name()
                .is_some_and(|n| n == "node_modules" || n == "test")
            {
                continue;
            }
            sources(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs" || x == "js") {
            out.push(p);
        }
    }
}

fn mentions(line: &str, name: &str) -> bool {
    let word = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    line.match_indices(name).any(|(i, _)| {
        let before = line[..i].chars().next_back();
        let after = line[i + name.len()..].chars().next();
        !before.is_some_and(word) && !after.is_some_and(word)
    })
}

#[test]
fn app_knowledge_no_declared_app_or_stack_is_named_in_the_code() {
    let names = declared_names();
    assert!(names.len() > 20, "the stack sweep broke: {:?}", names);
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    for dir in [
        "core/src",
        "host/src",
        "client/src",
        "admin/src",
        "admin/web/js",
        "proto/src",
    ] {
        sources(&repo.join(dir), &mut files);
    }
    assert!(files.len() > 50, "only {} files scanned", files.len());
    let mut hits = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let t = line.trim_start();
            // A `#[cfg(test)] mod …` runs to the end of its file here.
            if t.starts_with("#[cfg(test)]")
                && lines
                    .get(i + 1)
                    .is_some_and(|n| n.trim_start().starts_with("mod "))
            {
                break;
            }
            if t.starts_with("//") || t.starts_with('*') || t.starts_with("/*") {
                continue;
            }
            let code = line.split(" // ").next().unwrap_or(line);
            for n in &names {
                if mentions(code, n) {
                    let rel = f.strip_prefix(&repo).unwrap_or(&f).display().to_string();
                    hits.push(format!("{}:{}: `{}` in {}", rel, i + 1, n, t.trim()));
                }
            }
        }
    }
    assert!(
        hits.is_empty(),
        "the code names an app or stack the stack files declare; put what it knows in \
         that app's files instead (homepage_widgets, busy_check, probes, latch_files, \
         restore_note, ...):\n{}",
        hits.join("\n")
    );
}
