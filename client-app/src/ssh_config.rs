//! SSH config management for HostManagement tab.
//! Provides idempotent add/update of SSH aliases for LXC containers.

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

/// Represents an SSH alias entry.
pub struct SshAlias<'a> {
    pub alias: &'a str,
    pub hostname: &'a str,
    pub user: Option<&'a str>,
    pub port: Option<u16>,
}

/// Returns the path to ~/.ssh/config
fn ssh_config_path() -> PathBuf {
    let home = std::env::var("HOME").expect("HOME not set");
    PathBuf::from(home).join(".ssh/config")
}

/// Adds or updates an SSH alias in ~/.ssh/config (idempotent, safe, POSIX only)
#[cfg(unix)]
pub fn add_or_update_ssh_alias(
    alias: &str,
    hostname: &str,
    user: Option<&str>,
    port: Option<u16>,
) -> io::Result<()> {
    let path = ssh_config_path();
    let mut config = fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<&str> = config.lines().collect();
    let mut found = false;
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("Host ") && lines[i].contains(alias) {
            found = true;
            // Overwrite the next lines with new config
            let mut j = i + 1;
            while j < lines.len() && !lines[j].trim_start().starts_with("Host ") {
                j += 1;
            }
            lines.splice(
                i..j,
                vec![
                    &format!("Host {}", alias),
                    &format!("    HostName {}", hostname),
                    &format!("    User {}", user.unwrap_or("root")),
                    &format!("    Port {}", port.unwrap_or(22)),
                ],
            );
            break;
        }
        i += 1;
    }
    if !found {
        // Append new entry
        lines.push("");
        lines.push(&format!("Host {}", alias));
        lines.push(&format!("    HostName {}", hostname));
        lines.push(&format!("    User {}", user.unwrap_or("root")));
        lines.push(&format!("    Port {}", port.unwrap_or(22)));
    }
    let new_config = lines.join("\n");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&path)?;
    file.write_all(new_config.as_bytes())?;
    Ok(())
}

/// On Windows, this function is a no-op and returns an error.
#[cfg(windows)]
pub fn add_or_update_ssh_alias(
    _alias: &str,
    _hostname: &str,
    _user: Option<&str>,
    _port: Option<u16>,
) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Other,
        "SSH config management is not supported on Windows.",
    ))
}
