//! feat-stacks-11 (Area C, 2026-09-30): the latch form — which apps' `.env`
//! comes from latch (`latch_secrets:`) and which secret FILES this stack
//! gets from latch (`latch_files:`). Both are client-only keys, siblings of
//! the manifest's own top-level keys in `lxc-compose.yml`
//! (`client/src/spec.rs::StackFile` flattens the manifest in) — `yamledit`
//! works on the file's text, not on a Rust struct, so setting them at the
//! document's top level is exactly like setting `apps:` or `storage:`.
//! Pure.

use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use super::yamledit::{Item, Op, path};
use homelab_proto::StackManifest;

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LatchEdit {
    /// None: leave `latch_secrets:` alone. `Some(apps)`: the whole list —
    /// one checkbox per app in the form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secrets: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<LatchFileEdit>>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LatchFileEdit {
    #[serde(default)]
    pub origin: Option<usize>,
    pub from: String,
    pub dest: String,
    pub mode: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub restarts: Option<String>,
}

/// What the file currently holds, read independently of `StackManifest`
/// (these keys are not part of it — see the module doc).
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CurrentLatch {
    #[serde(default)]
    pub latch_secrets: Vec<String>,
    #[serde(default)]
    pub latch_files: Vec<CurrentLatchFile>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CurrentLatchFile {
    pub from: String,
    pub dest: String,
    pub mode: String,
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub restarts: Option<String>,
}

/// What `latch_secrets:` and `latch_files:` currently hold, for the edit
/// page to show (the form's read side); `changes()` above is the write
/// side and reads the same way independently.
pub fn current(text: &str) -> CurrentLatch {
    serde_yaml::from_str::<Value>(text)
        .ok()
        .and_then(|v| serde_yaml::from_value(v).ok())
        .unwrap_or_default()
}

/// A value that will reach latch's `--expand`, which parses every file it
/// is handed: one stray `${…}` it cannot resolve breaks every stack's
/// secrets, not just this one's (the trap latch's own docs warn about).
fn dollar_problem(what: &str, v: &str) -> Option<String> {
    v.contains("${").then(|| {
        format!(
            "{what} {v:?} contains '${{': latch --expand parses every file it is given, so an \
             unresolvable placeholder here would break every stack's secrets, not only this \
             one's — write the literal value"
        )
    })
}

/// Mirrors `client/src/spec.rs::check_latch_file` (the field that field
/// belongs to is private to the client crate, so the same rules are
/// repeated here for the form's own quick feedback); the client re-checks
/// everything at deploy time regardless.
fn plain(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '.' | '_' | '-'))
        && !s.split('/').any(|p| p == "..")
}

pub fn problems(m: &StackManifest, e: &LatchEdit) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(apps) = &e.secrets {
        for a in apps {
            if let Some(p) = dollar_problem("latch_secrets", a) {
                out.push(p);
            }
            if !m.apps.contains(a) {
                out.push(format!("latch_secrets: {a} is not an app of this stack"));
            }
        }
    }
    if let Some(files) = &e.files {
        for f in files {
            for (what, v) in [
                ("latch_files.from", f.from.as_str()),
                ("latch_files.dest", f.dest.as_str()),
                ("latch_files.mode", f.mode.as_str()),
            ]
            .into_iter()
            .chain(f.owner.as_deref().map(|o| ("latch_files.owner", o)))
            .chain(f.restarts.as_deref().map(|r| ("latch_files.restarts", r)))
            {
                if let Some(p) = dollar_problem(what, v) {
                    out.push(p);
                }
            }
            if !f.dest.starts_with('/') || !plain(&f.dest) {
                out.push(format!(
                    "latch_files: dest {:?} must be an absolute path of letters, digits, '/', \
                     '.', '_' and '-'",
                    f.dest
                ));
            }
            if f.from.starts_with('/') || !plain(&f.from) {
                out.push(format!(
                    "latch_files: from {:?} must be a relative path inside the stack",
                    f.from
                ));
            }
            let octal =
                (3..=4).contains(&f.mode.len()) && f.mode.chars().all(|c| ('0'..='7').contains(&c));
            if !octal {
                out.push(format!(
                    "latch_files: mode {:?} for {} must be octal like \"640\"",
                    f.mode, f.dest
                ));
            }
            if let Some(o) = &f.owner {
                let part = |p: &str| {
                    !p.is_empty()
                        && p.chars()
                            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
                };
                let ok = matches!(o.split_once(':'), Some((u, g)) if part(u) && part(g));
                if !ok {
                    out.push(format!(
                        "latch_files: owner {o:?} for {} must be user:group",
                        f.dest
                    ));
                }
            }
            if let Some(u) = &f.restarts
                && !m.natives.contains(u)
            {
                out.push(format!(
                    "latch_files: restarts {u:?} for {} is not a native unit of this stack \
                         :: natives are [{}]",
                    f.dest,
                    m.natives.join(", ")
                ));
            }
        }
    }
    out
}

fn latch_file_value(f: &LatchFileEdit) -> Value {
    let mut m = Mapping::new();
    m.insert("from".into(), Value::from(f.from.as_str()));
    m.insert("dest".into(), Value::from(f.dest.as_str()));
    m.insert("mode".into(), Value::from(f.mode.as_str()));
    if let Some(o) = &f.owner {
        m.insert("owner".into(), Value::from(o.as_str()));
    }
    if let Some(r) = &f.restarts {
        m.insert("restarts".into(), Value::from(r.as_str()));
    }
    Value::Mapping(m)
}

/// The ops on the raw text. `manifest_text` is read independently of the
/// already-parsed `StackManifest` because these keys are not in it.
pub fn ops(manifest_text: &str, e: &LatchEdit) -> Vec<Op> {
    let now = current(manifest_text);
    let mut ops = Vec::new();
    if let Some(apps) = &e.secrets {
        let mut wanted: Vec<String> = apps.clone();
        wanted.sort();
        wanted.dedup();
        let mut have = now.latch_secrets.clone();
        have.sort();
        if have != wanted {
            if wanted.is_empty() {
                if !now.latch_secrets.is_empty() {
                    ops.push(Op::Remove {
                        path: path("latch_secrets"),
                    });
                }
            } else {
                ops.push(Op::Set {
                    path: path("latch_secrets"),
                    value: Value::Sequence(
                        wanted.iter().map(|a| Value::from(a.as_str())).collect(),
                    ),
                });
            }
        }
    }
    if let Some(files) = &e.files {
        let old_len = now.latch_files.len();
        let mut used = std::collections::BTreeSet::new();
        let mut items = Vec::new();
        for f in files {
            match f.origin {
                Some(i) if i < old_len && used.insert(i) => {
                    let unchanged = now.latch_files.get(i).is_some_and(|o| {
                        o.from == f.from
                            && o.dest == f.dest
                            && o.mode == f.mode
                            && o.owner == f.owner
                            && o.restarts == f.restarts
                    });
                    if unchanged {
                        items.push(Item::Keep(i));
                    } else {
                        items.push(Item::Retext(i, latch_file_value(f)));
                    }
                }
                _ => items.push(Item::New(latch_file_value(f))),
            }
        }
        let same = items.len() == old_len
            && items
                .iter()
                .enumerate()
                .all(|(n, it)| matches!(it, Item::Keep(i) if *i == n));
        if !same {
            if items.is_empty() {
                if old_len > 0 {
                    ops.push(Op::Remove {
                        path: path("latch_files"),
                    });
                }
            } else if old_len == 0 {
                // Nothing to keep or retext (every item is `Item::New`
                // since there was nothing old to point `origin` at) and
                // possibly no `latch_files:` key in the file at all yet —
                // `Op::Seq`'s rebuild needs an existing key to rewrite,
                // the same as `Op::Set` needs for a NESTED path, but a
                // top-level `Op::Set` (like `latch_secrets` above) creates
                // the key when it is missing, so it is used here instead.
                ops.push(Op::Set {
                    path: path("latch_files"),
                    value: Value::Sequence(
                        items
                            .into_iter()
                            .map(|it| match it {
                                Item::New(v) => v,
                                Item::Keep(_) | Item::Retext(_, _) => {
                                    unreachable!("old_len == 0: every item is Item::New")
                                }
                            })
                            .collect(),
                    ),
                });
            } else {
                ops.push(Op::Seq {
                    path: path("latch_files"),
                    items,
                });
            }
        }
    }
    ops
}

pub fn describe(e: &LatchEdit) -> Vec<String> {
    let mut parts = Vec::new();
    if let Some(apps) = &e.secrets {
        parts.push(format!("latch secrets: {} app(s)", apps.len()));
    }
    if let Some(files) = &e.files {
        parts.push(format!("latch files: {} row(s)", files.len()));
    }
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> StackManifest {
        serde_yaml::from_str(
            r#"
stack_name: demo
vmid: 200
hostname: 200-app-demo
network: { ip: "10.10.10.10/24", gateway: "10.10.10.1" }
resources: { cores: 2, memory_mb: 1024, disk_gb: 8 }
lxc: { template: "local:vztmpl/x.tar.zst" }
boot: {}
apps: [jellyfin]
natives: [kyu]
"#,
        )
        .unwrap()
    }

    const TEXT: &str = "stack_name: demo\nvmid: 200\napps: [jellyfin]\nnatives: [kyu]\n";

    #[test]
    fn secrets_must_be_an_app() {
        let m = manifest();
        let e = LatchEdit {
            secrets: Some(vec!["nope".into()]),
            ..Default::default()
        };
        let p = problems(&m, &e);
        assert_eq!(p, vec!["latch_secrets: nope is not an app of this stack"]);
    }

    #[test]
    fn dollar_brace_is_refused() {
        let m = manifest();
        let e = LatchEdit {
            files: Some(vec![LatchFileEdit {
                origin: None,
                from: "env.txt".into(),
                dest: "/etc/app/${X}".into(),
                mode: "640".into(),
                owner: None,
                restarts: None,
            }]),
            ..Default::default()
        };
        let p = problems(&m, &e);
        assert!(p.iter().any(|s| s.contains("latch --expand")));
    }

    #[test]
    fn restarts_must_be_a_native() {
        let m = manifest();
        let e = LatchEdit {
            files: Some(vec![LatchFileEdit {
                origin: None,
                from: "unit.env".into(),
                dest: "/etc/kyu/unit.env".into(),
                mode: "600".into(),
                owner: None,
                restarts: Some("not-a-native".into()),
            }]),
            ..Default::default()
        };
        let p = problems(&m, &e);
        assert!(p.iter().any(|s| s.contains("is not a native unit")));
    }

    #[test]
    fn no_change_means_no_ops() {
        let e = LatchEdit {
            secrets: Some(vec!["jellyfin".into()]),
            ..Default::default()
        };
        let text = format!("{TEXT}latch_secrets: [jellyfin]\n");
        assert!(ops(&text, &e).is_empty());
    }

    #[test]
    fn adding_a_secret_app_sets_the_list() {
        let e = LatchEdit {
            secrets: Some(vec!["jellyfin".into(), "sonarr".into()]),
            ..Default::default()
        };
        let got = ops(TEXT, &e);
        assert_eq!(got.len(), 1);
    }
}
