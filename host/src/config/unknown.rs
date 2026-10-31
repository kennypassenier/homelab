//! Finding the keys of host.toml that no setting reads (F186).

use super::*;

/// F186: a misplaced `host.toml` key silently lands under the wrong table
/// header and serde drops it as unknown — see REGISTER.md. Found by reading
/// the file into `FileConfig` and writing that back: whatever the round trip
/// lost, no field reads (config-four-representations). Returned as dotted
/// paths, e.g. `registry_cache.upstreams[0].opnsense_url`. A file that does
/// not deserialize returns nothing here; loading it fails loudly.
pub(crate) fn unknown_keys(raw: &toml::Table) -> Vec<String> {
    fn walk(out: &mut Vec<String>, raw: &toml::Value, read: Option<&toml::Value>, path: &str) {
        match (raw, read) {
            (toml::Value::Table(r), Some(toml::Value::Table(k))) => {
                for (key, v) in r {
                    let p = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{}.{}", path, key)
                    };
                    match k.get(key) {
                        Some(kv) => walk(out, v, Some(kv), &p),
                        None => out.push(p),
                    }
                }
            }
            (toml::Value::Array(r), Some(toml::Value::Array(k))) => {
                for (i, v) in r.iter().enumerate() {
                    walk(out, v, k.get(i), &format!("{}[{}]", path, i));
                }
            }
            _ => {}
        }
    }
    let Ok(file) = raw.clone().try_into::<FileConfig>() else {
        return Vec::new();
    };
    let Ok(read) = toml::Table::try_from(&file) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(
        &mut out,
        &toml::Value::Table(raw.clone()),
        Some(&toml::Value::Table(read)),
        "",
    );
    out.sort();
    out
}
