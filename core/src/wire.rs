//! fix-211 (Kenny, 2026-10-02: "Host weigert enkel onbekende velden" — the
//! host refuses a command only for a field it does not know, never for a
//! host merely older than the client): tells a struct this build does not
//! know a field the wire carried apart from a struct that round-trips
//! clean.
//!
//! The 2026-08-31 incident was the host silently doing less than it was
//! asked: a host one release behind read a `DeploySpec` whose struct had no
//! `data_mounts` field, serde dropped it without a word, and the deploy
//! quietly came up without the downloader's disks. `client/src/link.rs`'s
//! `refuse_older_host` used to guard the opposite direction (a client
//! refusing an older HOST outright, by version number, for every mutating
//! command); this is the host's own half of the fix, named in the decision
//! above: refuse the one command that actually carries something this
//! build cannot read, name the field, and let every other command through
//! regardless of how old the host is.
//!
//! `RpcRequest`'s wire shape (`#[serde(flatten)]` over an internally-tagged
//! `Command` enum) defeats both obvious ways to ask serde itself for the
//! unknown-field path: `serde_path_to_error` only ever reports the root
//! path ("."), and `serde_ignored`'s callback is never invoked at all —
//! both rely on driving the original `Deserializer` through the field
//! visitor, and serde's own internally-tagged/flatten machinery instead
//! buffers the payload into a private `Content` tree and replays THAT
//! through a fresh, unwrapped deserializer to pick the variant. (Verified
//! empirically against this exact shape before choosing this design —
//! see the fix-211 REGISTER.md row.) Comparing two already-materialised
//! `serde_json::Value` trees sidesteps that entirely: it needs no
//! cooperation from the `Deserializer`, so it does not care how many
//! buffering layers serde interposed.

use serde::Serialize;
use serde_json::Value;

/// Every field path present in `original`'s JSON that `typed` (the value
/// `original` was deserialized into) does not carry any more once
/// reserialized — i.e. a field this build's struct has no room for. Paths
/// read like `manifest.storage[2].data_mounts`: dotted through objects,
/// bracketed through arrays, rooted at the JSON `original` itself (an
/// `RpcRequest`'s own top-level keys included, since `cmd` and the
/// command's fields are flattened there).
///
/// A key absent from the reserialization is reported only when its
/// original value was not "empty" (null, `false`, `0`, `""`, `[]` or
/// `{}`): every `#[serde(skip_serializing_if = ...)]` field in this
/// codebase (`Option::is_none`, `Vec::is_empty`, `BTreeMap::is_empty`,
/// `std::ops::Not::not`, `BackupPause::is_off`, ...) is exactly a field the
/// struct DOES know, just skipped on the way back out because the value
/// sent was its empty one — reporting those as "unknown" would refuse a
/// perfectly ordinary deploy for naming a field explicitly as absent. The
/// cost is a blind spot the other way: a genuinely foreign field sent with
/// an empty value is not caught. The 2026-08-31 incident's `data_mounts`
/// held real paths, never an empty list, so this trade keeps the check
/// from crying wolf on ordinary traffic without losing the case it exists
/// for.
pub fn unknown_fields<T: Serialize>(original: &Value, typed: &T) -> Vec<String> {
    let roundtrip = serde_json::to_value(typed)
        .expect("a type this build just deserialized also serializes back out");
    let mut out = Vec::new();
    diff(original, &roundtrip, "", &mut out);
    out
}

fn diff(original: &Value, roundtrip: &Value, path: &str, out: &mut Vec<String>) {
    match (original, roundtrip) {
        (Value::Object(o), Value::Object(r)) => {
            for (key, ov) in o {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match r.get(key) {
                    Some(rv) => diff(ov, rv, &child, out),
                    None if is_empty_ish(ov) => {}
                    None => out.push(child),
                }
            }
        }
        (Value::Array(o), Value::Array(r)) => {
            for (i, (ov, rv)) in o.iter().zip(r.iter()).enumerate() {
                diff(ov, rv, &format!("{path}[{i}]"), out);
            }
            // A length mismatch would mean the struct's own Vec field
            // dropped or gained elements on the way out, which no field in
            // this codebase does; nothing further to compare past the
            // shorter side.
        }
        // Scalars, or the two sides disagreeing on shape entirely: nothing
        // under a mismatched or leaf node to recurse into.
        _ => {}
    }
}

/// Exactly the values `#[serde(skip_serializing_if = ...)]` is written
/// against in this codebase: the zero/empty/off value of its type.
fn is_empty_ish(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Serialize, Deserialize)]
    struct Inner {
        a: u32,
        #[serde(default)]
        b: Vec<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize)]
    struct Outer {
        #[serde(default)]
        apps: Vec<Inner>,
    }

    #[test]
    fn fix_211_a_field_the_struct_has_no_room_for_is_named_by_path() {
        let original: Value = serde_json::from_str(
            r#"{"apps":[{"a":1},{"a":2,"b":[1,2],"data_mounts":["x"]}],"top_unknown":true}"#,
        )
        .unwrap();
        let typed: Outer = serde_json::from_value(original.clone()).unwrap();
        let unknown = unknown_fields(&original, &typed);
        assert_eq!(unknown, vec!["apps[1].data_mounts", "top_unknown"]);
    }

    #[test]
    fn fix_211_a_payload_with_only_known_fields_names_nothing() {
        let original: Value = serde_json::from_str(r#"{"apps":[{"a":1,"b":[1,2]}]}"#).unwrap();
        let typed: Outer = serde_json::from_value(original.clone()).unwrap();
        assert!(unknown_fields(&original, &typed).is_empty());
    }

    #[test]
    fn fix_211_an_explicit_null_for_a_known_skip_serializing_if_field_is_not_unknown() {
        // `source` is a field the struct DOES know; sent as `null`, it
        // round-trips to nothing at all (skip_serializing_if) — the exact
        // shape the field-keeping rule in `manifest::client_knows` already
        // treats as "known, nothing declared", never as foreign.
        let original: Value = serde_json::from_str(r#"{"apps":[{"a":1,"source":null}]}"#).unwrap();
        let typed: Outer = serde_json::from_value(original.clone()).unwrap();
        assert!(unknown_fields(&original, &typed).is_empty());
    }

    #[test]
    fn fix_211_an_empty_foreign_value_is_the_documented_blind_spot() {
        // Not a goal to close: an unknown field sent with an empty value
        // is indistinguishable from a known one skipped for being empty.
        let original: Value =
            serde_json::from_str(r#"{"apps":[{"a":1}],"future_flag":false}"#).unwrap();
        let typed: Outer = serde_json::from_value(original.clone()).unwrap();
        assert!(unknown_fields(&original, &typed).is_empty());
    }
}
