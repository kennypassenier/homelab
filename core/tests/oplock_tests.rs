//! long-silences (expert panel, 2026-09-27): a command typed while another
//! operation holds the host's single operation lock waited with no word —
//! during the nightly round, for the whole backup batch.

use homelab_core::oplock::{Holder, waiting_message};

const NOW: u64 = 1_790_000_000;

/// The waiting client is told at once what it waits for, since when, and
/// how far along the batch is.
/// covers: fix-104
#[test]
fn fix_104_a_command_behind_the_nightly_backup_is_told_so_at_once() {
    let h = Holder {
        what: "the nightly backup".into(),
        started_unix: NOW - 23 * 60,
        done: 4,
        total: 14,
        stack: None,
    };
    assert_eq!(
        waiting_message(Some(&h), NOW),
        "waiting for the nightly backup (started 23 min ago, stack 5 of 14); \
         this command runs as soon as it is done"
    );
    let deploy = Holder {
        what: "deploy".into(),
        started_unix: NOW - 75 * 60 - 5,
        done: 0,
        total: 0,
        stack: Some("media".into()),
    };
    assert_eq!(
        waiting_message(Some(&deploy), NOW),
        "waiting for deploy (started 1 h 15 min ago); this command runs as soon as it is done"
    );
    let fresh = Holder {
        what: "backup".into(),
        started_unix: NOW - 40,
        done: 0,
        total: 0,
        stack: None,
    };
    assert!(waiting_message(Some(&fresh), NOW).contains("started 40 s ago"));
    assert_eq!(
        waiting_message(None, NOW),
        "waiting for another operation to finish; this command runs as soon as it is done"
    );
}
