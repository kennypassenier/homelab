//! fix-209: a boolean query flag as the pages actually send it.
//!
//! The pages write `?fresh=1` and `?refresh=1`; serde's plain `bool` only
//! takes `true`/`false`, so axum answered 400 and "Compare with the files"
//! never ran (Kenny, 2026-10-02: "Failed to deserialize query string:
//! fresh: provided string was not `true` or `false`"). Every boolean query
//! field goes through [`flag`], which takes both spellings.

use serde::{Deserialize, Deserializer, de::Error};

/// `1`/`true`/`yes`/`on` and an empty value (`?fresh`) are true;
/// `0`/`false`/`no`/`off` are false; anything else is refused by name.
pub fn flag<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let s = String::deserialize(d)?;
    match s.to_ascii_lowercase().as_str() {
        "" | "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        other => Err(D::Error::custom(format!(
            "expected 1/0 or true/false, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use axum::extract::Query;
    use axum::http::Uri;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Q {
        #[serde(default, deserialize_with = "super::flag")]
        fresh: bool,
    }

    fn read(uri: &str) -> Result<bool, String> {
        let uri: Uri = uri.parse().unwrap();
        Query::<Q>::try_from_uri(&uri)
            .map(|q| q.0.fresh)
            .map_err(|e| e.to_string())
    }

    #[test]
    fn fix_209_the_pages_own_spelling_of_a_flag_is_read() {
        assert_eq!(read("/data/drift?fresh=1"), Ok(true));
        assert_eq!(read("/data/drift?fresh=true"), Ok(true));
        assert_eq!(read("/data/drift?fresh=0"), Ok(false));
        assert_eq!(read("/data/drift"), Ok(false));
        assert!(read("/data/drift?fresh=maybe").is_err());
    }
}
