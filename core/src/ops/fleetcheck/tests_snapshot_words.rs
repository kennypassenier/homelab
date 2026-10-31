use super::backup_age::no_snapshot_words;

#[test]
fn inbox_dup_finding_1_an_empty_app_other_than_the_stack_is_named() {
    assert_eq!(
        no_snapshot_words("registry", &["keep".into()]),
        "has data to back up but no snapshot exists yet (app keep)"
    );
    assert_eq!(
        no_snapshot_words("media", &["media".into()]),
        "has data to back up but no snapshot exists yet"
    );
    assert_eq!(
        no_snapshot_words("media", &[]),
        "has data to back up but no snapshot exists yet"
    );
}
