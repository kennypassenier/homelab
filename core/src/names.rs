//! redesign-final (coordinator, 2026-10-04: the incident bundle named
//! "update kp-soft" that nothing could match or open is a fault class):
//! one normalising key for a name that rows are matched, keyed or linked
//! by — an operation, a stack, an app, a job, an incident, a repository,
//! a snapshot tag. The dashboard's `admin/web/js/namekey.js` is the same
//! function; `names_tests` and `finalreview2.test.js` drive the same names
//! through both.

/// Lower case, every run of characters other than `a`–`z`, `0`–`9`, `.`
/// and `_` one `-`, no `-` at either end.
pub fn name_key(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut dash = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' {
            if dash && !out.is_empty() {
                out.push('-');
            }
            dash = false;
            out.push(c);
        } else {
            dash = true;
        }
    }
    out
}

/// An incident bundle's directory name: `<unix seconds>-<op's key>`, so
/// every bundle the host writes is one the dashboard can open.
pub fn incident_name(ts_unix: u64, op: &str) -> String {
    format!("{ts_unix}-{}", name_key(op))
}

#[cfg(test)]
mod names_tests {
    use super::*;

    // The names the dashboard's own test drives through namekey.js.
    pub const CASES: &[(&str, &str)] = &[
        ("update kp-soft", "update-kp-soft"),
        ("update-kp-soft", "update-kp-soft"),
        ("Backup  Gateway", "backup-gateway"),
        (
            "device-backup-OPNsense Router",
            "device-backup-opnsense-router",
        ),
        ("wipe-kp-soft/jobtracker", "wipe-kp-soft-jobtracker"),
        ("app_v2.1", "app_v2.1"),
        (" -lead and trail- ", "lead-and-trail"),
        ("café über", "caf-ber"),
        ("", ""),
    ];

    #[test]
    fn redesign_final_one_name_key_for_every_name() {
        for (name, key) in CASES {
            assert_eq!(name_key(name), *key, "{name:?}");
            assert_eq!(name_key(key), *key, "a key is its own key: {key:?}");
        }
    }

    #[test]
    fn redesign_final_every_incident_name_the_host_writes_is_one_it_can_open() {
        for (name, _) in CASES.iter().filter(|(n, _)| !n.trim().is_empty()) {
            let n = incident_name(1_800_000_000, name);
            assert!(
                n.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
                "{n:?}"
            );
            assert!(!n.contains(".."), "{n:?}");
        }
    }
}
