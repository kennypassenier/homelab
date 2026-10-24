//! pinned-digest-pruned-1: every digest a stack pins stays pullable after the
//! upstream deletes it. Story: `docs/deployment/REGISTER.md`.
//!
//! Super Productivity keeps only its last 17 `master-<sha>` builds on ghcr.io;
//! the supersync pin of 2026-09-28 was a 404 by 2026-10-23. A rebuild or a
//! disaster recovery from a pinned digest then fails at its pull.
//!
//! The pull-through proxies on CT 117 cannot be the answer: they expire what
//! they hold (`proxy.ttl`), the registry may evict it, and the CT is
//! deliberately not backed up. So there is a SECOND, plain registry beside
//! them (the `keep` service of `stacks/registry`, no `proxy:` section, so
//! nothing ever expires by itself) that only this module writes to:
//!
//! * a round (nightly, and after every deploy) reads the digest pins out of
//!   the compose files each stack's container really runs, copies every pin
//!   the keep registry does not hold yet into it (`skopeo copy --all`, which
//!   keeps the manifest digest), and drops what no stack pins any more, so
//!   the store is exactly the set of current pins and cannot grow past it;
//! * a deploy whose pull failed (the origin said 404) points the digest-pinned
//!   `image:` lines at the keep registry and pulls once more;
//! * `homelab check` says "upstream deleted it, the kept copy at ... is used"
//!   instead of Broken when the keep registry still has the digest.
//!
//! A kept copy lives at `<host>:<keep_port>/<registry>/<repository>` and is
//! TAGGED `sha256-<hex>`. The tag is what makes the store enumerable (a
//! registry cannot list manifests that only exist by digest); pulls still go
//! by digest.

use std::collections::BTreeSet;

use crate::executor::{Executor, pct_sh};
use crate::ops::pinexists::{PinnedDigest, parse_digest_header, pinned_digests};
use crate::ops::registry_cache::CacheCfg;
use crate::state::HostState;

/// The compose app on the registry stack that holds the `keep` registry and
/// the `keep-copy` tool (skopeo, profile `tool`).
pub const KEEP_APP_DIR: &str = "/opt/registry/keep";
pub const KEEP_SERVICE: &str = "keep";
pub const KEEP_COPY_SERVICE: &str = "keep-copy";
/// Where the keep registry's data lives inside CT 117 (a stack-file mount).
pub const KEEP_DATA_DIR: &str = "/appdata/registry/keep-config";
/// Never more kept digests than this, whatever the stack files say.
pub const KEEP_MAX_COPIES: usize = 80;
/// At most this many new copies per round; the rest wait for the next one.
pub const KEEP_COPIES_PER_ROUND: usize = 12;
pub const COPY_TIMEOUT_S: u64 = 900;

const ACCEPT: &str = "application/vnd.oci.image.index.v1+json, \
application/vnd.docker.distribution.manifest.list.v2+json, \
application/vnd.oci.image.manifest.v1+json, \
application/vnd.docker.distribution.manifest.v2+json";

/// One pinned digest, by origin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pin {
    /// `ghcr.io`, `docker.io` (Hub in one spelling), ...
    pub registry: String,
    /// `owner/name`; Hub official images carry their `library/`.
    pub repository: String,
    /// `sha256:<hex>`.
    pub digest: String,
}

impl Pin {
    /// The tag a kept copy carries: `sha256-<hex>`.
    pub fn tag(&self) -> String {
        format!("sha256-{}", self.digest.trim_start_matches("sha256:"))
    }

    /// The repository name inside the keep registry.
    pub fn kept_repo(&self) -> String {
        format!("{}/{}", self.registry, self.repository)
    }

    /// The reference that pulls the kept copy.
    pub fn kept_ref(&self, host: &str, port: u16) -> String {
        format!("{}:{}/{}@{}", host, port, self.kept_repo(), self.digest)
    }

    /// The reference that pulls the origin.
    pub fn origin_ref(&self) -> String {
        format!("{}/{}@{}", self.registry, self.repository, self.digest)
    }
}

fn canonical_registry(r: &str) -> String {
    match r {
        "registry-1.docker.io" | "index.docker.io" | "docker.io" => "docker.io".into(),
        other => other.to_string(),
    }
}

/// A repository name the keep registry can hold: lower-case path components
/// of `[a-z0-9._-]`. A registry with a port in it (`host:5000/...`) cannot
/// be nested in a repository name and is not kept.
fn keepable(registry: &str, repository: &str) -> bool {
    let ok = |s: &str| {
        !s.is_empty()
            && s.split('/').all(|c| {
                !c.is_empty()
                    && c.chars().all(|ch| {
                        ch.is_ascii_lowercase() || ch.is_ascii_digit() || "._-".contains(ch)
                    })
            })
    };
    ok(registry) && ok(repository)
}

/// The pin a [`PinnedDigest`] (as the stack-file reader gives it) stands for.
pub fn pin_of(p: &PinnedDigest) -> Option<Pin> {
    let registry = canonical_registry(&p.registry);
    keepable(&registry, &p.repository).then(|| Pin {
        registry,
        repository: p.repository.clone(),
        digest: p.digest.clone(),
    })
}

/// Every pin in a compose file as it is in the REPOSITORY (origin names).
pub fn pins_in_compose(compose: &str) -> Vec<Pin> {
    pinned_digests(compose).iter().filter_map(pin_of).collect()
}

/// Every pin in a compose file as it sits in a CONTAINER: a deploy rewrote
/// the origin to the cache (`host:5001/owner/name@sha256:...`), and a keep
/// fallback to `host:<keep_port>/<registry>/<repository>@sha256:...`. Both
/// map back to the origin so the same pin is seen whichever way it was
/// deployed.
pub fn pins_in_deployed_compose(compose: &str, cfg: &CacheCfg) -> Vec<Pin> {
    let mut out = Vec::new();
    for p in pinned_digests(compose) {
        // The reference still carries `@sha256:…`; its ':' is not a tag's.
        let name = p
            .reference
            .split_once('@')
            .map_or(&*p.reference, |(n, _)| n);
        let Some((first, rest)) = name.split_once('/') else {
            out.extend(pin_of(&p));
            continue;
        };
        let name_path = rest.rsplit_once(':').map_or(rest, |(n, _)| n);
        // The registry part of a pinned_digests() reference is not usable
        // for a host:port first segment (its repository dropped nothing),
        // so decide here.
        if let Some(port) = first
            .strip_prefix(&format!("{}:", cfg.host))
            .and_then(|p| p.parse::<u16>().ok())
        {
            if Some(port) == cfg.keep_port {
                if let Some((registry, repository)) = name_path.split_once('/') {
                    let pin = Pin {
                        registry: canonical_registry(registry),
                        repository: repository.to_string(),
                        digest: p.digest.clone(),
                    };
                    if keepable(&pin.registry, &pin.repository) {
                        out.push(pin);
                    }
                }
            } else if let Some(up) = cfg.upstreams.iter().find(|u| u.port == port) {
                let pin = Pin {
                    registry: canonical_registry(&up.registry),
                    repository: name_path.to_string(),
                    digest: p.digest.clone(),
                };
                if keepable(&pin.registry, &pin.repository) {
                    out.push(pin);
                }
            }
            continue;
        }
        out.extend(pin_of(&p));
    }
    out
}

/// Point every digest-pinned `image:` line at the keep registry; every other
/// line, tag-only images and `skip_registry` (a registry this stack signs
/// into, whose images are private and never kept) included, is untouched.
pub fn rewrite_to_keep(
    compose: &str,
    host: &str,
    port: u16,
    skip_registry: Option<&str>,
) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in compose.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("image:")
            && let Some(p) = pins_in_compose(trimmed).into_iter().next()
            && Some(p.registry.as_str()) != skip_registry
        {
            let indent = &line[..line.len() - trimmed.len()];
            out.push(format!("{}image: {}", indent, p.kept_ref(host, port)));
            continue;
        }
        out.push(line.to_string());
    }
    out.join("\n") + if compose.ends_with('\n') { "\n" } else { "" }
}

/// What a round has to do: pure, so it can be tested.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Plan {
    /// Pins the keep registry does not hold yet (at most
    /// [`KEEP_COPIES_PER_ROUND`], and never past [`KEEP_MAX_COPIES`] in all).
    pub copy: Vec<Pin>,
    /// Held `(repository, tag)` that no stack pins any more.
    pub drop: Vec<(String, String)>,
    /// Pins left for the next round or refused for the cap, with the reason.
    pub notes: Vec<String>,
}

/// `held` is every `(kept repository, tag)` the keep registry lists with a
/// `sha256-` tag. `complete` is false when a stack's compose could not be
/// read this round: then `wanted` may be missing pins, and dropping on that
/// would delete the only copy of one. An EMPTY `wanted` never drops either.
pub fn plan(wanted: &BTreeSet<Pin>, held: &BTreeSet<(String, String)>, complete: bool) -> Plan {
    let mut p = Plan::default();
    let want_keys: BTreeSet<(String, String)> =
        wanted.iter().map(|w| (w.kept_repo(), w.tag())).collect();
    let missing: Vec<&Pin> = wanted
        .iter()
        .filter(|w| !held.contains(&(w.kept_repo(), w.tag())))
        .collect();
    let room = KEEP_MAX_COPIES.saturating_sub(held.len());
    let take = missing.len().min(KEEP_COPIES_PER_ROUND).min(room);
    p.copy = missing.iter().take(take).map(|m| (*m).clone()).collect();
    if missing.len() > take {
        p.notes.push(format!(
            "{} more pin(s) wait for the next round (at most {} new copies a round, {} in all)",
            missing.len() - take,
            KEEP_COPIES_PER_ROUND,
            KEEP_MAX_COPIES
        ));
    }
    if complete && !wanted.is_empty() {
        p.drop = held.difference(&want_keys).cloned().collect();
    } else if !held.is_empty() && held != &want_keys {
        p.notes
            .push("nothing dropped this round: the pins of every stack could not be read".into());
    }
    p
}

/// The shell line that copies one pin from `src` into the keep registry,
/// run inside CT 117. `src_plain` = the source speaks plain HTTP (the cache).
pub fn copy_script(pin: &Pin, src: &str, src_plain: bool, keep_port: u16) -> String {
    format!(
        "cd {dir} && docker compose run --rm -T {svc} copy --all --preserve-digests \
         {srctls} --dest-tls-verify=false 'docker://{src}' \
         'docker://127.0.0.1:{port}/{repo}:{tag}'",
        dir = KEEP_APP_DIR,
        svc = KEEP_COPY_SERVICE,
        srctls = if src_plain {
            "--src-tls-verify=false"
        } else {
            "--src-tls-verify=true"
        },
        src = src,
        port = keep_port,
        repo = pin.kept_repo(),
        tag = pin.tag()
    )
}

/// The `repositories` of a registry `_catalog` answer.
pub fn parse_catalog(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("repositories").and_then(|r| r.as_array()).map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// The `sha256-` tags of a registry `tags/list` answer.
pub fn parse_tags(body: &str) -> Vec<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("tags").and_then(|r| r.as_array()).map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str())
                    .filter(|t| t.starts_with("sha256-"))
                    .map(str::to_string)
                    .collect()
            })
        })
        .unwrap_or_default()
}

/// The pins of every compose stack, read from the containers (what is really
/// deployed). `complete` is false when any stack could not be read.
pub async fn gather_wanted(
    exec: &dyn Executor,
    state: &HostState,
    cfg: &CacheCfg,
    no_touch: &[u16],
) -> (BTreeSet<Pin>, bool, Vec<String>) {
    let mut wanted = BTreeSet::new();
    let mut complete = true;
    let mut notes = Vec::new();
    for (name, st) in &state.stacks {
        if st.is_native() || no_touch.contains(&st.vmid) {
            continue;
        }
        let skip = st
            .manifest
            .as_ref()
            .and_then(|m| m.registry_login.as_ref())
            .map(|r| r.registry.clone());
        let script = format!("cat /opt/{}/*/docker-compose.yml 2>/dev/null; true", name);
        match pct_sh(exec, st.vmid, &script, 60).await {
            Ok(o) if o.success() => {
                for pin in pins_in_deployed_compose(&o.stdout, cfg) {
                    if Some(pin.registry.as_str()) != skip.as_deref() {
                        wanted.insert(pin);
                    }
                }
            }
            Ok(o) => {
                complete = false;
                notes.push(format!(
                    "[keep] {} (CT {}): compose files not read: {}",
                    name,
                    st.vmid,
                    o.stderr.trim()
                ));
            }
            Err(e) => {
                complete = false;
                notes.push(format!(
                    "[keep] {} (CT {}): compose files not read: {}",
                    name, st.vmid, e
                ));
            }
        }
    }
    (wanted, complete, notes)
}

fn local(port: u16, path: &str) -> String {
    format!("http://127.0.0.1:{}/v2/{}", port, path)
}

/// One keep round, run on the registry stack's guest. One log line per thing
/// done or refused; a failure is a line, never an error (the round is a
/// safety net, not a gate).
pub async fn round(
    exec: &dyn Executor,
    cfg: &CacheCfg,
    cache_vmid: u16,
    wanted: &BTreeSet<Pin>,
    complete: bool,
) -> Vec<String> {
    let mut lines = Vec::new();
    let Some(port) = cfg.keep_port else {
        return lines;
    };
    // 1. what is held
    let mut held: BTreeSet<(String, String)> = BTreeSet::new();
    let cat = pct_sh(
        exec,
        cache_vmid,
        &format!("curl -fsS -m 20 '{}'", local(port, "_catalog?n=1000")),
        40,
    )
    .await;
    match cat {
        Ok(o) if o.success() => {
            for repo in parse_catalog(&o.stdout) {
                let t = pct_sh(
                    exec,
                    cache_vmid,
                    &format!(
                        "curl -fsS -m 20 '{}'",
                        local(port, &format!("{repo}/tags/list"))
                    ),
                    40,
                )
                .await;
                if let Ok(t) = t
                    && t.success()
                {
                    for tag in parse_tags(&t.stdout) {
                        held.insert((repo.clone(), tag));
                    }
                }
            }
        }
        Ok(o) => {
            lines.push(format!(
                "[keep] the keep registry on port {} does not answer: {}",
                port,
                o.stderr.trim()
            ));
            return lines;
        }
        Err(e) => {
            lines.push(format!(
                "[keep] the keep registry on port {port} not asked: {e}"
            ));
            return lines;
        }
    }
    let plan = plan(wanted, &held, complete);
    lines.extend(plan.notes.iter().map(|n| format!("[keep] {n}")));

    // 2. copy what is missing, within the disk cap
    let mut used_mb = disk_used_mb(exec, cache_vmid).await;
    for pin in &plan.copy {
        if let Some(u) = used_mb
            && u >= cfg.keep_cap_mb()
        {
            lines.push(format!(
                "[keep] {} MB used of the {} MB cap — no more copies until something is dropped",
                u,
                cfg.keep_cap_mb()
            ));
            break;
        }
        let mut sources: Vec<(String, bool)> = vec![(pin.origin_ref(), false)];
        if let Some(up) = cfg.upstreams.iter().find(|u| u.registry == pin.registry) {
            sources.push((
                format!("{}:{}/{}@{}", cfg.host, up.port, pin.repository, pin.digest),
                true,
            ));
        }
        let mut done = false;
        for (src, plain) in &sources {
            let r = pct_sh(
                exec,
                cache_vmid,
                &copy_script(pin, src, *plain, port),
                COPY_TIMEOUT_S,
            )
            .await;
            if matches!(&r, Ok(o) if o.success()) && verify(exec, cache_vmid, port, pin).await {
                lines.push(format!(
                    "[keep] {} kept ({})",
                    pin.origin_ref(),
                    if *plain {
                        "from the cache"
                    } else {
                        "from its registry"
                    }
                ));
                done = true;
                break;
            }
        }
        if !done {
            lines.push(format!(
                "[keep] {} could not be kept: neither its registry nor the cache delivered it",
                pin.origin_ref()
            ));
        }
        used_mb = disk_used_mb(exec, cache_vmid).await;
    }

    // 3. drop what nothing pins any more, then give the blobs back
    let mut dropped = 0;
    for (repo, tag) in &plan.drop {
        let digest = tag_digest(exec, cache_vmid, port, repo, tag).await;
        let Some(digest) = digest else { continue };
        let del = pct_sh(
            exec,
            cache_vmid,
            &format!(
                "curl -fsS -m 20 -X DELETE -o /dev/null '{}'",
                local(port, &format!("{repo}/manifests/{digest}"))
            ),
            40,
        )
        .await;
        if matches!(&del, Ok(o) if o.success()) {
            dropped += 1;
            lines.push(format!(
                "[keep] {repo}@{digest} dropped: no stack pins it any more"
            ));
        }
    }
    if dropped > 0 {
        let gc = pct_sh(
            exec,
            cache_vmid,
            &format!(
                "cd {} && docker compose exec -T {} registry garbage-collect --delete-untagged \
                 /etc/distribution/config.yml",
                KEEP_APP_DIR, KEEP_SERVICE
            ),
            600,
        )
        .await;
        match gc {
            Ok(o) if o.success() => {
                lines.push(format!("[keep] garbage collected after {dropped} drop(s)"))
            }
            Ok(o) => lines.push(format!(
                "[keep] garbage collection failed: {}",
                o.stderr.trim()
            )),
            Err(e) => lines.push(format!("[keep] garbage collection not run: {e}")),
        }
    }
    lines
}

async fn disk_used_mb(exec: &dyn Executor, vmid: u16) -> Option<u64> {
    let o = pct_sh(
        exec,
        vmid,
        &format!("du -sm {} 2>/dev/null | cut -f1", KEEP_DATA_DIR),
        120,
    )
    .await
    .ok()?;
    o.stdout.trim().parse().ok()
}

async fn tag_digest(
    exec: &dyn Executor,
    vmid: u16,
    port: u16,
    repo: &str,
    tag: &str,
) -> Option<String> {
    let o = pct_sh(
        exec,
        vmid,
        &format!(
            "curl -fsS -m 20 -I -H 'Accept: {}' '{}'",
            ACCEPT,
            local(port, &format!("{repo}/manifests/{tag}"))
        ),
        40,
    )
    .await
    .ok()?;
    o.success()
        .then(|| parse_digest_header(&o.stdout))
        .flatten()
}

/// The copy is only a copy when the digest under its tag is the pin's own.
/// A different one is deleted again: a kept image that is not the pinned one
/// would be worse than none.
async fn verify(exec: &dyn Executor, vmid: u16, port: u16, pin: &Pin) -> bool {
    let got = tag_digest(exec, vmid, port, &pin.kept_repo(), &pin.tag()).await;
    if got.as_deref() == Some(pin.digest.as_str()) {
        return true;
    }
    if let Some(d) = got {
        let _ = pct_sh(
            exec,
            vmid,
            &format!(
                "curl -fsS -m 20 -X DELETE -o /dev/null '{}'",
                local(port, &format!("{}/manifests/{}", pin.kept_repo(), d))
            ),
            40,
        )
        .await;
    }
    false
}
