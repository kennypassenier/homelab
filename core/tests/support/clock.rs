//! redesign-final (B, coordinator 2026-10-04): the Rust tests' one clock.
//! A test never reads the real clock; it judges dates against this fixed
//! day (scripts take it as `--today`), so a date it names never expires.
//! admin/web/scripts/check-test-clock.mjs refuses any other clock in a test.

/// The day every dated test is judged on.
pub const TODAY: &str = "2026-10-03";
