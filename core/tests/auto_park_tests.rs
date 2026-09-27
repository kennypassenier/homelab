//! gap-22: the automatic-park notice says what the automatic park does.

use homelab_core::ops::enable::AUTO_PARK_NOTICE;

#[test]
fn gap_22_the_auto_park_notice_does_not_claim_onboot_was_cleared() {
    assert!(
        !AUTO_PARK_NOTICE.contains("no onboot"),
        "{AUTO_PARK_NOTICE}"
    );
    assert!(AUTO_PARK_NOTICE.contains("onboot and the running containers are left"));
    // And the host sends this text, not a copy of its own.
    let host = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../host/src/main.rs"),
    )
    .unwrap();
    assert!(!host.contains("no onboot until re-enabled"));
    assert!(host.matches("ops::enable::AUTO_PARK_NOTICE").count() >= 2);
}
