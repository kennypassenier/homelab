//! F3 notification building + repeat-damping (hardening H13). The payload
//! is built here so its shape is golden-tested — the HA automation parses
//! these fields, and a silent rename would break the events log.

use std::collections::HashMap;

/// One operation event, exactly as POSTed to the webhook.
pub fn op_payload(op: &str, label: &str, ok: bool, error: Option<&str>) -> String {
    serde_json::json!({
        "source": "homelab-host",
        "op": op,
        "label": label,
        "ok": ok,
        "error": error,
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
            op_payload("deploy-synctest", "deploy", true, None),
            r#"{"error":null,"label":"deploy","ok":true,"op":"deploy-synctest","source":"homelab-host"}"#
        );
        assert_eq!(
            op_payload(
                "backup-media",
                "scheduled-backup",
                false,
                Some("rclone: timeout")
            ),
            r#"{"error":"rclone: timeout","label":"scheduled-backup","ok":false,"op":"backup-media","source":"homelab-host"}"#
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
}
