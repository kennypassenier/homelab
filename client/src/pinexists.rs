//! gap-37: the client half of the pinned-digest check run by `homelab check`.
//!
//! For every `image: …@sha256:…` in the stack files: `GET /v2/` for the
//! registry's auth challenge, an anonymous pull token from its realm, then
//! `HEAD /v2/<repository>/manifests/<digest>`. 200 is present, 404 is
//! missing; anything else (401/403 on a private repository, no network) is
//! "not asked" and never a fault. Only anonymous reads: no credential leaves
//! this machine. One registry that does not answer at all is asked once, not
//! once per image.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

use homelab_core::ops::fleetcheck::Finding;
use homelab_core::ops::pinexists::{
    evaluate_pin_existence, parse_challenge, pinned_digests, PinAnswer, PinnedDigest,
};

const ACCEPT: &str = "application/vnd.oci.image.index.v1+json, \
application/vnd.docker.distribution.manifest.list.v2+json, \
application/vnd.oci.image.manifest.v1+json, \
application/vnd.docker.distribution.manifest.v2+json";

/// Every pinned digest under `stacks_dir`, with its stack name.
pub fn collect(stacks_dir: &Path) -> Vec<PinnedDigest> {
    let mut out = Vec::new();
    let Ok(stacks) = std::fs::read_dir(stacks_dir) else {
        return out;
    };
    let mut dirs: Vec<_> = stacks.flatten().map(|e| e.path()).collect();
    dirs.sort();
    for stack in dirs {
        let name = stack
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let Ok(apps) = std::fs::read_dir(&stack) else {
            continue;
        };
        let mut apps: Vec<_> = apps.flatten().map(|e| e.path()).collect();
        apps.sort();
        for app in apps {
            let Ok(text) = std::fs::read_to_string(app.join("docker-compose.yml")) else {
                continue;
            };
            for mut p in pinned_digests(&text) {
                p.stack = name.clone();
                out.push(p);
            }
        }
    }
    out
}

/// `curl` with a timeout; stdout and the HTTP status (000 = no answer).
fn curl(args: &[&str]) -> (String, String) {
    let out = Command::new("curl")
        .args(["-sS", "-m", "20"])
        .args(args)
        .output();
    match out {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::new(),
        ),
        Err(e) => (String::new(), e.to_string()),
    }
}

fn token(registry: &str, repository: &str) -> Result<Option<String>, String> {
    let (headers, err) = curl(&[
        "-D",
        "-",
        "-o",
        "/dev/null",
        &format!("https://{}/v2/", registry),
    ]);
    if !err.is_empty() || headers.is_empty() {
        return Err(format!("{} did not answer", registry));
    }
    let Some(challenge) = headers.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("www-authenticate")
            .then(|| v.trim().to_string())
    }) else {
        return Ok(None);
    };
    let Some((realm, service)) = parse_challenge(&challenge) else {
        return Ok(None);
    };
    let url = format!(
        "{}?service={}&scope=repository:{}:pull",
        realm, service, repository
    );
    let (body, err) = curl(&[&url]);
    if !err.is_empty() {
        return Err(err);
    }
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("token answer: {}", e))?;
    Ok(v.get("token")
        .or_else(|| v.get("access_token"))
        .and_then(|t| t.as_str())
        .map(str::to_string))
}

fn ask(p: &PinnedDigest) -> PinAnswer {
    let tok = match token(&p.registry, &p.repository) {
        Ok(t) => t,
        Err(e) => return PinAnswer::NotAsked(e),
    };
    let url = format!(
        "https://{}/v2/{}/manifests/{}",
        p.registry, p.repository, p.digest
    );
    let auth = tok.map(|t| format!("Authorization: Bearer {}", t));
    let accept = format!("Accept: {}", ACCEPT);
    let mut args: Vec<&str> = vec!["-I", "-o", "/dev/null", "-w", "%{http_code}", "-H", &accept];
    if let Some(a) = auth.as_deref() {
        args.push("-H");
        args.push(a);
    }
    args.push(&url);
    let (code, err) = curl(&args);
    match code.trim() {
        "200" => PinAnswer::Present,
        "404" => PinAnswer::Missing,
        _ if !err.is_empty() => PinAnswer::NotAsked(err),
        other => PinAnswer::NotAsked(format!("the registry answered {}", other)),
    }
}

/// Ask about every pin; a registry that did not answer once is not asked
/// again in the same run.
pub fn check_pins(stacks_dir: &Path) -> Vec<Finding> {
    let mut dead: BTreeSet<String> = BTreeSet::new();
    let mut seen: BTreeMap<String, PinAnswer> = BTreeMap::new();
    let mut answers = Vec::new();
    for p in collect(stacks_dir) {
        let a = if dead.contains(&p.registry) {
            PinAnswer::NotAsked(format!("{} did not answer", p.registry))
        } else if let Some(a) = seen.get(&p.reference) {
            a.clone()
        } else {
            let a = ask(&p);
            if matches!(&a, PinAnswer::NotAsked(w) if w.ends_with("did not answer")) {
                dead.insert(p.registry.clone());
            }
            seen.insert(p.reference.clone(), a.clone());
            a
        };
        answers.push((p, a));
    }
    evaluate_pin_existence(&answers)
}
