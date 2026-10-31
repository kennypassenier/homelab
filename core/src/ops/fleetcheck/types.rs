//! The two types every check speaks: how bad a difference is, and the
//! finding that names it.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Something is not doing its job right now.
    Broken,
    /// Working, but drifting — it will bite on the next deploy or outage.
    Drift,
    /// Not a problem: something deliberately arranged, printed so it stays
    /// visible (Kenny, form Z3, 2026-09-02).
    ///
    /// The case it exists for: a stack whose data is declared reproducible
    /// has no backup, and a check that only knows Broken and Drift would
    /// either shout "never been backed up" at a decision, or say nothing at
    /// all — and silence makes a deliberate gap indistinguishable from a
    /// forgotten one. Neither is the truth; this is.
    Noted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    /// Which stack, container or file this is about.
    pub subject: String,
    /// What is wrong, in one sentence.
    pub what: String,
    /// What to do about it. Every finding carries one (standing rule 11).
    pub remedy: String,
}
