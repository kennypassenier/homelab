//! fix-92 (expert panel, routes-outside-repo-unvalidated, 2026-09-27): what
//! a route file claims and where it sends it, read with a real YAML parser.
//!
//! On the client because core parses no YAML (a deliberate choice, F200):
//! core judges the facts this produces, in
//! [`homelab_core::routes::fleet_route_problems`].

use homelab_core::routes::{RouteDecl, RouteFacts};

fn parse(content: &str) -> Result<serde_yaml::Value, String> {
    serde_yaml::from_str(content).map_err(|e| e.to_string())
}

/// Every name inside `Host(...)` in every router rule, lower-cased, in the
/// order they appear. `HostSNI` and `HostRegexp` are other matchers and are
/// not read as hostnames.
pub fn hostnames(content: &str) -> Result<Vec<String>, String> {
    let doc = parse(content)?;
    let mut out = Vec::new();
    for proto in ["http", "tcp"] {
        let Some(routers) = doc
            .get(proto)
            .and_then(|p| p.get("routers"))
            .and_then(|r| r.as_mapping())
        else {
            continue;
        };
        for (_, router) in routers {
            let Some(rule) = router.get("rule").and_then(|r| r.as_str()) else {
                continue;
            };
            let mut rest = rule;
            while let Some(at) = rest.find("Host(") {
                let args = &rest[at + "Host(".len()..];
                let end = args.find(')').unwrap_or(args.len());
                // Host(`a`) and the older Host(`a`, `b`): every backticked
                // name between the parentheses.
                for (i, part) in args[..end].split('`').enumerate() {
                    if i % 2 == 1 && !part.is_empty() {
                        out.push(part.to_ascii_lowercase());
                    }
                }
                rest = &args[end..];
            }
        }
    }
    Ok(out)
}

/// Every backend a route file forwards to: each `url` or `address` in a
/// `servers` list, exactly as the file writes it.
pub fn backends(content: &str) -> Result<Vec<String>, String> {
    fn walk(v: &serde_yaml::Value, out: &mut Vec<String>) {
        match v {
            serde_yaml::Value::Mapping(m) => {
                for (k, val) in m {
                    if k.as_str() == Some("servers") {
                        for s in val.as_sequence().into_iter().flatten() {
                            for key in ["url", "address"] {
                                if let Some(t) = s.get(key).and_then(|t| t.as_str()) {
                                    out.push(t.to_string());
                                }
                            }
                        }
                    } else {
                        walk(val, out);
                    }
                }
            }
            serde_yaml::Value::Sequence(s) => s.iter().for_each(|x| walk(x, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(&parse(content)?, &mut out);
    Ok(out)
}

/// The facts core judges, read out of one declared route file.
pub fn facts(decl: &RouteDecl) -> Result<RouteFacts, String> {
    Ok(RouteFacts {
        stack: decl.stack.clone(),
        filename: decl.filename.clone(),
        hosts: hostnames(&decl.content)?,
        backends: backends(&decl.content)?,
        external: decl.external.clone(),
    })
}
