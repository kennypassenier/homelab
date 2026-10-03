//! replace-goaccess / redesign-371-metrics: the Traffic tab's reads of the
//! proxy's access log in Loki, as one answer for `/data/traffic`.
//!
//! The reads are independent, so they run together (the review measured
//! seven in a row against one Loki). The top errors are read in two steps:
//! grouping by the raw path would make one series per path a scanner
//! tries, which hits Loki's series limit on real traffic, so the first read
//! groups by status and hostname only, and a second read per winning pair
//! asks for its one busiest path, query strings stripped. The window before
//! a week or a month barely moves between two reads 30 s apart, so its
//! total is kept for a while instead of read every time.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use super::loki::Loki;

/// How long the window before this one stays good: a day's share of a
/// week, a sixth of a day for a month; the shorter windows never keep it.
fn prev_ttl(span: u64) -> Option<u64> {
    (span >= 7 * 86_400).then_some(span / 168)
}

/// The previous window's total per (Loki, job, span): when it was read and
/// what it said.
type PrevCache = Mutex<HashMap<(String, String, u64), (u64, serde_json::Value)>>;

fn prev_cache() -> &'static PrevCache {
    static C: OnceLock<PrevCache> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// The biggest error answers kept beside the status mix.
const TOP_ERRORS: usize = 5;

/// Every read the Traffic tab shows, over the `span` seconds ending at
/// `end`, charts in `step`-second buckets.
pub async fn read(loki: &Loki, job: &str, span: u64, step: u64, end: u64) -> serde_json::Value {
    let job = job.replace('"', "");
    let sel = format!("{{job=\"{job}\"}} | json | __error__=\"\"");
    let start = end.saturating_sub(span);
    let per = |by: &str| format!("topk(8, sum by ({by}) (count_over_time({sel} [{step}s])))");
    let host_q = per("RequestHost");
    let status_q = per("DownstreamStatus");
    let top = |by: &str| format!("topk(20, sum by ({by}) (count_over_time({sel} [{span}s])))");
    let hosts_q = top("RequestHost");
    let clients_q = top("ClientHost");
    let prev_q = format!("sum(count_over_time({sel} [{span}s] offset {span}s))");
    let count_q = format!("count(sum by (ClientHost) (count_over_time({sel} [{span}s])))");
    let pairs_q = format!(
        "topk({TOP_ERRORS}, sum by (DownstreamStatus, RequestHost) (count_over_time({sel} | DownstreamStatus >= 400 [{span}s])))"
    );
    let key = (loki.base().to_string(), job.clone(), span);
    let cached_prev = prev_ttl(span).and_then(|ttl| {
        let c = prev_cache().lock().ok()?;
        c.get(&key)
            .filter(|(at, _)| end.saturating_sub(*at) < ttl)
            .map(|(_, v)| v.clone())
    });
    let one = |r: Result<Vec<(String, f64)>, String>| {
        r.ok()
            .and_then(|rows| rows.first().map(|x| x.1))
            .map(serde_json::Value::from)
            .unwrap_or(serde_json::Value::Null)
    };
    let prev_read = async {
        match &cached_prev {
            Some(v) => v.clone(),
            None => one(loki.metric_now(&prev_q, end, "").await),
        }
    };
    let (by_host, by_status, hosts, clients, prev_total, clients_total, pairs) = tokio::join!(
        loki.metric_range(&host_q, start, end, step, Some("RequestHost")),
        loki.metric_range(&status_q, start, end, step, Some("DownstreamStatus")),
        loki.metric_now(&hosts_q, end, "RequestHost"),
        loki.metric_now(&clients_q, end, "ClientHost"),
        prev_read,
        loki.metric_now(&count_q, end, ""),
        loki.metric_now_by(&pairs_q, end, &["DownstreamStatus", "RequestHost"]),
    );
    if cached_prev.is_none()
        && prev_ttl(span).is_some()
        && !prev_total.is_null()
        && let Ok(mut c) = prev_cache().lock()
    {
        c.insert(key, (end, prev_total.clone()));
    }

    let panel = |title: &str, desc: &str, query: &str, legend: &str, r| {
        let p = serde_json::json!({ "title": title, "desc": desc, "query": query, "unit": "count", "legend": legend });
        match r {
            Ok(series) => serde_json::json!({ "panel": p, "series": series }),
            Err(e) => serde_json::json!({ "panel": p, "series": [], "error": e }),
        }
    };
    let out = vec![
        panel(
            "Requests per hostname",
            "How many requests each hostname behind the proxy received in \
             this window, from its access log.",
            &host_q,
            "RequestHost",
            by_host,
        ),
        panel(
            "Requests per status",
            "How many requests answered with each HTTP status in this \
             window; a rising share of 4xx/5xx is worth a look.",
            &status_q,
            "DownstreamStatus",
            by_status,
        ),
    ];

    // The busiest path of each winning (status, hostname) pair, its query
    // string stripped so `/a?x=1` and `/a?x=2` count as one path.
    let errors = match pairs {
        Ok(pairs) => {
            let paths = futures_util::future::join_all(pairs.iter().map(|(k, _)| {
                let status = k[0].replace('"', "");
                let host = k[1].replace('"', "");
                let q = format!(
                    "topk(1, sum by (path) (count_over_time({sel} | DownstreamStatus=\"{status}\" | RequestHost=\"{host}\" | label_format path=`{{{{ regexReplaceAll \"\\\\?.*\" .RequestPath \"\" }}}}` [{span}s])))"
                );
                async move { loki.metric_now(&q, end, "path").await }
            }))
            .await;
            serde_json::json!({
                "rows": pairs
                    .into_iter()
                    .zip(paths)
                    .map(|((k, n), p)| {
                        let path = p.ok().and_then(|rows| rows.into_iter().next()).map(|x| x.0).unwrap_or_default();
                        serde_json::json!({ "status": k[0], "host": k[1], "path": path, "n": n })
                    })
                    .collect::<Vec<_>>(),
            })
        }
        Err(e) => serde_json::json!({ "rows": [], "error": e }),
    };
    let table = |r: Result<Vec<(String, f64)>, String>| match r {
        Ok(rows) => serde_json::json!({ "rows": rows }),
        Err(e) => serde_json::json!({ "rows": [], "error": e }),
    };
    serde_json::json!({
        "panels": out,
        "hosts": table(hosts),
        "clients": table(clients),
        "prev_total": prev_total,
        "clients_total": clients_total,
        "errors": errors,
        "from": start,
        "to": end,
        "step": step,
    })
}
