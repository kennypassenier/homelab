//! fix-65 (nightly-report-always-red, 2026-09-27): `homelab checks answer`
//! took exactly one id, so the morning round of several open checks meant
//! one invocation per id. The pure id-list parsing lives here so it is
//! tested without a host; `main.rs` sends one `AnswerManualCheck` per id
//! with the same verdict, note and (for `accept`) days.

/// `<id>[,<id>,...]` → the trimmed, non-empty ids, in the order given.
/// Blank entries from a stray comma or trailing comma are dropped rather
/// than sent to the host as an id that cannot exist.
pub fn split_ids(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fix_65_one_id_is_still_just_one_id() {
        assert_eq!(split_ids("3f2a9c1e"), vec!["3f2a9c1e"]);
    }

    #[test]
    fn fix_65_several_ids_answer_in_one_command() {
        assert_eq!(
            split_ids("3f2a9c1e,9c1e3f2a,abc"),
            vec!["3f2a9c1e", "9c1e3f2a", "abc"]
        );
    }

    #[test]
    fn fix_65_whitespace_and_stray_commas_are_forgiven() {
        assert_eq!(split_ids(" a , b ,, ,c,"), vec!["a", "b", "c"]);
    }

    #[test]
    fn fix_65_empty_input_is_no_ids() {
        assert!(split_ids("").is_empty());
        assert!(split_ids(" , , ").is_empty());
    }
}
