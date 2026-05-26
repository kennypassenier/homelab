//! Idempotent SSH alias management — Rust equivalent of scripts/client/add-ssh.sh.
//!
//! Parses `~/.ssh/config` into `SshEntry` values and provides an upsert
//! operation that adds or replaces a single Host block without touching the
//! rest of the file. The write is atomic (tmp → rename).

use anyhow::{Context, Result};
use std::{fs, io::Write, path::PathBuf};

/// One Host block from `~/.ssh/config`.
#[derive(Debug, Clone)]
pub struct SshEntry {
    pub host: String,
    pub hostname: String,
    pub user: String,
    pub port: u16,
}

impl SshEntry {
    /// Serialises the entry as a `~/.ssh/config` Host block.
    pub fn to_config_block(&self) -> String {
        format!(
            "Host {}\n    HostName {}\n    User {}\n    Port {}\n",
            self.host, self.hostname, self.user, self.port,
        )
    }
}

/// Returns the canonical path to `~/.ssh/config` for the running user.
pub fn ssh_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    PathBuf::from(home).join(".ssh/config")
}

/// Reads and parses `~/.ssh/config`, returning all complete Host blocks.
///
/// Entries without a `HostName` directive are skipped (e.g. the `Host *`
/// wildcard block). Unrecognised directives are silently ignored.
pub fn parse_ssh_config() -> Vec<SshEntry> {
    let path = ssh_config_path();
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut entries: Vec<SshEntry> = Vec::new();
    let mut cur_host: Option<String> = None;
    let mut cur_hostname = String::new();
    let mut cur_user = String::from("root");
    let mut cur_port: u16 = 22;

    let mut flush = |entries: &mut Vec<SshEntry>,
                     host: Option<String>,
                     hostname: &str,
                     user: &str,
                     port: u16| {
        if let Some(h) = host {
            if !hostname.is_empty() && h != "*" {
                entries.push(SshEntry {
                    host: h,
                    hostname: hostname.to_string(),
                    user: user.to_string(),
                    port,
                });
            }
        }
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.splitn(2, char::is_whitespace);
        let key = parts.next().unwrap_or("").to_lowercase();
        let value = parts.next().unwrap_or("").trim().to_string();

        match key.as_str() {
            "host" => {
                flush(&mut entries, cur_host.take(), &cur_hostname, &cur_user, cur_port);
                cur_host = Some(value);
                cur_hostname.clear();
                cur_user = String::from("root");
                cur_port = 22;
            }
            "hostname" => cur_hostname = value,
            "user" => cur_user = value,
            "port" => cur_port = value.parse().unwrap_or(22),
            _ => {}
        }
    }
    flush(&mut entries, cur_host.take(), &cur_hostname, &cur_user, cur_port);
    entries
}

/// Idempotently adds or replaces a single Host block in `~/.ssh/config`.
///
/// - Matching is case-insensitive on the Host name.
/// - If an existing block is found it is replaced in-place; all other content
///   is preserved verbatim.
/// - Otherwise the new block is appended at the end.
/// - The write is atomic: new content → `.tmp` → rename.
///
/// `~/.ssh/config` and its directory are created with secure permissions
/// (`0o600` / `0o700`) if they do not exist.
pub fn upsert_ssh_entry(entry: &SshEntry) -> Result<()> {
    let path = ssh_config_path();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).context("creating ~/.ssh")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    if !path.exists() {
        fs::write(&path, "").context("creating ~/.ssh/config")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
    }

    let existing = fs::read_to_string(&path).unwrap_or_default();
    let new_block = entry.to_config_block();
    let host_lower = entry.host.to_lowercase();

    let mut output = String::with_capacity(existing.len() + new_block.len() + 2);
    let mut inside_replaced_block = false;
    let mut replaced = false;

    for line in existing.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("host ") {
            if trimmed.to_lowercase() == format!("host {}", host_lower) {
                output.push_str(&new_block);
                output.push('\n');
                inside_replaced_block = true;
                replaced = true;
                continue;
            } else if inside_replaced_block {
                inside_replaced_block = false;
            }
        } else if inside_replaced_block {
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }

    if !replaced {
        if !output.ends_with("\n\n") {
            if !output.ends_with('\n') && !output.is_empty() {
                output.push('\n');
            }
            output.push('\n');
        }
        output.push_str(&new_block);
    }

    let tmp_path = path.with_extension("tmp");
    {
        let mut f = fs::File::create(&tmp_path).context("writing ~/.ssh/config.tmp")?;
        f.write_all(output.as_bytes())?;
        f.flush()?;
    }
    fs::rename(&tmp_path, &path).context("renaming config.tmp")?;
    Ok(())
}
