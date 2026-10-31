//! Tests of restic's progress and summary lines.

use super::progress::restic_bytes;

#[test]
fn restic_status_lines_read_as_bytes_and_stay_out_of_the_output() {
    let backup = r#"{"message_type":"status","percent_done":0.4,"total_files":9,"files_done":3,"total_bytes":1000,"bytes_done":400}"#;
    let b = restic_bytes(backup).unwrap();
    assert_eq!((b.done, b.total, b.is_status), (400, Some(1000), true));
    let restore = r#"{"message_type":"status","seconds_elapsed":5,"percent_done":0.5,"total_files":2,"files_restored":1,"total_bytes":80,"bytes_restored":40}"#;
    let r = restic_bytes(restore).unwrap();
    assert_eq!((r.done, r.total), (40, Some(80)));
    // Still scanning: no total yet.
    let scanning = r#"{"message_type":"status","total_bytes":0,"bytes_done":0}"#;
    assert_eq!(restic_bytes(scanning).unwrap().total, None);
}

#[test]
fn restic_summary_is_bytes_but_is_kept_for_the_empty_check() {
    let line =
        r#"{"message_type":"summary","total_files_processed":2,"total_bytes_processed":512}"#;
    let b = restic_bytes(line).unwrap();
    assert_eq!((b.done, b.total, b.is_status), (512, Some(512), false));
    assert!(restic_bytes("not json").is_none());
    assert!(restic_bytes(r#"{"message_type":"error"}"#).is_none());
}
