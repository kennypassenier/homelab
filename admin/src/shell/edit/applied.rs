//! What the host last applied for a stack, against what a deploy of the
//! new files would send: `+ path`, `~ path`, `- path` (like `homelab
//! apply`). Read-only on the host.

use std::collections::BTreeMap;
use std::time::Duration;

use homelab_proto::{Command, FileBlob};

use super::EditCtx;

pub(super) async fn applied_changes(
    c: &EditCtx,
    stack: &str,
    digest: Option<&BTreeMap<String, String>>,
) -> serde_json::Value {
    let Some(digest) = digest else {
        return serde_json::json!({ "unavailable": "the stack's files could not be listed" });
    };
    let applied = match fetch_applied(c, stack).await {
        Ok(a) => a,
        Err(unavailable) => return unavailable,
    };
    if applied.is_empty() {
        return serde_json::json!({ "never": true, "changes": [] });
    }
    serde_json::json!({ "changes": diff_applied(digest, &applied) })
}

/// The host's applied files, or the `{ "unavailable": why }` answer.
async fn fetch_applied(c: &EditCtx, stack: &str) -> Result<Vec<FileBlob>, serde_json::Value> {
    let r = c
        .host
        .ask_traced(
            Command::GetApplied {
                stack: stack.to_string(),
            },
            Duration::from_secs(30),
            None,
        )
        .await;
    match r {
        Ok(r) if r.ok => serde_json::from_str(&r.message)
            .map_err(|_| serde_json::json!({ "unavailable": "the host's answer did not read" })),
        Ok(r) => Err(
            serde_json::json!({ "unavailable": format!("the host answered: {}", r.message.chars().take(200).collect::<String>()) }),
        ),
        Err(e) => Err(serde_json::json!({ "unavailable": e })),
    }
}

fn diff_applied(digest: &BTreeMap<String, String>, applied: &[FileBlob]) -> Vec<String> {
    let mut out = Vec::new();
    for (path, sha) in digest {
        match applied.iter().find(|a| &a.path == path) {
            None => out.push(format!("+ {path}")),
            Some(a) if &homelab_core::manifest::sha256_hex(a.content.as_bytes()) != sha => {
                out.push(format!("~ {path}"))
            }
            Some(_) => {}
        }
    }
    for a in applied {
        if !digest.contains_key(&a.path) {
            out.push(format!("- {}", a.path));
        }
    }
    out.sort_by(|a, b| a[2..].cmp(&b[2..]));
    out
}
