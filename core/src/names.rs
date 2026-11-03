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

/// Whether a file name (last path component, no directories) is a secret env
/// file that is never shown, diffed or committed: `.env`, `.env.*`, `*.env`
/// and `*.env.*` (`app.env.local`, `prod.env`, `.env.production`). A leading
/// `.env` prefix stays secret too (`.envrc`), as the old per-crate copies
/// treated it. `environment.md`, `envoy.conf` and `service.yml` are not.
pub fn is_secret_env_name(name: &str) -> bool {
    name.starts_with(".env") || name.ends_with(".env") || name.contains(".env.")
}

/// [`is_secret_env_name`] for a path: judges the last component.
pub fn is_secret_env_path(path: &str) -> bool {
    is_secret_env_name(path.rsplit('/').next().unwrap_or(path))
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

#[cfg(test)]
mod secret_env_tests {
    use super::{is_secret_env_name, is_secret_env_path};

    #[test]
    fn names_table() {
        let cases: [(&str, bool); 20] = [
            (".env", true),
            (".env.local", true),
            (".env.production", true),
            ("app.env", true),
            ("prod.env", true),
            ("app.env.local", true),
            ("traefik.env.bak", true),
            (".envrc", true),
            ("service.yml", false),
            ("environment.md", false),
            ("envoy.conf", false),
            ("env", false),
            ("env.md", false),
            ("docker-compose.yml", false),
            ("myenv", false),
            ("env-notes.md", false),
            ("", false),
            ("app.envx", false),
            ("a.environment", false),
            ("checks.yml", false),
        ];
        for (name, want) in cases {
            assert_eq!(is_secret_env_name(name), want, "{name:?}");
        }
    }

    #[test]
    fn paths_judge_the_last_component() {
        assert!(is_secret_env_path("stacks/media/app.env.local"));
        assert!(is_secret_env_path(".env"));
        assert!(!is_secret_env_path("stacks/.env/readme.md"));
        assert!(!is_secret_env_path("admin/env-notes.md"));
    }
}
