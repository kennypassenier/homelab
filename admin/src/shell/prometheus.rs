//! replace-grafana (2026-09-30): the dashboard's server asks Prometheus for
//! the charts; the browser never does. The address is a setting
//! (`admin.prometheus_url`, HOMELAB_ADMIN_PROMETHEUS_URL).

use std::time::Duration;

use crate::core::config::AdminConfig;

#[derive(Clone)]
pub struct Prometheus {
    base: String,
    http: reqwest::Client,
    /// `admin.charts_host`: the hypervisor's `host` label.
    pub host_label: Option<String>,
}

impl Prometheus {
    /// A client for the configured Prometheus, or None when none is set.
    pub fn from_config(c: Option<&AdminConfig>) -> Option<Self> {
        let c = c?;
        let base = c.prometheus_base()?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(c.loki_timeout_s))
            .build()
            .ok()?;
        Some(Prometheus {
            base,
            http,
            host_label: c.charts_host.clone().filter(|h| !h.trim().is_empty()),
        })
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    /// feat-overview-11 (capacity map): one `query` (instant, now), answered
    /// the same shape as [`Prometheus::range`] but with one point per
    /// series — so the capacity map and the charts page read the same
    /// `{label, points: [[t, v]]}` shape.
    pub async fn instant(
        &self,
        query: &str,
        legend: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let url = format!("{}/api/v1/query", self.base);
        let res = self
            .http
            .get(&url)
            .query(&[("query", query)])
            .send()
            .await
            .map_err(|e| format!("Prometheus at {} did not answer: {e}", self.base))?;
        let status = res.status();
        let body: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Prometheus's answer did not read: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "Prometheus answered HTTP {}: {}",
                status.as_u16(),
                body["error"].as_str().unwrap_or("")
            ));
        }
        Ok(instant_series(&body, legend))
    }

    /// One `query_range`, answered as `[{label, points: [[t, v], …]}]`, the
    /// series told apart by the value of `legend` (or numbered).
    pub async fn range(
        &self,
        query: &str,
        start: u64,
        end: u64,
        step: u64,
        legend: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, String> {
        let url = format!("{}/api/v1/query_range", self.base);
        let res = self
            .http
            .get(&url)
            .query(&[
                ("query", query),
                ("start", start.to_string().as_str()),
                ("end", end.to_string().as_str()),
                ("step", step.to_string().as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("Prometheus at {} did not answer: {e}", self.base))?;
        let status = res.status();
        let body: serde_json::Value = res
            .json()
            .await
            .map_err(|e| format!("Prometheus's answer did not read: {e}"))?;
        if !status.is_success() {
            return Err(format!(
                "Prometheus answered HTTP {}: {}",
                status.as_u16(),
                body["error"].as_str().unwrap_or("")
            ));
        }
        Ok(series(&body, legend))
    }
}

/// A `vector` answer (one `[unix, value]` per series) in the same shape
/// [`series`] gives a `matrix` answer, so a caller that wants either can
/// treat them alike.
pub fn instant_series(body: &serde_json::Value, legend: Option<&str>) -> Vec<serde_json::Value> {
    body["data"]["result"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .enumerate()
                .map(|(i, r)| {
                    let label = legend
                        .and_then(|l| r["metric"][l].as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            if rs.len() == 1 {
                                String::new()
                            } else {
                                format!("series {}", i + 1)
                            }
                        });
                    let points: Vec<serde_json::Value> = r["value"]
                        .as_array()
                        .and_then(|p| {
                            let t = p.first()?.as_f64()?;
                            let v: f64 = p.get(1)?.as_str()?.parse().ok()?;
                            v.is_finite().then(|| serde_json::json!([t, v]))
                        })
                        .into_iter()
                        .collect();
                    serde_json::json!({ "label": label, "points": points })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A `matrix` answer as series of `[unix, value]` points.
pub fn series(body: &serde_json::Value, legend: Option<&str>) -> Vec<serde_json::Value> {
    body["data"]["result"]
        .as_array()
        .map(|rs| {
            rs.iter()
                .enumerate()
                .map(|(i, r)| {
                    let label = legend
                        .and_then(|l| r["metric"][l].as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| {
                            if rs.len() == 1 {
                                String::new()
                            } else {
                                format!("series {}", i + 1)
                            }
                        });
                    let points: Vec<serde_json::Value> = r["values"]
                        .as_array()
                        .map(|vs| {
                            vs.iter()
                                .filter_map(|p| {
                                    let t = p[0].as_f64()?;
                                    let v: f64 = p[1].as_str()?.parse().ok()?;
                                    v.is_finite().then(|| serde_json::json!([t, v]))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    serde_json::json!({ "label": label, "points": points })
                })
                .collect()
        })
        .unwrap_or_default()
}
