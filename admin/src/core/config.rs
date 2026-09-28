//! arch-config: the dashboard's own settings, the `[admin]` table of the
//! chassis config file (`<state_dir>/config.toml`,
//! `/appdata/admin/admin-config/config.toml` in CT 120). Secrets are written
//! as `${VAR}` and come from the environment file latch fills; chassis
//! expands them and refuses an unset one.
//!
//! ```toml
//! [admin]
//! host = "10.10.10.250:8443"
//! host_token = "${HOMELAB_ADMIN_HOST_TOKEN}"
//! poll_s = 10
//! ```

use std::time::Duration;

use serde::Deserialize;

/// The file's project half, as chassis hands it over.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub admin: AdminConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdminConfig {
    /// `host:port` of homelab-host.
    pub host: String,
    /// The token this dashboard presents to the host (scope `all`).
    pub host_token: String,
    /// Seconds between two fleet reads. Default 10.
    #[serde(default = "d_poll")]
    pub poll_s: u64,
    /// Events a browser may fall behind before it is told to resync.
    #[serde(default = "d_sse_buffer")]
    pub sse_buffer: usize,
    /// Reconnect backoff to the host: first wait and ceiling, seconds.
    #[serde(default = "d_backoff_min")]
    pub backoff_min_s: u64,
    #[serde(default = "d_backoff_max")]
    pub backoff_max_s: u64,
}

fn d_poll() -> u64 {
    10
}
fn d_sse_buffer() -> usize {
    256
}
fn d_backoff_min() -> u64 {
    1
}
fn d_backoff_max() -> u64 {
    60
}

/// `${NAME}` in a value, replaced from `lookup`; an unset name is an error,
/// never an empty string (the chassis rule for its own knobs, which it does
/// not apply to a project's table).
pub fn expand(value: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find("${") {
        out.push_str(&rest[..at]);
        let tail = &rest[at + 2..];
        let end = tail
            .find('}')
            .ok_or_else(|| format!("unclosed ${{ in {:?}", value))?;
        let name = &tail[..end];
        let v =
            lookup(name).ok_or_else(|| format!("${{{}}} is not set in the environment", name))?;
        out.push_str(&v);
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl AdminConfig {
    /// Replace `${NAME}` in the string settings from `lookup`.
    pub fn expanded(mut self, lookup: &dyn Fn(&str) -> Option<String>) -> Result<Self, String> {
        self.host = expand(&self.host, lookup)?;
        self.host_token = expand(&self.host_token, lookup)?;
        Ok(self)
    }

    /// The settings, or every reason they cannot run, in one message.
    pub fn validate(self) -> Result<Self, String> {
        let mut why = Vec::new();
        if self
            .host
            .rsplit_once(':')
            .and_then(|(_, p)| p.parse::<u16>().ok())
            .is_none()
        {
            why.push(format!(
                "admin.host {:?} must be host:port, e.g. 10.10.10.250:8443",
                self.host
            ));
        }
        if self.host_token.len() < 16 {
            why.push(
                "admin.host_token must be at least 16 characters (the host refuses shorter ones)"
                    .into(),
            );
        }
        if self.poll_s == 0 {
            why.push("admin.poll_s must be at least 1".into());
        }
        if self.sse_buffer == 0 {
            why.push("admin.sse_buffer must be at least 1".into());
        }
        if self.backoff_min_s == 0 || self.backoff_max_s < self.backoff_min_s {
            why.push(
                "admin.backoff_min_s must be at least 1 and at most admin.backoff_max_s".into(),
            );
        }
        if why.is_empty() {
            Ok(self)
        } else {
            Err(why.join("; "))
        }
    }

    pub fn poll(&self) -> Duration {
        Duration::from_secs(self.poll_s)
    }
    pub fn backoff(&self) -> (Duration, Duration) {
        (
            Duration::from_secs(self.backoff_min_s),
            Duration::from_secs(self.backoff_max_s),
        )
    }
}

/// From the TOML table chassis returns for the project.
pub fn from_table(table: toml::Table) -> Result<AdminConfig, String> {
    let file: File = toml::Value::Table(table)
        .try_into()
        .map_err(|e| format!("the [admin] settings do not read: {}", e))?;
    file.admin.validate()
}
