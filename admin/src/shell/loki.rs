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

    /// An "instant" metric read, as [`Loki::metric_range`] with a one-second
    /// window ending at `at` (fix-221: the Loki gateway's one read endpoint
    /// — `stacks/metrics/loki-push/nginx.conf` — allows `query_range` only;
    /// `query` (the real instant endpoint) answers everything, including
    /// this dashboard, with a bare HTTP 403, since Kenny's decision "one
    /// read endpoint for the dashboard" never carved out a second door).
    /// This is exact, not an approximation: every query this function is
    /// given already carries its own lookback inside the query text itself
    /// (`count_over_time(...[{span}s])`), so `query_range` evaluating that
    /// expression at one instant (the end of a 1 s window) returns the same
    /// number `query` would have — `query_range` is a superset of `query`,
    /// a series of instants rather than one, and taking the LAST point
    /// (closest to `at`) reduces it back to one.
    pub async fn metric_now(
        &self,
        query: &str,
        at: u64,
        label: &str,
    ) -> Result<Vec<(String, f64)>, String> {
        Ok(self
            .metric_now_by(query, at, &[label])
            .await?
            .into_iter()
            .map(|(mut k, n)| (k.pop().unwrap_or_default(), n))
            .collect())
    }

    /// redesign-371-metrics: [`Loki::metric_now`] for a query that groups
    /// by several labels (the Traffic tab's top errors: status, hostname,
    /// path) — each row's values of `labels`, in that order, and its number,
    /// biggest first. The same `query_range` read, so the gateway lets it
    /// through (fix-221).
    pub async fn metric_now_by(
        &self,
        query: &str,
        at: u64,
        labels: &[&str],
    ) -> Result<Vec<(Vec<String>, f64)>, String> {
        let start = at.saturating_sub(1);
        let v = self
            .get(
                "query_range",
                &[
                    ("query", query.to_string()),
                    ("start", start.to_string()),
                    ("end", at.to_string()),
                    ("step", "1s".to_string()),
                ],
            )
            .await?;
        let mut out: Vec<(Vec<String>, f64)> = v["data"]["result"]
            .as_array()
            .map(|rs| {
                rs.iter()
                    .filter_map(|r| {
                        let k = labels
                            .iter()
                            .map(|l| r["metric"][*l].as_str().unwrap_or("").to_string())
                            .collect();
                        let last = r["values"].as_array()?.last()?;
                        let n: f64 = last[1].as_str()?.parse().ok()?;
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
            return Err(upstream_reason("Loki", status, &body));
        }
        serde_json::from_str(&body).map_err(|e| format!("Loki's answer did not read: {e}"))
    }
}

/// fix-221: an upstream's error page (nginx's, Traefik's, Loki's own HTML
/// 400 page) never reaches the browser as page text — only the status and a
/// short reason do. A body that is not HTML is still shown, truncated, same
/// as before.
fn upstream_reason(service: &str, status: reqwest::StatusCode, body: &str) -> String {
    let trimmed = body.trim_start();
    let looks_like_html = trimmed
        .get(..15)
        .unwrap_or(trimmed)
        .to_ascii_lowercase()
        .starts_with("<!doctype html")
        || trimmed
            .get(..5)
            .unwrap_or(trimmed)
            .to_ascii_lowercase()
            .starts_with("<html")
        || body.to_ascii_lowercase().contains("<html");
    if looks_like_html {
        format!("{service} refused the query (HTTP {})", status.as_u16())
    } else {
        format!(
            "{service} answered HTTP {}: {}",
            status.as_u16(),
            body.chars().take(200).collect::<String>()
        )
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

    /// A client for a given base (feat-platform-10's demo metrics server;
    /// also this module's own tests) rather than the project config.
    pub fn new(
        base: String,
        timeout: Duration,
        max_since_s: u64,
        max_limit: usize,
    ) -> Option<Self> {
        let http = reqwest::Client::builder().timeout(timeout).build().ok()?;
        Some(Loki {
            base,
            http,
            max_since_s,
            max_limit,
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
            return Err(upstream_reason(
                &format!("Loki at {}", self.base),
                status,
                &body,
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
