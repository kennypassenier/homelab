//! feat-stacks-10 (overview of stale docker images): the fleet check
//! already answers one `noted` finding per pinned image whose upstream has
//! moved on (fix-83, `homelab_core::ops::pins::evaluate_pins`) — this page
//! adds nothing new to ask the host; it reads the same `/data/fleet-check`
//! findings the Health page already shows and turns the ones about pinned
//! images into a table, instead of a sentence to parse by eye.
//!
//! The finding's `subject` is "stack/container[, stack/container, …]" and
//! its `what` is "pinned to {version}; upstream {upstream} released
//! {latest} on {date}" (the exact wording `evaluate_pins` writes); this
//! module reads that back out rather than re-deriving it from host state,
//! so the table can never disagree with the fleet check's own sentence.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleImageRow {
    /// "stack/container".
    pub where_: String,
    pub pinned: String,
    pub upstream: String,
    pub latest: String,
    /// `YYYY-MM-DD`, when the finding's sentence carried one.
    pub released: Option<String>,
    /// fix-231: the `<app>/<service>` whose `image:` line in the stack's
    /// own files names this container — what the Fleet view's Update
    /// rewrites. None when no stack file names it (a pin kept in code, such
    /// as the guards' metrics agent): that one moves with a homelab release.
    pub key: Option<String>,
}

/// One finding as `/data/fleet-check` answers it: only the two fields this
/// reads.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct FindingLike {
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub what: String,
}

/// `"pinned to {version}; upstream {upstream} released {latest}[ on
/// {date}]"` — parsed back into its parts. None when the sentence does not
/// start the way `evaluate_pins` writes it (a finding from a different
/// check, or the wording changed underneath this reader).
fn parse_what(what: &str) -> Option<(String, String, String, Option<String>)> {
    let rest = what.strip_prefix("pinned to ")?;
    let (version, rest) = rest.split_once("; upstream ")?;
    let (upstream, rest) = rest.split_once(" released ")?;
    let (latest, date) = match rest.split_once(" on ") {
        Some((l, d)) => (l, Some(d.to_string())),
        None => (rest, None),
    };
    Some((
        version.to_string(),
        upstream.to_string(),
        latest.to_string(),
        date,
    ))
}

/// One table row per `stack/container` named in a finding (a finding may
/// cover several, when the same version/upstream pair shows up more than
/// once in the fleet).
pub fn from_findings(findings: &[FindingLike]) -> Vec<StaleImageRow> {
    let mut out = Vec::new();
    for f in findings {
        let Some((pinned, upstream, latest, released)) = parse_what(&f.what) else {
            continue;
        };
        for w in f.subject.split(", ").filter(|s| !s.is_empty()) {
            out.push(StaleImageRow {
                where_: w.to_string(),
                pinned: pinned.clone(),
                upstream: upstream.clone(),
                latest: latest.clone(),
                released: released.clone(),
                key: None,
            });
        }
    }
    out.sort_by(|a, b| a.where_.cmp(&b.where_));
    out
}

/// fix-231: the `<app>/<service>` of one stack's files (`texts`: path
/// relative to the stack directory → content) whose container is
/// `container` — by its `container_name`, else by compose's own naming
/// (`<service>`, or `<app>-<service>-<n>` for a project named after the
/// app directory). Only a service that carries an `image:` line counts.
pub fn locate(
    texts: &std::collections::BTreeMap<String, String>,
    container: &str,
) -> Option<String> {
    use serde_yaml::Value;
    for (path, text) in texts {
        let Some(app) = path.strip_suffix("/docker-compose.yml") else {
            continue;
        };
        if app.contains('/') {
            continue;
        }
        let Ok(v) = serde_yaml::from_str::<Value>(text) else {
            continue;
        };
        let Some(Value::Mapping(services)) = v.get("services") else {
            continue;
        };
        for (name, s) in services {
            let Some(service) = name.as_str() else {
                continue;
            };
            if s.get("image").and_then(Value::as_str).is_none() {
                continue;
            }
            let named = s.get("container_name").and_then(Value::as_str);
            let matches = match named {
                Some(n) => n == container,
                None => {
                    container == service
                        || container
                            .strip_prefix(&format!("{app}-{service}-"))
                            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                }
            };
            if matches {
                return Some(format!("{app}/{service}"));
            }
        }
    }
    None
}

/// fix-231: the image tag to move to, from the upstream's release tag:
/// a GitHub release is often `v1.2.3` while the image tag is `1.2.3` (or
/// the other way round), so the pinned tag's own `v` convention is kept.
pub fn target_tag(pinned: &str, latest: &str) -> String {
    let pinned_v = pinned.starts_with('v') && pinned[1..].starts_with(|c: char| c.is_ascii_digit());
    let latest_v = latest.starts_with('v') && latest[1..].starts_with(|c: char| c.is_ascii_digit());
    match (pinned_v, latest_v) {
        (true, false) => format!("v{latest}"),
        (false, true) => latest[1..].to_string(),
        _ => latest.to_string(),
    }
}

/// The image name without its tag and digest (`registry/repo:tag@sha256:…`
/// → `registry/repo`); a `:` inside the registry's own host:port is kept.
pub fn image_name(reference: &str) -> &str {
    let name = reference.split('@').next().unwrap_or(reference);
    match name.rsplit_once(':') {
        Some((n, tag)) if !tag.contains('/') => n,
        _ => name,
    }
}

/// fix-231: the reference that replaces `current` — same image name, the
/// new tag, the digest its registry gave for that tag.
pub fn retarget(current: &str, tag: &str, digest: &str) -> String {
    format!("{}:{tag}@{digest}", image_name(current))
}

/// The registry and repository to ask about `reference`'s tags — Docker
/// Hub's API host (`registry-1.docker.io`, `library/` for an official
/// image) in place of the `docker.io` the image name implies.
pub fn registry_of(reference: &str) -> (String, String) {
    let (registry, repository) =
        homelab_core::ops::registry_cache::split_registry(image_name(reference));
    if registry == "docker.io" {
        ("registry-1.docker.io".to_string(), repository)
    } else {
        (registry, repository)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(subject: &str, what: &str) -> FindingLike {
        FindingLike {
            subject: subject.into(),
            what: what.into(),
        }
    }

    #[test]
    fn parses_one_finding_with_a_date() {
        let rows = from_findings(&[f(
            "media/sonarr",
            "pinned to 4.0.1; upstream github.com/Sonarr/Sonarr released 4.0.2 on 2026-09-20",
        )]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].where_, "media/sonarr");
        assert_eq!(rows[0].pinned, "4.0.1");
        assert_eq!(rows[0].upstream, "github.com/Sonarr/Sonarr");
        assert_eq!(rows[0].latest, "4.0.2");
        assert_eq!(rows[0].released.as_deref(), Some("2026-09-20"));
    }

    #[test]
    fn parses_a_finding_naming_several_containers() {
        let rows = from_findings(&[f(
            "media/radarr, paperwork/paperless",
            "pinned to 1.2.3; upstream github.com/x/y released 1.3.0",
        )]);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].where_, "media/radarr");
        assert_eq!(rows[1].where_, "paperwork/paperless");
        assert!(rows[0].released.is_none());
    }

    #[test]
    fn a_finding_from_a_different_check_is_skipped() {
        let rows = from_findings(&[f(
            "gateway",
            "the files differ from what the host applied on 2026-09-20",
        )]);
        assert!(rows.is_empty());
    }

    fn texts(pairs: &[(&str, &str)]) -> std::collections::BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(p, t)| (p.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    /// covers: fix-231
    fn fix_231_locate_finds_the_service_by_container_name_or_compose_naming() {
        let t = texts(&[
            (
                "web/docker-compose.yml",
                "services:\n  web:\n    image: a/web:1.0@sha256:aa\n    container_name: demo-web\n",
            ),
            (
                "api/docker-compose.yml",
                "services:\n  api:\n    image: a/api:1.0\n  worker:\n    image: a/w:1\n",
            ),
            ("lxc-compose.yml", "stack_name: x\n"),
        ]);
        assert_eq!(locate(&t, "demo-web").as_deref(), Some("web/web"));
        // container_name wins: the bare service name no longer matches.
        assert_eq!(locate(&t, "web"), None);
        assert_eq!(locate(&t, "api").as_deref(), Some("api/api"));
        assert_eq!(locate(&t, "api-worker-1").as_deref(), Some("api/worker"));
        // A pin kept in code is in no stack file.
        assert_eq!(locate(&t, "demo-agent"), None);
    }

    #[test]
    /// covers: fix-231
    fn fix_231_target_tag_keeps_the_pinned_tags_v_convention() {
        assert_eq!(target_tag("10.11.11", "v10.11.12"), "10.11.12");
        assert_eq!(target_tag("v3.7.12", "3.8.0"), "v3.8.0");
        assert_eq!(target_tag("v3.7.12", "v3.8.0"), "v3.8.0");
        assert_eq!(
            target_tag("5.2.3_v2.0.14-ls474", "5.2.3_v2.0.14-ls475"),
            "5.2.3_v2.0.14-ls475"
        );
        // A leading "v" that is a word, not a version prefix, is left alone.
        assert_eq!(target_tag("1.0", "very-new"), "very-new");
    }

    #[test]
    /// covers: fix-231
    fn fix_231_retarget_keeps_the_name_and_swaps_tag_and_digest() {
        assert_eq!(
            retarget(
                "jellyfin/jellyfin:10.11.11@sha256:aa",
                "10.11.12",
                "sha256:bb"
            ),
            "jellyfin/jellyfin:10.11.12@sha256:bb"
        );
        assert_eq!(
            retarget("registry.local:5000/x/y:1@sha256:aa", "2", "sha256:bb"),
            "registry.local:5000/x/y:2@sha256:bb"
        );
    }

    #[test]
    /// covers: fix-231
    fn fix_231_registry_of_asks_docker_hubs_api_host() {
        assert_eq!(
            registry_of("traefik:v3.7.12@sha256:aa"),
            ("registry-1.docker.io".into(), "library/traefik".into())
        );
        assert_eq!(
            registry_of("ghcr.io/o/r:1@sha256:aa"),
            ("ghcr.io".into(), "o/r".into())
        );
    }

    #[test]
    fn rows_are_sorted_by_where() {
        let rows = from_findings(&[
            f("z/app", "pinned to 1; upstream u released 2"),
            f("a/app", "pinned to 1; upstream u released 2"),
        ]);
        assert_eq!(rows[0].where_, "a/app");
        assert_eq!(rows[1].where_, "z/app");
    }
}
