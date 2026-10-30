//! Reading `host.toml` and the checks that stop the daemon at start.

use super::*;

/// A missing file is legal; one that does not parse stops the daemon (F186).
pub(crate) fn read_file_config(path: &str) -> FileConfig {
    match std::fs::read_to_string(path) {
        Err(_) => FileConfig::default(),
        Ok(raw) => {
            let parsed: toml::Table = match toml::from_str(&raw) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("FATAL: {} does not parse as TOML: {}", path, e);
                    std::process::exit(1);
                }
            };
            for key in unknown_keys(&parsed) {
                eprintln!(
                    "WARNING: {}: '{}' is not a setting this daemon reads and is being \
                     ignored. A key written after a [table] header belongs to that table \
                     - move it above the first one.",
                    path, key
                );
            }
            match toml::Table::try_into::<FileConfig>(parsed) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("FATAL: {} is not a valid host config: {}", path, e);
                    std::process::exit(1);
                }
            }
        }
    }
}

/// The values every other setting depends on, checked once at start: the
/// token, the status interval, the log level and the token list. Any fault
/// is said out loud and stops the daemon, since a half-working host is worse
/// than one that does not start. Returns the token, the log level and the
/// token list.
pub(crate) fn start_checks(file: &FileConfig, path: &str) -> (String, String, Vec<TokenEntry>) {
    let token = std::env::var("HOMELAB_TOKEN")
        .ok()
        .or(file.token.clone())
        .unwrap_or_default();
    if token.len() < 16 {
        eprintln!(
            "FATAL: token must be set (>=16 chars) via {} or HOMELAB_TOKEN",
            path
        );
        std::process::exit(1);
    }
    if file.status_interval_s.is_some_and(|s| s < 10) {
        eprintln!(
            "FATAL: {}: status_interval_s must be at least 10 (each reading runs one probe per container)",
            path
        );
        std::process::exit(1);
    }
    // fix-122: the runtime debug toggle, checked the same way at start as it
    // is at `SetHostConfig` (`startup_problems`) — a directive string
    // `EnvFilter` cannot parse would otherwise be written to host.toml and
    // only fail the NEXT restart, silently, with the daemon logging nothing
    // at all.
    let log_level = file
        .log_level
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "info".to_string());
    if !log_level_is_valid(&log_level) {
        eprintln!(
            "FATAL: {}: log_level {:?} is not a tracing/EnvFilter directive (e.g. \"info\", \"debug\")",
            path, log_level
        );
        std::process::exit(1);
    }
    let tokens = file.tokens.clone().unwrap_or_default();
    if let Err(e) = validate_tokens(&tokens) {
        eprintln!("FATAL: {}: {}", path, e);
        std::process::exit(1);
    }
    (token, log_level, tokens)
}
