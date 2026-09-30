//! feat-ops-4: the dashboard's server asks Loki; the browser never does
//! (arch-config: the address is a setting, CT 120's firewall admits it).

use std::time::Duration;

use crate::core::config::AdminConfig;
use crate::core::logs::{self, LogLine, LogQuery};

#[derive(Clone)]
pub struct Loki {
    base: String,
    http: reqwest::Client,
    /// Decision "23 constants": `admin.logs_max_since_s` /
    /// `admin.logs_max_limit` (`logs::MAX_SINCE_S` / `logs::MAX_LIMIT` by
    /// default).
    max_since_s: u64,
    max_limit: usize,
}

/// replace-goaccess: a metric query over a window, as chart series.
impl Loki {
    pub async fn metric_range(
        &self,
        query: &str,
        start: u64,
        end: u64,
        step: u64,
        legend: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let v = self
            .get(
                "query_range",
                &[
                    ("query", query.to_string()),
                    ("start", start.to_string()),
                    ("end", end.to_string()),
                    ("step", format!("{}s", step)),
                ],
            )
            .await?;
        Ok(crate::shell::prometheus::series(&v, legend))
    }

    /// An instant metric query: `[(label value, number)]`, largest first.
    pub async fn metric_now(
        &self,
        query: &str,
        at: u64,
        label: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        let v = self
            .get(
                "query",
                &[("query", query.to_string()), ("time", at.to_string())],
            )
            .await?;
        let mut out: Vec<(String, f64)> = v["data"]["result"]
            .as_array()
            .map(|rs| {
                rs.iter()
                    .filter_map(|r| {
                        let k = r["metric"][label].as_str().unwrap_or("").to_string();
                        let n: f64 = r["value"][1].as_str()?.parse().ok()?;
                        Some((k, n))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(out)
    }

    async fn get(&self, path: &str, q: &[(&str, String)]) -> Result<serde_json::Value, String> {
        let url = format!("{}/loki/api/v1/{}", self.base, path);
        let res = self
            .http
            .get(&url)
            .query(q)
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
                "Loki answered HTTP {}: {}",
                status.as_u16(),
                body.chars().take(200).collect::<String>()
            ));
        }
        serde_json::from_str(&body).map_err(|e| format!("Loki's answer did not read: {e}"))
    }
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
        Some(Loki {
            base,
            http,
            max_since_s: c.logs_max_since_s,
            max_limit: c.logs_max_limit,
        })
    }

    /// Where it asks, for error messages.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// Ask Loki for the lines `q` names, newest first.
    pub async fn query(&self, q: &LogQuery, now: u64) -> Result<Found, String> {
        let logql = logs::logql(q)?;
        let (from, to, limit) = logs::window(q, now, self.max_since_s, self.max_limit);
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
