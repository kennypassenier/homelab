//! `config/host.toml` against the host's own running host.toml, key by key.

use serde_json::Value;

use super::{Finding, LiveFacts, Severity};

/// fix-110 (homelab-admin, 2026-10-01): `config/host.toml` against the
/// host's own running host.toml, key by key — the same shape
/// `evaluate_repo_drift` compares a stack's files in. `declared_host_config`
/// absent (an older client, the nightly round, or a repository without the
/// file yet) skips the comparison entirely, same as empty `digests` skips
/// the stack-file one. A key either side sets that the other does not, or
/// sets to a different value, is one Drift finding naming both.
pub fn evaluate_host_config_drift(live: &LiveFacts) -> Vec<Finding> {
    let Some(declared) = &live.declared_host_config else {
        return Vec::new();
    };
    // fix-170: a secret or a host-held key (`tokens`, generated and kept by
    // the host itself) is never the repository's to declare, so it is
    // never compared — belt and braces on top of the callers that already
    // leave these out of both maps.
    // The order drives only the output; sorted for a stable report.
    let keys: std::collections::BTreeSet<&String> = declared
        .keys()
        .chain(live.live_host_config.keys())
        .filter(|k| !crate::hostconfig::is_secret(k) && !crate::hostconfig::is_host_held(k))
        .collect();
    keys.into_iter()
        .filter_map(|key| key_finding(key, declared.get(key), live.live_host_config.get(key)))
        .collect()
}

/// One key's finding, when the two sides really differ.
fn key_finding(key: &str, want: Option<&Value>, have: Option<&Value>) -> Option<Finding> {
    if want == have {
        return None;
    }
    // fix-181: a key the host no longer reads at all (retired — it has
    // left `hostconfig::KEYS`, e.g. the Kuma/Grafana keys superseded by
    // later work) but still sits in the host's own host.toml is not
    // drift against the repository, which rightly declares nothing for
    // it any more — it is a leftover `homelab host apply` would clear.
    if crate::hostconfig::key_info(key).is_none() {
        return have.map(|v| retired_key(key, v));
    }
    if effectively_equal(key, want, have) {
        return None;
    }
    let describe = |v: Option<&Value>| match v {
        Some(v) => v.to_string(),
        None => "its compiled default (unset)".to_string(),
    };
    Some(Finding {
        severity: Severity::Drift,
        subject: "host.toml".into(),
        what: format!(
            "{key}: config/host.toml declares {}, the host's host.toml has {}",
            describe(want),
            describe(have)
        ),
        remedy: "`homelab host apply` to make the host match the repository, or edit \
                  config/host.toml to match the host and commit that"
            .into(),
    })
}

fn retired_key(key: &str, v: &Value) -> Finding {
    Finding {
        severity: Severity::Drift,
        subject: "host.toml".into(),
        what: format!("{key}: retired key still set on the host ({v})"),
        remedy: "`homelab host apply` removes it — the host no longer reads this key".into(),
    }
}

/// fix-181: compare EFFECTIVE values. A key either side leaves unset runs as
/// its compiled default (`hostconfig::default_effective`), so declaring that
/// same default explicitly is not a difference — only a value that actually
/// diverges from what the other side ends up running is.
fn effectively_equal(key: &str, want: Option<&Value>, have: Option<&Value>) -> bool {
    let effective = |v: Option<&Value>| {
        v.cloned()
            .or_else(|| crate::hostconfig::default_effective(key))
    };
    effective(want) == effective(have)
}
