//! Small pure helpers over native-service manifests and staged binaries.

use crate::native::NativeServiceManifest;
use crate::ops::util::shq;

/// T85: fill a deploy's binary map from what was staged one message at a
/// time. A unit whose entry is missing or empty is looked up; a unit that
/// arrived with its bytes (an older client) is left alone; an entry that is
/// empty with nothing staged is dropped, so the deploy treats the binary as
/// not shipped rather than as an empty program. Returns the units filled.
pub fn merge_staged_binaries(
    natives: &[String],
    map: &mut std::collections::BTreeMap<String, String>,
    fetch: impl Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut merged = Vec::new();
    for unit in natives {
        let present = map.get(unit).map(|v| !v.trim().is_empty()).unwrap_or(false);
        if present {
            continue;
        }
        match fetch(unit) {
            Some(b64) if !b64.trim().is_empty() => {
                map.insert(unit.clone(), b64);
                merged.push(unit.clone());
            }
            _ => {}
        }
    }
    map.retain(|_, v| !v.trim().is_empty());
    merged
}

/// fix-114: which unit of a native stack a per-unit verb acts on — the one
/// named, or the only one there is.
pub fn select_unit(
    services: &[NativeServiceManifest],
    unit: Option<&str>,
) -> Result<NativeServiceManifest, String> {
    let names = || {
        services
            .iter()
            .map(|s| s.unit.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    match unit {
        Some(u) => services
            .iter()
            .find(|s| s.unit == u)
            .cloned()
            .ok_or_else(|| format!("no unit '{}' on this stack (it has: {})", u, names())),
        None if services.len() == 1 => Ok(services[0].clone()),
        None => Err(format!(
            "this stack has several units ({}) — name one as <stack>/<unit>",
            names()
        )),
    }
}

/// The unit's data dirs as one shell-quoted argument list.
pub(super) fn quoted_dirs(m: &NativeServiceManifest) -> String {
    m.data_dirs
        .iter()
        .map(|d| shq(d))
        .collect::<Vec<_>>()
        .join(" ")
}
