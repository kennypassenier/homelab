//! gap-37: the client half of the pinned-digest check run by `homelab check`.
//!
//! For every `image: …@sha256:…` in the stack files: `GET /v2/` for the
//! registry's auth challenge, an anonymous pull token from its realm, then
//! `HEAD /v2/<repository>/manifests/<digest>`. 200 is present, 404 is
//! missing; anything else (401/403 on a private repository, no network) is
//! "not asked" and never a fault. Only anonymous reads: no credential leaves
//! this machine. One registry that does not answer at all is not asked again
//! for the rest of the run. Registries are asked 8 at a time; nothing is
//! remembered between runs, so a vanished image is always reported fresh.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use homelab_core::ops::fleetcheck::Finding;
use homelab_core::ops::pinexists::{
    PinAnswer, PinnedDigest, evaluate_pin_existence, parse_challenge, pinned_digests,
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

/// registry-cache-plaintext (deep-dive answer, 2026-10-01): resolve one
/// `registry/repository:tag` to the digest the source registry hands out
/// for it right now. Same anonymous-token flow as `ask` above, `HEAD` in
/// place of a digest `GET` because only the header is needed; `None` when
/// the registry answered but carried no `Docker-Content-Digest` (an old
/// registry, a manifest list format it chose not to send one for) rather
/// than treating that as a hard failure — the caller leaves the tag as
/// written either way.
pub fn resolve_digest(
    registry: &str,
    repository: &str,
    tag: &str,
) -> Result<Option<String>, String> {
    let tok = token(registry, repository)?;
    let url = format!("https://{}/v2/{}/manifests/{}", registry, repository, tag);
    let accept = format!("Accept: {}", ACCEPT);
    let auth = tok.map(|t| format!("Authorization: Bearer {}", t));
    let mut args: Vec<&str> = vec!["-I", "-H", &accept];
    if let Some(a) = auth.as_deref() {
        args.push("-H");
        args.push(a);
    }
    args.push(&url);
    let (headers, err) = curl(&args);
    if !err.is_empty() {
        return Err(err);
    }
    Ok(homelab_core::ops::pinexists::parse_digest_header(&headers))
}

/// How many registries are asked at the same time (decision "Fleet check
/// speed", 2026-09-29): 8, no answer remembered across runs.
pub const PIN_ASK_WIDTH: usize = 8;

/// Ask about every pin; a registry that did not answer once is not asked
/// again in the same run.
pub fn check_pins(stacks_dir: &Path) -> Vec<Finding> {
    let answers = answer_pins(collect(stacks_dir), PIN_ASK_WIDTH, ask);
    evaluate_pin_existence(&answers)
}

/// The answer for every pin, in the order given. Identical references are
/// asked once; a registry that "did not answer" is not asked again.
pub fn answer_pins<F>(
    pins: Vec<PinnedDigest>,
    width: usize,
    ask: F,
) -> Vec<(PinnedDigest, PinAnswer)>
where
    F: Fn(&PinnedDigest) -> PinAnswer + Sync,
{
    // One question per distinct reference, in first-seen order.
    let mut firsts: Vec<usize> = Vec::new();
    let mut slot_of: BTreeMap<&str, usize> = BTreeMap::new();
    let mut slots: Vec<usize> = Vec::with_capacity(pins.len());
    for (i, p) in pins.iter().enumerate() {
        let slot = *slot_of.entry(p.reference.as_str()).or_insert_with(|| {
            firsts.push(i);
            firsts.len() - 1
        });
        slots.push(slot);
    }
    let next = AtomicUsize::new(0);
    let dead: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
    let results: Vec<Mutex<Option<PinAnswer>>> = firsts.iter().map(|_| Mutex::new(None)).collect();
    let workers = width.max(1).min(firsts.len());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let slot = next.fetch_add(1, Ordering::SeqCst);
                    let Some(&i) = firsts.get(slot) else { break };
                    let p = &pins[i];
                    let known_dead = dead
                        .lock()
                        .map(|d| d.contains(&p.registry))
                        .unwrap_or(false);
                    let a = if known_dead {
                        PinAnswer::NotAsked(format!("{} did not answer", p.registry))
                    } else {
                        let a = ask(p);
                        if matches!(&a, PinAnswer::NotAsked(w) if w.ends_with("did not answer"))
                            && let Ok(mut d) = dead.lock()
                        {
                            d.insert(p.registry.clone());
                        }
                        a
                    };
                    if let Ok(mut r) = results[slot].lock() {
                        *r = Some(a);
                    }
                }
            });
        }
    });
    let results: Vec<PinAnswer> = results
        .into_iter()
        .map(|r| {
            r.into_inner()
                .ok()
                .flatten()
                .unwrap_or_else(|| PinAnswer::NotAsked("the question was not finished".into()))
        })
        .collect();
    pins.into_iter()
        .zip(slots)
        .map(|(p, slot)| (p, results[slot].clone()))
        .collect()
}
