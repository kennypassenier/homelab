//! fix-83: what each pinned `manual` app runs, and whether upstream moved on.
//!
//! fix-82 pinned every `manual` image to the version and digest it ran, so a
//! rebuild can no longer jump ahead — and the same pin also means nothing
//! ever moves by itself without this module looking. Story:
//! `docs/deployment/REGISTER.md`.
//!
//! So the nightly round does two things here. It reads, per compose stack,
//! which digest every `manual` container actually runs and keeps that in host
//! state; and for each container that declares where its releases come from
//! (`com.homelab.update.upstream=github.com/<owner>/<repo>`), it asks GitHub
//! for the latest release, at most once a night, and the fleet check prints a
//! `noted` line when the pinned version is older. Noted, because being behind
//! is not a fault: moving stays Kenny's decision.
//!
//! GitHub is reached the way `native::release_update` already reaches it:
//! `curl` from the host against the public API, no token. Every call carries
//! `-m`, answers are cached in state for [`UPSTREAM_MAX_AGE_S`], and the first
//! call that gets no answer at all stops the round, so an unreachable GitHub
//! costs one timeout rather than one per upstream.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::executor::{Cmd, Executor, pct_sh};
use crate::ops::fleetcheck::{Finding, Severity};
use crate::state::HostState;

/// The label a `manual` service uses to say where its releases are
/// announced. Only `github.com/<owner>/<repo>` is understood.
pub const UPSTREAM_LABEL: &str = "com.homelab.update.upstream";

/// How long one answer from GitHub stands. Under a day so every nightly
/// round asks once; well over the length of a round so a restarted daemon or
/// a second check the same night does not ask again.
pub const UPSTREAM_MAX_AGE_S: u64 = 20 * 3600;

/// The bound on one GitHub call, in seconds (curl's `-m`). The command gets
/// ten more so curl's own timeout is the one that fires and says so.
const GITHUB_TIMEOUT_S: u64 = 20;

/// One `manual` container as the nightly round last saw it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RunningImage {
    /// The reference the container was started from (`.Config.Image`), after
    /// the deploy's cache rewrite.
    pub image: String,
    /// The digest of the image it runs, `sha256:...`.
    pub digest: String,
    /// Its `com.homelab.update.upstream` label, when it has one.
    #[serde(default)]
    pub upstream: Option<String>,
    /// Unix time this was read.
    pub seen_at: u64,
}

/// The last answer about one upstream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UpstreamRelease {
    /// Unix time it was asked (or skipped); the cache key for the night.
    pub checked_at: u64,
    /// The latest release's tag.
    #[serde(default)]
    pub latest: Option<String>,
    /// When that release was published, as GitHub wrote it.
    #[serde(default)]
    pub published_at: Option<String>,
    /// Why there is no answer. None when there is one.
    #[serde(default)]
    pub error: Option<String>,
}

/// What one nightly round read, applied to state by [`apply`].
#[derive(Debug, Clone, Default)]
pub struct PinFacts {
    /// Stack -> container -> what it runs. Only stacks that could be read.
    pub images: BTreeMap<String, BTreeMap<String, RunningImage>>,
    /// Upstreams asked (or deliberately skipped) this round.
    pub releases: BTreeMap<String, UpstreamRelease>,
}

/// Inside the container: one line per running `manual` container,
/// `name|config image|image id|upstream label|repo digests`.
const INSPECT: &str = "for c in $(docker ps -q --filter label=com.homelab.update.policy=manual); do \
     i=$(docker inspect --format '{{.Image}}' \"$c\"); \
     echo \"$(docker inspect --format '{{.Name}}|{{.Config.Image}}|{{.Image}}|{{index .Config.Labels \"com.homelab.update.upstream\"}}' \"$c\")|$(docker image inspect --format '{{join .RepoDigests \",\"}}' \"$i\")\"; \
     done";

/// Parse the inspect loop's output. The digest is the pinned one when the
/// image carries it (and the image's own digests agree), otherwise the first
/// digest docker holds for the image, otherwise its id.
pub fn parse_inspect(stdout: &str, now: u64) -> BTreeMap<String, RunningImage> {
    let mut out = BTreeMap::new();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.trim().splitn(5, '|').collect();
        if parts.len() < 5 {
            continue;
        }
        let name = parts[0].trim_start_matches('/').to_string();
        let image = parts[1].to_string();
        let id = parts[2];
        let repo_digests: Vec<&str> = parts[4]
            .split(',')
            .filter_map(|d| d.split_once('@').map(|(_, x)| x))
            .collect();
        let pinned = image.split_once('@').map(|(_, d)| d);
        let digest = match pinned {
            Some(p) if repo_digests.contains(&p) => p.to_string(),
            _ => repo_digests
                .first()
                .map(|d| d.to_string())
                .unwrap_or_else(|| id.to_string()),
        };
        let upstream = Some(parts[3].trim())
            .filter(|u| !u.is_empty() && *u != "<no value>")
            .map(str::to_string);
        if name.is_empty() {
            continue;
        }
        out.insert(
            name,
            RunningImage {
                image,
                digest,
                upstream,
                seen_at: now,
            },
        );
    }
    out
}

/// redesign-flows-6 (the Update flow's verify): what one stack's containers
/// run right now, read fresh after a deploy — every container with whether
/// it runs, and each `manual` one's image. The dashboard reads it through
/// `Command::StackRuntime`; an older host does not know that command, and
/// the flow then says the version was not reported rather than inventing it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackRuntime {
    /// Every container of the guest (`docker ps -a`), with whether it runs.
    pub containers: Vec<(String, bool)>,
    /// Each `manual` container's image, as the nightly round reads it.
    pub images: BTreeMap<String, RunningImage>,
}

const RUNTIME_MARK: &str = "--manual--";

/// The one script [`read_runtime`] runs in the guest.
pub fn runtime_script() -> String {
    format!(
        "docker ps -a --format '{{{{.Names}}}}|{{{{.State}}}}'; echo '{RUNTIME_MARK}'; {INSPECT}"
    )
}

/// Parse [`runtime_script`]'s output.
pub fn parse_runtime(stdout: &str, now: u64) -> StackRuntime {
    let (ps, manual) = stdout.split_once(RUNTIME_MARK).unwrap_or((stdout, ""));
    let containers = ps
        .lines()
        .filter_map(|l| {
            let (name, state) = l.trim().split_once('|')?;
            (!name.is_empty()).then(|| (name.to_string(), state.trim() == "running"))
        })
        .collect();
    StackRuntime {
        containers,
        images: parse_inspect(manual, now),
    }
}

/// Read one guest's [`StackRuntime`] now.
pub async fn read_runtime(
    exec: &dyn Executor,
    vmid: u16,
    now: u64,
) -> Result<StackRuntime, String> {
    match pct_sh(exec, vmid, &runtime_script(), 60).await {
        Ok(o) if o.success() => Ok(parse_runtime(&o.stdout, now)),
        Ok(o) => Err(format!(
            "could not read CT {vmid}'s containers: {}",
            o.stderr.trim()
        )),
        Err(e) => Err(format!("could not read CT {vmid}'s containers: {e}")),
    }
}

/// The repository part of an image reference: no registry host, no tag, no
/// digest (`ghcr.io/example/demo-api:v2@sha256:…` → `example/demo-api`), so
/// a deploy's registry-cache rewrite still names the same image.
fn repository(image: &str) -> String {
    let name = image.split('@').next().unwrap_or(image);
    let (head, last) = name.rsplit_once('/').unwrap_or(("", name));
    let last = last.split(':').next().unwrap_or(last);
    let mut parts: Vec<&str> = head.split('/').filter(|p| !p.is_empty()).collect();
    if parts
        .first()
        .is_some_and(|p| p.contains('.') || p.contains(':') || *p == "localhost")
    {
        parts.remove(0);
    }
    parts.push(last);
    parts.join("/")
}

/// redesign-flows-6: whether a container of `running` runs `want` (an image
/// line the Update flow committed): the same digest, or the same repository
/// at the same version. `None` when no `manual` container runs that
/// repository at all.
pub fn runs_image(running: &BTreeMap<String, RunningImage>, want: &str) -> Option<bool> {
    let repo = repository(want);
    let digest = want.split_once('@').map(|(_, d)| d);
    let version = pinned_version(want);
    let same: Vec<&RunningImage> = running
        .values()
        .filter(|r| repository(&r.image) == repo)
        .collect();
    if same.is_empty() {
        return None;
    }
    Some(same.iter().all(|r| {
        digest.is_some_and(|d| r.digest == d)
            || (version.is_some() && pinned_version(&r.image) == version)
    }))
}

/// The version a reference is pinned to: the tag in its last path segment,
/// without the digest. None for a digest-only reference.
pub fn pinned_version(image: &str) -> Option<String> {
    let name = image.split('@').next().unwrap_or(image);
    let last = name.rsplit('/').next().unwrap_or(name);
    last.split_once(':').map(|(_, t)| t.to_string())
}

fn numbers(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// Whether `pinned` is older than `latest`, comparing the numbers in each in
/// order. Decoration (`v`, `-alpine`, linuxserver's `-ls474`) does not count;
/// a version with no numbers is never called behind.
pub fn is_behind(pinned: &str, latest: &str) -> bool {
    let (p, l) = (numbers(pinned), numbers(latest));
    !p.is_empty() && !l.is_empty() && p < l
}

/// Read what every compose stack's `manual` containers run, and ask GitHub
/// about each upstream whose answer is older than tonight.
///
/// Read-only on the containers. A container that cannot be read is named in
/// the notes and left out, so [`apply`] keeps what was last known about it.
pub async fn gather(
    exec: &dyn Executor,
    state: &HostState,
    no_touch: &[u16],
    now: u64,
    max_age_s: u64,
) -> (PinFacts, Vec<String>) {
    let mut facts = PinFacts::default();
    let mut notes = Vec::new();
    for (name, st) in &state.stacks {
        if st.is_native() || no_touch.contains(&st.vmid) {
            continue;
        }
        match pct_sh(exec, st.vmid, INSPECT, 60).await {
            Ok(o) if o.success() => {
                facts
                    .images
                    .insert(name.clone(), parse_inspect(&o.stdout, now));
            }
            Ok(o) => notes.push(format!(
                "[pins] {} (CT {}): could not read its containers: {}",
                name,
                st.vmid,
                o.stderr.trim()
            )),
            Err(e) => notes.push(format!(
                "[pins] {} (CT {}): could not read its containers: {}",
                name, st.vmid, e
            )),
        }
    }

    let upstreams: BTreeSet<String> = facts
        .images
        .values()
        .flat_map(|m| m.values())
        .filter(|r| pinned_version(&r.image).is_some())
        .filter_map(|r| r.upstream.clone())
        .collect();
    let mut unreachable: Option<String> = None;
    for up in upstreams {
        let fresh = state
            .upstream_releases
            .get(&up)
            .is_some_and(|r| now.saturating_sub(r.checked_at) < max_age_s);
        if fresh {
            continue;
        }
        let mut rec = UpstreamRelease {
            checked_at: now,
            ..Default::default()
        };
        if let Some(first) = &unreachable {
            rec.error = Some(format!(
                "not asked tonight: GitHub did not answer for {}",
                first
            ));
            facts.releases.insert(up, rec);
            continue;
        }
        let Some(repo) = up
            .strip_prefix("github.com/")
            .filter(|r| r.split('/').count() == 2)
        else {
            rec.error = Some(format!(
                "'{}' is not github.com/<owner>/<repo>, the only upstream this check can ask",
                up
            ));
            facts.releases.insert(up, rec);
            continue;
        };
        let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
        let bound = GITHUB_TIMEOUT_S.to_string();
        let out = exec
            .run(&Cmd::new(
                "curl",
                &[
                    "-sSL",
                    "-m",
                    &bound,
                    "-H",
                    "Accept: application/vnd.github+json",
                    &url,
                ],
                GITHUB_TIMEOUT_S + 10,
            ))
            .await;
        match out {
            Ok(o) if o.success() => match parse_release(&o.stdout) {
                Ok((tag, published)) => {
                    rec.latest = Some(tag);
                    rec.published_at = published;
                }
                Err(e) => rec.error = Some(e),
            },
            Ok(o) => {
                rec.error = Some(format!("GitHub did not answer: {}", o.stderr.trim()));
                unreachable = Some(up.clone());
            }
            Err(e) => {
                rec.error = Some(format!("GitHub did not answer: {}", e));
                unreachable = Some(up.clone());
            }
        }
        facts.releases.insert(up, rec);
    }
    (facts, notes)
}

/// `tag_name` and `published_at` of a `releases/latest` answer, or what
/// GitHub said instead (a 404 or a rate limit arrives as JSON with a
/// `message`).
fn parse_release(body: &str) -> Result<(String, Option<String>), String> {
    let v: serde_json::Value =
        serde_json::from_str(body).map_err(|e| format!("GitHub's answer is not JSON: {}", e))?;
    match v["tag_name"].as_str() {
        Some(t) => Ok((
            t.to_string(),
            v["published_at"].as_str().map(str::to_string),
        )),
        None => Err(format!(
            "GitHub answered without a release: {}",
            v["message"].as_str().unwrap_or("no tag_name")
        )),
    }
}

/// Fold one round into state: the stacks that were read replace their
/// record, whatever [`is_current`] no longer holds is pruned, and every
/// upstream asked tonight replaces its cached answer.
pub fn apply(state: &mut HostState, facts: PinFacts) {
    for (stack, images) in facts.images {
        state.running_images.insert(stack, images);
    }
    prune(state);
    for (up, rec) in facts.releases {
        state.upstream_releases.insert(up, rec);
    }
}

/// fix-227 (Kenny, 2026-10-02): whether a recorded `(stack, container)` is
/// still part of the fleet. `running_images` is a nightly snapshot, so a
/// stack destroyed or an app dropped during the day stayed in it until the
/// next round, and the fleet check (with the dashboard's Stale images table
/// behind it) kept naming `uptime/cadvisor`, `home/cadvisor` and
/// `metrics/grafana` after all three had been retired.
///
/// Current means: the stack is in state, and the container is not an app
/// the stack retired (`HostState::retired`, keyed `<stack>/<name>`) unless
/// the stack declares it again. A redeploy that brings an app back also
/// drops its retired record (`retired::unretire`), so a live app is never
/// hidden. Containers that are not apps themselves (a guard's cAdvisor, or
/// the several caches one app runs) stay current while their stack is.
pub fn is_current(state: &HostState, stack: &str, container: &str) -> bool {
    let Some(st) = state.stacks.get(stack) else {
        return false;
    };
    let declared = st.apps.iter().any(|a| a == container)
        || st
            .manifest
            .as_ref()
            .is_some_and(|m| m.apps.iter().any(|a| a == container));
    declared
        || !state
            .retired
            .contains_key(&format!("{}/{}", stack, container))
}

/// Drop every record [`is_current`] rejects, and stacks left with none.
/// Run by the nightly [`apply`] and by the destroy and deploy that retire a
/// stack or app (`retired::retire_stack`, `retired::retire_app`); the fleet
/// check, which only reads state, applies the same rule in
/// [`evaluate_pins`]. So a retired stack or app leaves every list at once.
pub fn prune(state: &mut HostState) {
    let mut kept = std::mem::take(&mut state.running_images);
    for (stack, images) in kept.iter_mut() {
        images.retain(|c, _| is_current(state, stack, c));
    }
    kept.retain(|s, images| state.stacks.contains_key(s) && !images.is_empty());
    state.running_images = kept;
}

/// The fleet check's half: one `noted` finding per pinned version that its
/// upstream has moved past (cAdvisor on twelve stacks is one line, not
/// twelve), and one naming every upstream that could not be asked.
pub fn evaluate_pins(state: &HostState) -> Vec<Finding> {
    // (upstream, pinned version, latest) -> where it runs.
    let mut behind: BTreeMap<(String, String, String), Vec<String>> = BTreeMap::new();
    let mut failed: BTreeMap<String, String> = BTreeMap::new();
    for (stack, images) in &state.running_images {
        for (container, rec) in images {
            // fix-227: the record is a nightly snapshot; a stack or app
            // retired since is not reported.
            if !is_current(state, stack, container) {
                continue;
            }
            let (Some(up), Some(version)) = (rec.upstream.as_ref(), pinned_version(&rec.image))
            else {
                continue;
            };
            let Some(release) = state.upstream_releases.get(up) else {
                continue;
            };
            if let Some(e) = &release.error {
                failed.insert(up.clone(), e.clone());
                continue;
            }
            let Some(latest) = release.latest.as_ref() else {
                continue;
            };
            if is_behind(&version, latest) {
                behind
                    .entry((up.clone(), version, latest.clone()))
                    .or_default()
                    .push(format!("{}/{}", stack, container));
            }
        }
    }
    let mut out = Vec::new();
    for ((up, version, latest), mut wheres) in behind {
        wheres.sort();
        let published = state
            .upstream_releases
            .get(&up)
            .and_then(|r| r.published_at.as_deref())
            .map(|p| format!(" on {}", p.get(..10).unwrap_or(p)))
            .unwrap_or_default();
        out.push(Finding {
            severity: Severity::Noted,
            subject: wheres.join(", "),
            what: format!(
                "pinned to {}; upstream {} released {}{}",
                version, up, latest, published
            ),
            remedy: "nothing is urgent. To move, put the new version and its digest on the \
                     `image:` line and deploy the stack (UPDATE_POLICY.md)"
                .into(),
        });
    }
    if !failed.is_empty() {
        out.push(Finding {
            severity: Severity::Noted,
            subject: "upstream releases".into(),
            what: format!(
                "not known tonight for {}",
                failed
                    .iter()
                    .map(|(u, e)| format!("{} ({})", u, e))
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
            remedy: "nothing to do; the next nightly round asks again".into(),
        });
    }
    out
}
