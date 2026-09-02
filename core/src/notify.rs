//! F3 notification building + repeat-damping (hardening H13). The payload
//! is built here so its shape is golden-tested — the HA automation parses
//! these fields, and a silent rename would break the events log.

use std::collections::HashMap;

/// One event, exactly as POSTed to the webhook — and the ONLY place a
/// payload is built.
///
/// F86: the nightly fleet check used to hand-build its own JSON with no
/// `source` and no `label`, which made the most important report of the day
/// the one event a filter on `source` would silently drop. The boot notice
/// hand-built a third variant. Three shapes for one contract is how a
/// consumer ends up parsing the two it happens to have seen.
pub fn op_payload(op: &str, label: &str, ok: bool, error: Option<&str>, version: &str) -> String {
    serde_json::json!({
        "source": "homelab-host",
        "op": op,
        "label": label,
        "ok": ok,
        "error": error,
        "version": version,
    })
    .to_string()
}

/// Failure-repeat damping: an identical failing event inside the window is
/// suppressed (a stack that fails every night must not page every night);
/// successes are never suppressed, and a CHANGED error text passes through
/// (it is new information).
pub struct NotifyDamper {
    window_s: u64,
    last_sent: HashMap<String, u64>,
}

impl NotifyDamper {
    pub fn new(window_s: u64) -> Self {
        Self {
            window_s,
            last_sent: HashMap::new(),
        }
    }

    /// Decide-and-record. `now` injected — core reads no clocks.
    pub fn should_send(&mut self, op: &str, ok: bool, error: Option<&str>, now: u64) -> bool {
        if ok {
            return true;
        }
        let key = format!("{}|{}", op, error.unwrap_or(""));
        match self.last_sent.get(&key) {
            Some(&t) if now.saturating_sub(t) < self.window_s => false,
            _ => {
                self.last_sent.insert(key, now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_payload_shape() {
        // The HA automation reads exactly these fields — frozen here.
        assert_eq!(
            op_payload("deploy-synctest", "deploy", true, None, "3.35.0"),
            r#"{"error":null,"label":"deploy","ok":true,"op":"deploy-synctest","source":"homelab-host","version":"3.35.0"}"#
        );
        assert_eq!(
            op_payload(
                "backup-media",
                "scheduled-backup",
                false,
                Some("rclone: timeout"),
                "3.35.0"
            ),
            r#"{"error":"rclone: timeout","label":"scheduled-backup","ok":false,"op":"backup-media","source":"homelab-host","version":"3.35.0"}"#
        );
    }

    #[test]
    fn damper_suppresses_repeats_within_window_only() {
        let mut d = NotifyDamper::new(20 * 3600);
        let t0 = 1_800_000_000;
        assert!(d.should_send("backup-media", false, Some("timeout"), t0));
        // Same failure an hour later: suppressed.
        assert!(!d.should_send("backup-media", false, Some("timeout"), t0 + 3600));
        // Different error text: new information, passes.
        assert!(d.should_send("backup-media", false, Some("repo locked"), t0 + 3600));
        // After the window: resends (still broken → remind once a day).
        assert!(d.should_send("backup-media", false, Some("timeout"), t0 + 21 * 3600));
        // Successes are never suppressed.
        assert!(d.should_send("backup-media", true, None, t0));
        assert!(d.should_send("backup-media", true, None, t0 + 1));
    }

    /// F86: the nightly fleet check is the most important report the system
    /// sends, and it was the only one that did not go through this function
    /// — no `source`, no `label`. A consumer filtering on `source` would
    /// have dropped exactly that one. Frozen here so a third shape cannot
    /// quietly appear again.
    #[test]
    fn the_fleet_check_uses_the_same_shape_as_every_other_event() {
        let p = op_payload(
            "fleet-check",
            "nightly",
            false,
            Some("4 finding(s)"),
            "3.35.0",
        );
        let v: serde_json::Value = serde_json::from_str(&p).unwrap();
        for field in ["source", "op", "label", "ok", "error", "version"] {
            assert!(
                v.get(field).is_some(),
                "fleet-check payload lost '{}'",
                field
            );
        }
        assert_eq!(v["source"], "homelab-host");
        assert_eq!(v["label"], "nightly");
    }
}
