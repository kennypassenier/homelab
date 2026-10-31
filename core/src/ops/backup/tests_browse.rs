//! Tests of reading `restic ls --json` output.

use super::browse::parse_snapshot_ls;

#[test]
fn empty_summary_is_empty_entries() {
    assert!(parse_snapshot_ls("").is_empty());
}

#[test]
fn parses_node_lines_and_skips_the_snapshot_summary_line() {
    let raw = "{\"struct_type\":\"snapshot\",\"id\":\"abc\"}\n\
                {\"struct_type\":\"node\",\"name\":\"a.txt\",\"path\":\"/a.txt\",\"type\":\"file\",\"size\":12}\n\
                {\"struct_type\":\"node\",\"name\":\"sub\",\"path\":\"/sub\",\"type\":\"dir\"}\n";
    let entries = parse_snapshot_ls(raw);
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[0].kind, "file");
    assert_eq!(entries[0].size, Some(12));
    assert_eq!(entries[1].kind, "dir");
    assert_eq!(entries[1].size, None);
}

#[test]
fn malformed_lines_are_skipped_not_fatal() {
    let raw =
        "not json\n{\"struct_type\":\"node\",\"name\":\"ok\",\"path\":\"/ok\",\"type\":\"file\"}\n";
    let entries = parse_snapshot_ls(raw);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "ok");
}
