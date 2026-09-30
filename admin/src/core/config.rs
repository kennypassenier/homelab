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
    /// arch-exposure, lock 1: the Cloudflare Access team (its key set lives
    /// at https://<team>/cdn-cgi/access/certs) and the application's
    /// audience tag. Neither is a secret.
    pub access_team_domain: String,
    pub access_aud: String,
    /// Clock skew a token's times may have. Default 60 s.
    #[serde(default = "d_leeway")]
    pub access_leeway_s: u64,
    /// How often the team's keys are fetched again. Default 3600 s; an
    /// unknown key id also fetches at once, at most once a minute.
    #[serde(default = "d_certs_refresh")]
    pub access_certs_refresh_s: u64,
    /// For a developer's machine only: run without the two locks (no
    /// Cloudflare in front of a loopback address). Logged as a warning at
    /// every start; stacks/admin's config never sets it, and a test says so.
    #[serde(default)]
    pub dev_without_locks: bool,
    /// feat-ops-4: Loki's HTTP API, e.g. `http://10.10.10.13:3100`. Only the
    /// dashboard's server asks it, never the browser. Unset: the logs tab
    /// says that no Loki is configured.
    #[serde(default)]
    pub loki_url: Option<String>,
    /// replace-grafana (2026-09-30): Prometheus's HTTP API, e.g.
    /// `http://10.10.10.13:9090`, asked by the dashboard's server for the
    /// charts. Unset: the charts say that no Prometheus is configured.
    #[serde(default)]
    pub prometheus_url: Option<String>,
    /// The `host` label the hypervisor's node_exporter carries in
    /// Prometheus (the metrics stack's prometheus.yml sets it). Unset: no
    /// host charts.
    #[serde(default)]
    pub charts_host: Option<String>,
    /// replace-goaccess (2026-09-30): the Loki `job` of the proxy's access
    /// log, one JSON object per request (a stack's `log_files:` declares
    /// it). Unset: no Traffic page.
    #[serde(default)]
    pub traffic_job: Option<String>,
    /// Seconds one Loki query may take. Default 15.
    #[serde(default = "d_loki_timeout")]
    pub loki_timeout_s: u64,
    /// feat-ops-2: how long the host waits for an answer to a question (its
    /// own `ask_timeout_s`, default 120). Past it the dashboard refuses to
    /// send an answer, because the host has stopped waiting.
    #[serde(default = "d_ask_timeout")]
    pub ask_timeout_s: u64,
    /// Decision "23 constants" (2026-09-30): the longest log window one
    /// request may ask for, in seconds. Default 604800 (7 d),
    /// `HOMELAB_ADMIN_LOGS_MAX_SINCE_S`.
    #[serde(default = "d_logs_max_since")]
    pub logs_max_since_s: u64,
    /// The most log lines one request may ask for. Default 5000,
    /// `HOMELAB_ADMIN_LOGS_MAX_LIMIT`.
    #[serde(default = "d_logs_max_limit")]
    pub logs_max_limit: usize,
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
fn d_leeway() -> u64 {
    60
}
fn d_certs_refresh() -> u64 {
    3600
}
fn d_loki_timeout() -> u64 {
    15
}
fn d_ask_timeout() -> u64 {
    120
}
fn d_logs_max_since() -> u64 {
    crate::core::logs::MAX_SINCE_S
}
fn d_logs_max_limit() -> usize {
    crate::core::logs::MAX_LIMIT
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
        if let Some(url) = &self.loki_url {
            self.loki_url = Some(expand(url, lookup)?);
        }
        if let Some(url) = &self.prometheus_url {
            self.prometheus_url = Some(expand(url, lookup)?);
        }
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
        if !self.access_team_domain.ends_with(".cloudflareaccess.com") {
            why.push(format!(
                "admin.access_team_domain {:?} must be <team>.cloudflareaccess.com",
                self.access_team_domain
            ));
        }
        if self.access_aud.len() != 64 || !self.access_aud.bytes().all(|b| b.is_ascii_hexdigit()) {
            why.push(
                "admin.access_aud must be the Access application's 64-character audience tag"
                    .into(),
            );
        }
        if self.access_certs_refresh_s < 60 {
            why.push("admin.access_certs_refresh_s must be at least 60".into());
        }
        if let Some(url) = &self.loki_url {
            let scheme_ok = url.starts_with("http://") || url.starts_with("https://");
            if !scheme_ok || url.contains('?') || url.contains('#') {
                why.push(format!(
                    "admin.loki_url {:?} must be the base address, e.g. http://10.10.10.13:3100",
                    url
                ));
            }
        }
        if let Some(url) = &self.prometheus_url {
            let scheme_ok = url.starts_with("http://") || url.starts_with("https://");
            if !scheme_ok || url.contains('?') || url.contains('#') {
                why.push(format!(
                    "admin.prometheus_url {:?} must be the base address, e.g. http://prometheus:9090",
                    url
                ));
            }
        }
        if self.loki_timeout_s == 0 {
            why.push("admin.loki_timeout_s must be at least 1".into());
        }
        if self.ask_timeout_s == 0 {
            why.push("admin.ask_timeout_s must be at least 1".into());
        }
        if self.logs_max_since_s == 0 {
            why.push("admin.logs_max_since_s must be at least 1".into());
        }
        if self.logs_max_limit == 0 {
            why.push("admin.logs_max_limit must be at least 1".into());
        }
        if why.is_empty() {
            Ok(self)
        } else {
            Err(why.join("; "))
        }
    }

    /// Loki's base address without a trailing slash, when one is set.
    pub fn loki_base(&self) -> Option<String> {
        self.loki_url
            .as_ref()
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
    }

    /// Prometheus's base address without a trailing slash, when one is set.
    pub fn prometheus_base(&self) -> Option<String> {
        self.prometheus_url
            .as_ref()
            .map(|u| u.trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
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

/// The settings from the environment, `HOMELAB_ADMIN_<KEY>` per key (CT 120:
/// the unit's `Environment=` lines for the plain values, admin.env for the
/// secrets). `dev_without_locks` is file-only on purpose: no environment
/// variable can switch the locks off.
pub fn from_env(lookup: &dyn Fn(&str) -> Option<String>) -> Result<AdminConfig, String> {
    const NUMBERS: &[&str] = &[
        "poll_s",
        "sse_buffer",
        "backoff_min_s",
        "backoff_max_s",
        "access_leeway_s",
        "access_certs_refresh_s",
        "loki_timeout_s",
        "ask_timeout_s",
        "logs_max_since_s",
        "logs_max_limit",
    ];
    const STRINGS: &[&str] = &[
        "host",
        "host_token",
        "access_team_domain",
        "access_aud",
        "loki_url",
        "prometheus_url",
        "charts_host",
        "traffic_job",
    ];
    let mut t = toml::Table::new();
    for key in STRINGS {
        if let Some(v) = lookup(&format!("HOMELAB_ADMIN_{}", key.to_ascii_uppercase())) {
            t.insert((*key).into(), toml::Value::String(v));
        }
    }
    for key in NUMBERS {
        if let Some(v) = lookup(&format!("HOMELAB_ADMIN_{}", key.to_ascii_uppercase())) {
            let n: i64 = v.trim().parse().map_err(|_| {
                format!(
                    "HOMELAB_ADMIN_{} = {:?} is not a whole number",
                    key.to_ascii_uppercase(),
                    v
                )
            })?;
            t.insert((*key).into(), toml::Value::Integer(n));
        }
    }
    let admin: AdminConfig = toml::Value::Table(t)
        .try_into()
        .map_err(|e| format!("the HOMELAB_ADMIN_* settings do not read: {}", e))?;
    admin.validate()
}

/// From the TOML table chassis returns for the project.
pub fn from_table(table: toml::Table) -> Result<AdminConfig, String> {
    let file: File = toml::Value::Table(table)
        .try_into()
        .map_err(|e| format!("the [admin] settings do not read: {}", e))?;
    file.admin.validate()
}
