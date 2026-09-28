//! feat-ops-4: the dashboard's server asks Loki; the browser never does
//! (arch-config: the address is a setting, CT 120's firewall admits it).

use std::time::Duration;

use crate::core::config::AdminConfig;
use crate::core::logs::{self, LogLine, LogQuery};

#[derive(Clone)]
pub struct Loki {
    base: String,
    http: reqwest::Client,
}

/// What one query answered.
pub struct Found {
    pub logql: String,
    pub from: u64,
    pub to: u64,
    pub lines: Vec<LogLine>,
}

impl Loki {
    /// A client for the configured Loki, or None when none is set.
    pub fn from_config(c: Option<&AdminConfig>) -> Option<Self> {
        let c = c?;
        let base = c.loki_base()?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(c.loki_timeout_s))
            .build()
            .ok()?;
        Some(Loki { base, http })
    }

    /// Where it asks, for error messages.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Ask Loki for the lines `q` names, newest first.
    pub async fn query(&self, q: &LogQuery, now: u64) -> Result<Found, String> {
        let logql = logs::logql(q)?;
        let (from, to, limit) = logs::window(q, now);
        let ns = |s: u64| (s as u128 * 1_000_000_000).to_string();
        let url = format!("{}/loki/api/v1/query_range", self.base);
        let res = self
            .http
            .get(&url)
            .query(&[
                ("query", logql.as_str()),
                ("start", ns(from).as_str()),
                ("end", ns(to).as_str()),
                ("limit", limit.to_string().as_str()),
                ("direction", "backward"),
            ])
            .send()
            .await
            .map_err(|e| format!("Loki at {} did not answer: {e}", self.base))?;
        let status = res.status();
        let body = res
            .text()
            .await
            .map_err(|e| format!("Loki's answer broke off: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "Loki at {} answered HTTP {}: {}",
                self.base,
                status.as_u16(),
                body.chars().take(200).collect::<String>()
            ));
        }
        let lines = logs::parse_answer(&body, limit)?;
        Ok(Found {
            logql,
            from,
            to,
            lines,
        })
    }
}
