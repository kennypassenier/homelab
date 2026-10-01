//! feat-stacks-files (area A of the 2026-09-30 "everything editable" round):
//! the general file editor's create/delete/rename ops. Editing an existing
//! file's content keeps going through [`super::stackedit::StackEdit::Raw`]
//! unchanged (that already worked); this module only adds what raw refused:
//! a path that does not yet exist, or removing/renaming one that does.
//!
//! New module per the area split (`common-rules.txt`): `stackedit.rs` only
//! gets the small additive hook that dispatches into here. Pure: the
//! current texts come in, the new texts go out, same contract as
//! `stackedit::changes`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::actions::Refusal;
use super::stackedit::{FileChange, RAW_MAX, StackTexts, refusal};

/// What the Files tab asks to do with a path the raw editor could not:
/// bring a new one into existence, remove one, or give one another name.
/// Editing an existing file's content stays `StackEdit::Raw`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum FilesEdit {
    /// A file under the stack's directory that does not exist yet.
    Create { path: String, content: String },
    /// A file the stack already has.
    Delete { path: String },
    /// A file the stack already has, under a path nothing else already
    /// uses.
    Rename { from: String, to: String },
}

impl FilesEdit {
    pub fn describe(&self) -> String {
        match self {
            FilesEdit::Create { path, .. } => format!("{path} created in the dashboard"),
            FilesEdit::Delete { path } => format!("{path} removed in the dashboard"),
            FilesEdit::Rename { from, to } => format!("{from} renamed to {to}"),
        }
    }
}

/// A path segment is safe: no empty piece, no `.`/`..`, never absolute.
fn plain_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && path
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..")
}

fn is_secret(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name == ".env" || name.ends_with(".env") || name.starts_with(".env")
}

/// A path the editor may create: inside the stack, not already there, not
/// a secret. The same shape `raw_path_problem` requires of an existing
/// file, with the existence check flipped.
pub fn new_path_problem(path: &str, texts: &StackTexts) -> Option<String> {
    if !plain_path(path) {
        return Some(format!("{path:?} is not a path inside the stack"));
    }
    if is_secret(path) {
        return Some(format!(
            "{path} would hold secrets; secrets are changed through latch, never in the repository"
        ));
    }
    if texts.contains_key(path) {
        return Some(format!(
            "{path} already exists; open it in the editor instead"
        ));
    }
    None
}

/// A path the editor may act on because the stack already has it (delete,
/// or the source of a rename); the same refusal the raw editor gives.
fn existing_path_problem(path: &str, texts: &StackTexts) -> Option<String> {
    super::stackedit::raw_path_problem(path, texts)
}

/// The new texts for one files edit. Mirrors `stackedit::changes`'
/// contract: only the files that change come back.
pub fn changes(
    stack: &str,
    texts: &StackTexts,
    edit: &FilesEdit,
) -> Result<Vec<FileChange>, Refusal> {
    let full = |rel: &str| format!("stacks/{stack}/{rel}");
    match edit {
        FilesEdit::Create { path, content } => {
            if let Some(why) = new_path_problem(path, texts) {
                return Err(refusal(
                    stack,
                    why,
                    "pick a path the editor does not already list",
                ));
            }
            if content.len() > RAW_MAX {
                return Err(refusal(
                    stack,
                    format!(
                        "{path} would be {} bytes; the editor takes at most {RAW_MAX}",
                        content.len()
                    ),
                    "create a file that large in a workstation clone",
                ));
            }
            Ok(vec![FileChange {
                path: full(path),
                old: None,
                new: Some(content.clone()),
            }])
        }
        FilesEdit::Delete { path } => {
            if let Some(why) = existing_path_problem(path, texts) {
                return Err(refusal(stack, why, "pick a file the editor lists"));
            }
            if path == super::stackedit::MANIFEST {
                return Err(refusal(
                    stack,
                    format!("{path} is the stack's own manifest and cannot be deleted"),
                    "remove the whole stack instead, from the fleet page",
                ));
            }
            Ok(vec![FileChange {
                path: full(path),
                old: texts.get(path).cloned(),
                new: None,
            }])
        }
        FilesEdit::Rename { from, to } => {
            if let Some(why) = existing_path_problem(from, texts) {
                return Err(refusal(stack, why, "pick a file the editor lists"));
            }
            if from == super::stackedit::MANIFEST {
                return Err(refusal(
                    stack,
                    format!("{from} is the stack's own manifest and cannot be renamed"),
                    "leave lxc-compose.yml where it is",
                ));
            }
            if let Some(why) = new_path_problem(to, texts) {
                return Err(refusal(
                    stack,
                    why,
                    "pick a new path the editor does not already list",
                ));
            }
            if from == to {
                return Err(refusal(
                    stack,
                    "the new path is the same as the old one",
                    "change the path, or cancel",
                ));
            }
            let content = texts.get(from).cloned().unwrap_or_default();
            Ok(vec![
                FileChange {
                    path: full(from),
                    old: texts.get(from).cloned(),
                    new: None,
                },
                FileChange {
                    path: full(to),
                    old: None,
                    new: Some(content),
                },
            ])
        }
    }
}

/// A commented starting skeleton for a new file of a well-known kind,
/// chosen by its path's shape; the editor offers it and the field starts
/// with it, but the content sent to `changes` is always whatever the
/// browser has by then — the template is a convenience, not a contract.
pub fn template(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name == "checks.yml" {
        Some(CHECKS_TEMPLATE)
    } else if name == "service.yml" {
        Some(SERVICE_TEMPLATE)
    } else if name.ends_with(".service") {
        Some(UNIT_TEMPLATE)
    } else if name == "traefik-routes.yml" {
        Some(TRAEFIK_ROUTES_TEMPLATE)
    } else if path.starts_with("routes/") && (name.ends_with(".yml") || name.ends_with(".yaml")) {
        Some(ROUTE_FILE_TEMPLATE)
    } else if name == "docker-compose.yml" {
        Some(COMPOSE_TEMPLATE)
    } else {
        None
    }
}

/// Every template, by the well-known name it applies to (for the edit-read
/// endpoint, so the browser need not repeat this list).
pub fn templates() -> BTreeMap<&'static str, &'static str> {
    [
        ("checks.yml", CHECKS_TEMPLATE),
        ("service.yml", SERVICE_TEMPLATE),
        ("<unit>.service", UNIT_TEMPLATE),
        ("traefik-routes.yml", TRAEFIK_ROUTES_TEMPLATE),
        ("routes/<name>.yml", ROUTE_FILE_TEMPLATE),
        ("docker-compose.yml", COMPOSE_TEMPLATE),
    ]
    .into_iter()
    .collect()
}

const CHECKS_TEMPLATE: &str = r#"# checks.yml — what this service is judged on (homelab_core::checks).
# See the Checks tab for the form; this is the file it writes.
#
# checks: # a before/after pair, run around an update
#   - name: films in the library
#     command: curl -s http://localhost:8096/Items/Counts | jq .MovieCount
#     expect: never_decreases   # never_decreases | must_match | must_be_present
#     layer: application        # network | process | application | user_visible
#     blind_spot: says nothing about whether a film actually plays
#
# probes: # a nightly measurement with an absolute answer
#   - name: backup ran last night
#     command: find /appdata -newer /tmp/marker -mtime -1 | wc -l
#     healthy: { at_least: 1 }  # equals: "..." | at_least: N | at_most: N
#     layer: process
#
# manual: # a question only a person can answer, asked as a notification
#   - question: does the picture look right on the television?
#
# busy_check:
#   command: who-is-using-this --count
#
# url: https://example.org
checks: []
probes: []
manual: []
"#;

const SERVICE_TEMPLATE: &str = r#"# service.yml — a native systemd service (no docker layer). See
# homelab_core::native::NativeServiceManifest for every field.
stack_name: <stack>
vmid: 0
hostname: <vmid>-app-<stack>
unit: <unit>
binary: /opt/<unit>/bin/<unit>
# env_file: /appdata/<stack>/<unit>-config/<unit>.env
data_dirs:
  - /appdata/<stack>/<unit>-config
# update_cmd: <unit> update
# release_repo: <owner>/<unit>
# stateless: false
"#;

const UNIT_TEMPLATE: &str = r#"# <unit>.service — installed by homelab into the container's systemd.
[Unit]
Description=<unit>
Wants=network-online.target
After=network-online.target

[Service]
Type=simple
User=<unit>
Group=<unit>
WorkingDirectory=/appdata/<stack>/<unit>-config
ExecStart=/opt/<unit>/bin/<unit>
Restart=always
RestartSec=5s

[Install]
WantedBy=multi-user.target
"#;

const TRAEFIK_ROUTES_TEMPLATE: &str = r#"# traefik-routes.yml — this stack's gateway_route, deployed to the
# gateway's routes directory under the name homelab_core::routes expects.
http:
  routers:
    <app>:
      rule: "Host(`<app>.example.org`)"
      entryPoints: [web]
      service: <app>

  services:
    <app>:
      loadBalancer:
        servers:
          - url: "http://<container-ip>:<port>"
"#;

const ROUTE_FILE_TEMPLATE: &str = r#"# routes/<name>.yml — one of this stack's extra_routes: a route file the
# gateway serves that is not the stack's own gateway_route (homelab_core::
# routes::RouteDecl). Same shape as traefik-routes.yml.
http:
  routers:
    <name>:
      rule: "Host(`<name>.example.org`)"
      entryPoints: [web]
      service: <name>

  services:
    <name>:
      loadBalancer:
        servers:
          - url: "http://<target-ip>:<port>"
"#;

const COMPOSE_TEMPLATE: &str = r#"services:
  <service>:
    image: <name>:<tag>
    restart: unless-stopped
"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> StackTexts {
        [
            (
                "lxc-compose.yml".to_string(),
                "stack_name: demo\n".to_string(),
            ),
            (
                "app/docker-compose.yml".to_string(),
                "services: {}\n".to_string(),
            ),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn create_refuses_an_existing_path() {
        let t = texts();
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Create {
                path: "app/docker-compose.yml".into(),
                content: "x".into(),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn create_refuses_traversal_and_secrets() {
        let t = texts();
        for p in ["../evil", "/abs", "app/.env", "a//b", ""] {
            let r = changes(
                "demo",
                &t,
                &FilesEdit::Create {
                    path: p.into(),
                    content: "x".into(),
                },
            );
            assert!(r.is_err(), "{p:?} should have been refused");
        }
    }

    #[test]
    fn create_writes_a_new_file() {
        let t = texts();
        let out = changes(
            "demo",
            &t,
            &FilesEdit::Create {
                path: "app/checks.yml".into(),
                content: "checks: []\n".into(),
            },
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].path, "stacks/demo/app/checks.yml");
        assert_eq!(out[0].old, None);
        assert_eq!(out[0].new.as_deref(), Some("checks: []\n"));
    }

    #[test]
    fn create_refuses_oversized_content() {
        let t = texts();
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Create {
                path: "app/big.txt".into(),
                content: "x".repeat(RAW_MAX + 1),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn delete_refuses_a_missing_file() {
        let t = texts();
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Delete {
                path: "app/nope.yml".into(),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn delete_refuses_the_manifest() {
        let t = texts();
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Delete {
                path: "lxc-compose.yml".into(),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn delete_writes_a_removal() {
        let t = texts();
        let out = changes(
            "demo",
            &t,
            &FilesEdit::Delete {
                path: "app/docker-compose.yml".into(),
            },
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].new, None);
        assert!(out[0].old.is_some());
    }

    #[test]
    fn rename_refuses_onto_an_existing_path() {
        let mut t = texts();
        t.insert("app/other.yml".into(), "x".into());
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Rename {
                from: "app/docker-compose.yml".into(),
                to: "app/other.yml".into(),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn rename_refuses_the_manifest() {
        let t = texts();
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Rename {
                from: "lxc-compose.yml".into(),
                to: "lxc-compose2.yml".into(),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn rename_refuses_the_same_path() {
        let t = texts();
        let r = changes(
            "demo",
            &t,
            &FilesEdit::Rename {
                from: "app/docker-compose.yml".into(),
                to: "app/docker-compose.yml".into(),
            },
        );
        assert!(r.is_err());
    }

    #[test]
    fn rename_moves_the_content() {
        let t = texts();
        let out = changes(
            "demo",
            &t,
            &FilesEdit::Rename {
                from: "app/docker-compose.yml".into(),
                to: "app/renamed.yml".into(),
            },
        )
        .unwrap();
        assert_eq!(out.len(), 2);
        let removed = out
            .iter()
            .find(|c| c.path.ends_with("docker-compose.yml"))
            .unwrap();
        assert_eq!(removed.new, None);
        let added = out
            .iter()
            .find(|c| c.path.ends_with("renamed.yml"))
            .unwrap();
        assert_eq!(added.old, None);
        assert_eq!(added.new.as_deref(), Some("services: {}\n"));
    }

    #[test]
    fn every_template_is_registered() {
        for (name, body) in templates() {
            assert!(!body.trim().is_empty(), "{name} has an empty template");
        }
        assert!(template("app/checks.yml").is_some());
        assert!(template("app/service.yml").is_some());
        assert!(template("app/app.service").is_some());
        assert!(template("traefik-routes.yml").is_some());
        assert!(template("routes/manual-x.yml").is_some());
        assert!(template("newapp/docker-compose.yml").is_some());
        assert!(template("README.md").is_none());
    }

    /// The driven form (driveedit.rs) and the Files card's own buttons
    /// both build this JSON and hand it to `POST …/plan` /
    /// `POST …/commit` un-typed (`serde_json::from_value`); this is the
    /// wire shape that decode has to accept: the outer `StackEdit`'s own
    /// "kind" tag, then `FilesEdit`'s "op" tag, in the one object.
    #[test]
    fn files_edit_is_the_stackedit_wire_shape() {
        use super::super::stackedit::StackEdit;

        let create: StackEdit = serde_json::from_str(
            r#"{"kind":"files","op":"create","path":"app/checks.yml","content":"checks: []\n"}"#,
        )
        .unwrap();
        assert_eq!(
            create,
            StackEdit::Files(FilesEdit::Create {
                path: "app/checks.yml".into(),
                content: "checks: []\n".into(),
            })
        );

        let delete: StackEdit =
            serde_json::from_str(r#"{"kind":"files","op":"delete","path":"app/old.yml"}"#).unwrap();
        assert_eq!(
            delete,
            StackEdit::Files(FilesEdit::Delete {
                path: "app/old.yml".into(),
            })
        );

        let rename: StackEdit =
            serde_json::from_str(r#"{"kind":"files","op":"rename","from":"a.yml","to":"b.yml"}"#)
                .unwrap();
        assert_eq!(
            rename,
            StackEdit::Files(FilesEdit::Rename {
                from: "a.yml".into(),
                to: "b.yml".into(),
            })
        );

        // And back: what the browser/CLI sends is what `changes()` above
        // is exercised with, round-tripped through the same Value the
        // real request body is (`serde_json::from_value`, not `from_str`).
        let v = serde_json::to_value(&create).unwrap();
        let back: StackEdit = serde_json::from_value(v).unwrap();
        assert_eq!(back, create);
    }
}
