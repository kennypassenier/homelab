//! redesign-371-metrics: the demo host's made-up Prometheus and Loki
//! answers, as time series over the window a chart asks for.
//!
//! Before 3.71.0 every series was one point (the old charts only read the
//! last one), so the redesigned Metrics page (crosshair, change over the
//! hour before, drag to zoom, event markers) had nothing to show in the
//! browser tests and the screenshots. Here each query gets a deterministic
//! curve — a daily wave, a little noise, the nightly round's bump around
//! 03:00 and the odd event — that ends exactly on the value the demo host's
//! fleet reports, so a KPI tile and the chart under it agree.
//!
//! Generic on purpose (invariant 10): no stack or app is named here. The
//! fleet-wide `by (stack)` queries answer one series per stack the query
//! itself lists; per-app series are `<stack>-1`, `<stack>-2`; the access log
//! answers made-up `*.example.org` hostnames and documentation addresses
//! (RFC 5737 / RFC 3849).
//!
//! `demo-host` only (shell/mod.rs), like the rest of the demo host.

use std::f64::consts::PI;

/// The window a query covers: `query_range`'s start/end/step, or one
/// instant (`query`, and the one-second `query_range` of an instant read).
#[derive(Debug, Clone, Copy)]
pub struct Window {
    pub start: u64,
    pub end: u64,
    pub step: u64,
}

impl Window {
    /// `query_range`'s parameters as Prometheus (`"360"`) and Loki
    /// (`"360s"`) send them; `None` (an instant `query`) is one point now.
    pub fn from_params(
        start: Option<&str>,
        end: Option<&str>,
        step: Option<&str>,
        now: u64,
    ) -> Self {
        let num = |s: Option<&str>| -> Option<u64> {
            let s = s?.trim().trim_end_matches('s');
            s.parse::<f64>().ok().map(|f| f.max(0.0) as u64)
        };
        let end = num(end).unwrap_or(now);
        let start = num(start).unwrap_or(end).min(end);
        let step = num(step).unwrap_or(60).max(1);
        Window { start, end, step }
    }

    fn times(&self) -> Vec<u64> {
        if self.end <= self.start {
            return vec![self.end];
        }
        // Prometheus refuses more than 11 000 points; so does this.
        let step = self.step.max((self.end - self.start) / 11_000 + 1);
        let mut out: Vec<u64> = (self.start..=self.end).step_by(step as usize).collect();
        if out.last() != Some(&self.end) {
            out.push(self.end);
        }
        out
    }
}

/// FNV-1a: a stable number from a name, the seed of its curve.
fn hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// Noise in [-0.5, 0.5], the same for the same seed and minute.
fn noise(seed: u64, t: u64) -> f64 {
    let x = hash(&format!("{seed}:{}", t / 60));
    (x % 10_000) as f64 / 10_000.0 - 0.5
}

/// One made-up series: how it moves and where it ends.
#[derive(Debug, Clone)]
pub struct Shape {
    pub label: String,
    /// The value at the end of the window (what the fleet reports now).
    pub last: f64,
    /// The level it moves around.
    pub base: f64,
    /// The daily wave, as a share of `base`.
    pub wave: f64,
    /// The noise, as a share of `base`.
    pub jitter: f64,
    /// The nightly round's bump at 03:00 UTC, in the series' own unit.
    pub night: f64,
    /// Extra bumps: (seconds before now, height, width in seconds).
    pub bumps: Vec<(u64, f64, f64)>,
    /// Whole numbers only (a count of sectors, of requests).
    pub whole: bool,
}

impl Shape {
    pub fn new(label: impl Into<String>, last: f64) -> Self {
        Shape {
            label: label.into(),
            last,
            base: last,
            wave: 0.0,
            jitter: 0.0,
            night: 0.0,
            bumps: Vec::new(),
            whole: false,
        }
    }
    fn base(mut self, b: f64) -> Self {
        self.base = b;
        self
    }
    fn wave(mut self, w: f64) -> Self {
        self.wave = w;
        self
    }
    fn jitter(mut self, j: f64) -> Self {
        self.jitter = j;
        self
    }
    fn night(mut self, n: f64) -> Self {
        self.night = n;
        self
    }
    fn bump(mut self, ago: u64, h: f64, w: f64) -> Self {
        self.bumps.push((ago, h, w));
        self
    }
    fn whole(mut self) -> Self {
        self.whole = true;
        self
    }

    fn raw(&self, seed: u64, t: u64, now: u64) -> f64 {
        let day = (t % 86_400) as f64 / 86_400.0;
        let mut v = self.base
            * (1.0 + self.wave * (2.0 * PI * (day - 0.3)).sin() + self.jitter * noise(seed, t));
        if self.night != 0.0 {
            // The nearest 03:00 UTC.
            let c = (t / 86_400) * 86_400 + 3 * 3_600;
            for c in [c.saturating_sub(86_400), c, c + 86_400] {
                let d = (t as f64 - c as f64) / 1_200.0;
                v += self.night * (-d * d).exp();
            }
        }
        for &(ago, h, w) in &self.bumps {
            let d = (t as f64 - now.saturating_sub(ago) as f64) / w.max(1.0);
            v += h * (-d * d).exp();
        }
        v
    }

    /// The series over `w`, blended in its last twentieth onto `last`.
    pub fn points(&self, w: &Window, now: u64) -> Vec<(u64, f64)> {
        let seed = hash(&self.label);
        let ts = w.times();
        let span = (w.end - w.start) as f64;
        let tail = (span * 0.05).max(1.0);
        let off = self.last - self.raw(seed, w.end, now);
        ts.iter()
            .map(|&t| {
                let k = if span == 0.0 {
                    1.0
                } else {
                    ((t as f64 - (w.end as f64 - tail)) / tail).clamp(0.0, 1.0)
                };
                let mut v = self.raw(seed, t, now) + off * k;
                v = v.max(0.0);
                if self.whole {
                    v = v.round();
                }
                (t, v)
            })
            .collect()
    }
}

/// A made-up answer: one series per shape, under `key` (the label the
/// query groups by), as a `matrix` with an instant `value` too, so both
/// `query` and `query_range` readers find what they read.
fn answer(key: &str, shapes: &[Shape], w: &Window, now: u64) -> serde_json::Value {
    let result: Vec<serde_json::Value> = shapes
        .iter()
        .map(|s| {
            let pts = s.points(w, now);
            let last = pts.last().copied().unwrap_or((now, s.last));
            serde_json::json!({
                "metric": { key: s.label },
                "value": [last.0, last.1.to_string()],
                "values": pts.iter().map(|(t, v)| serde_json::json!([t, v.to_string()])).collect::<Vec<_>>(),
            })
        })
        .collect();
    serde_json::json!({ "status": "success", "data": { "resultType": "matrix", "result": result } })
}

/// A step series: `before` until `ago` seconds before now, `after` since.
fn step_shape(
    label: &str,
    before: f64,
    after: f64,
    ago: u64,
    w: &Window,
    now: u64,
) -> serde_json::Value {
    let at = now.saturating_sub(ago);
    let vals: Vec<serde_json::Value> = w
        .times()
        .iter()
        .map(|&t| serde_json::json!([t, (if t < at { before } else { after }).to_string()]))
        .collect();
    serde_json::json!({
        "metric": { "device": label },
        "value": [w.end, after.to_string()],
        "values": vals,
    })
}

fn matrix(result: Vec<serde_json::Value>) -> serde_json::Value {
    serde_json::json!({ "status": "success", "data": { "resultType": "matrix", "result": result } })
}

/// The names a fleet-wide query lists in its `stack=~"a|b|c"` matcher.
fn listed_stacks(query: &str) -> Vec<String> {
    let Some(i) = query.find("stack=~\"") else {
        return Vec::new();
    };
    let rest = &query[i + 8..];
    let end = rest.find('"').unwrap_or(rest.len());
    rest[..end]
        .split('|')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// The one stack a per-stack query names (`stack="x"`).
fn one_stack(query: &str) -> Option<String> {
    let i = query.find("stack=\"")?;
    let rest = &query[i + 7..];
    Some(rest[..rest.find('"')?].to_string())
}

/// The `[Ns]` lookback of a log query: the whole window for a table, one
/// step for a chart.
fn lookback(query: &str) -> u64 {
    let Some(i) = query.rfind('[') else { return 60 };
    let rest = &query[i + 1..];
    rest[..rest.find('s').unwrap_or(0)].parse().unwrap_or(60)
}

const GIB: f64 = 1_073_741_824.0;
/// The access log's made-up hostnames and their share of the requests.
const HOSTS: &[(&str, f64)] = &[
    ("demo.example.org", 0.706),
    ("films.example.org", 0.118),
    ("notes.example.org", 0.084),
    ("shop.example.org", 0.054),
    ("admin.example.org", 0.024),
    ("blog.example.org", 0.014),
];
/// Status codes and their share; the 5xx spike is 36 minutes ago.
const STATUSES: &[(&str, f64)] = &[
    ("200", 0.885),
    ("204", 0.02),
    ("301", 0.018),
    ("304", 0.013),
    ("404", 0.045),
    ("403", 0.01),
    ("401", 0.005),
    ("500", 0.003),
    ("502", 0.001),
];
/// Client addresses (documentation ranges) and their share; "" is a log
/// line that carried no address.
const CLIENTS: &[(&str, f64)] = &[
    ("203.0.113.4", 0.31),
    ("198.51.100.1", 0.22),
    ("", 0.13),
    ("203.0.113.17", 0.07),
    ("198.51.100.90", 0.05),
    ("2001:db8::9e1", 0.04),
];
/// Requests per second, on average, over the day.
const RATE: f64 = 0.44;

/// The access log's answers (the Traffic tab).
fn traffic(query: &str, w: &Window, now: u64) -> serde_json::Value {
    let span = lookback(query) as f64;
    // An instant read of a whole window (a table, a total): one number per
    // row, from the window's length.
    let total = RATE * span;
    if query.contains("DownstreamStatus >= 400") {
        let rows = [
            ("404", "demo.example.org", "/favicon.ico", 0.032),
            ("404", "demo.example.org", "/robots.txt", 0.006),
            ("403", "shop.example.org", "/wp-login.php", 0.008),
            ("500", "films.example.org", "/api/stream", 0.002),
        ];
        return matrix(
            rows.iter()
                .map(|(code, host, path, share)| {
                    let n = (total * share).round();
                    serde_json::json!({
                        "metric": { "DownstreamStatus": code, "RequestHost": host, "RequestPath": path },
                        "value": [w.end, n.to_string()],
                        "values": [[w.end, n.to_string()]],
                    })
                })
                .collect(),
        );
    }
    if query.contains("count(sum by (ClientHost)") {
        let n = (213.0 * (span / 86_400.0).powf(0.35)).round().max(1.0);
        return answer("", &[Shape::new("", n)], w, now);
    }
    if query.contains("offset") {
        return answer("", &[Shape::new("", (total * 0.926).round())], w, now);
    }
    if query.starts_with("sum(count_over_time") {
        return answer("", &[Shape::new("", total.round())], w, now);
    }
    // A chart (one count per step) when the lookback is the step; a table
    // (one count for the whole window) otherwise.
    let chart = w.end > w.start + 1;
    let per = if chart { RATE * span } else { total };
    let shapes = |key: &str, rows: &[(&str, f64)], spike: Option<(&str, f64)>| {
        let shapes: Vec<Shape> = rows
            .iter()
            .map(|(label, share)| {
                let v = per * share;
                let mut s = Shape::new(*label, if chart { v * 1.1 } else { v.round() });
                if chart {
                    s = s.base(v).wave(0.45).jitter(0.5).whole();
                    if let Some((code, h)) = spike
                        && code == *label
                    {
                        s = s.bump(2_160, per * h, 500.0);
                    }
                }
                s
            })
            .collect();
        answer(key, &shapes, w, now)
    };
    if query.contains("by (RequestHost)") {
        return shapes("RequestHost", HOSTS, None);
    }
    if query.contains("by (DownstreamStatus)") {
        return shapes("DownstreamStatus", STATUSES, Some(("500", 0.03)));
    }
    if query.contains("by (ClientHost)") {
        return shapes("ClientHost", CLIENTS, None);
    }
    answer("", &[Shape::new("", per)], w, now)
}

/// Every made-up answer, chosen from the query's own text — the same
/// shapes a real Prometheus or Loki answers for that `by (...)` clause.
/// Deliberately raw ids where the live host has them (fix-220): a guest's
/// firewall bridge and veth (`fwbr117i0`, `veth118i0`), an hwmon chip's
/// sysfs PCI path, a Proxmox cgroup id (`lxc/117`) — the humanizing code
/// path turns them into words, and a whole-screen invariant proves it.
/// One SMART drive turns "not ok" 20 hours ago (`sdb`) so Drives has both
/// states and a "since".
pub fn body(query: &str, w: &Window, now: u64) -> serde_json::Value {
    let day_ago = 86_400 + 4 * 3_600;
    if query.contains("job=\"") {
        return traffic(query, w, now);
    }
    // Fleet-wide: one series per stack the query lists.
    let stacks = listed_stacks(query);
    if !stacks.is_empty() {
        let pick = |s: &str, shift: u32, lo: f64, span: f64| -> f64 {
            lo + ((hash(s) >> shift) % 1_000) as f64 / 1_000.0 * span
        };
        let shapes: Vec<Shape> = stacks
            .iter()
            .map(|s| {
                let last = if query.contains("node_cpu_seconds_total") {
                    pick(s, 0, 1.0, 14.0).round()
                } else if query.contains("node_memory") {
                    pick(s, 12, 15.0, 70.0).round()
                } else if query.contains("node_filesystem") {
                    pick(s, 24, 10.0, 85.0).round()
                } else if query.contains("transmit") {
                    pick(s, 36, 600.0, 40_000.0).round()
                } else {
                    pick(s, 48, 1_000.0, 120_000.0).round()
                };
                Shape::new(s.clone(), last).wave(0.1).jitter(0.1)
            })
            .collect();
        return answer("stack", &shapes, w, now);
    }
    // One stack's own charts.
    if let Some(stack) = one_stack(query) {
        let apps = [format!("{stack}-1"), format!("{stack}-2")];
        let per_app = |lasts: [f64; 2], wave: f64| -> Vec<Shape> {
            apps.iter()
                .zip(lasts)
                .map(|(a, l)| {
                    Shape::new(a.clone(), l)
                        .wave(wave)
                        .jitter(0.2)
                        .night(l * 0.6)
                })
                .collect()
        };
        if query.contains("by (name)") {
            let shapes = if query.contains("container_cpu") {
                per_app([6.0, 2.0], 0.3)
            } else if query.contains("container_memory") {
                per_app([420.0 * 1_048_576.0, 180.0 * 1_048_576.0], 0.02)
            } else if query.contains("fs_writes") {
                per_app([52_000.0, 9_000.0], 0.4)
            } else if query.contains("network_receive") {
                per_app([38_000.0, 4_100.0], 0.5)
            } else {
                apps.iter()
                    .map(|a| Shape::new(a.clone(), 0.0).whole())
                    .collect()
            };
            return answer("name", &shapes, w, now);
        }
        let shape = if query.contains("node_cpu_seconds_total") {
            Shape::new("", 8.0)
                .base(9.0)
                .wave(0.3)
                .jitter(0.3)
                .night(30.0)
        } else if query.contains("node_memory") {
            Shape::new("", 640.0 * 1_048_576.0)
                .wave(0.02)
                .jitter(0.01)
                .night(120.0 * 1_048_576.0)
        } else {
            Shape::new("", 41.0).base(40.6).jitter(0.004)
        };
        return answer("stack", &[shape], w, now);
    }
    // The hypervisor's own charts.
    if query.contains("node_hwmon_temp_celsius") {
        return answer(
            "chip",
            &[
                Shape::new("0000:00:01_0_0000:01:00_0", 46.0)
                    .base(44.0)
                    .wave(0.04)
                    .jitter(0.03)
                    .night(9.0),
                Shape::new("0000:00:02_0_0000:02:00_0", 52.0)
                    .base(50.0)
                    .wave(0.04)
                    .jitter(0.03)
                    .night(9.0),
            ],
            w,
            now,
        );
    }
    if query.contains("smart_device_health_ok") {
        return matrix(vec![
            step_shape("sda", 1.0, 1.0, 0, w, now),
            step_shape("sdb", 1.0, 0.0, 20 * 3_600, w, now),
        ]);
    }
    if query.contains("smart_device_pending_sectors") {
        return matrix(vec![
            step_shape("sda", 0.0, 0.0, 0, w, now),
            step_shape("sdb", 7.0, 12.0, 9 * 3_600, w, now),
        ]);
    }
    if query.contains("smart_device_reallocated_sectors") {
        return matrix(vec![
            step_shape("sda", 0.0, 0.0, 0, w, now),
            step_shape("sdb", 1.0, 3.0, day_ago, w, now),
        ]);
    }
    if query.contains("smart_device_temperature_celsius") {
        return answer(
            "device",
            &[
                Shape::new("sda", 34.0).wave(0.05).jitter(0.03).night(6.0),
                Shape::new("sdb", 41.0).wave(0.05).jitter(0.03).night(6.0),
            ],
            w,
            now,
        );
    }
    if query.contains("smart_device_power_on_hours") {
        let hours = |label: &str, last: f64| {
            let vals: Vec<serde_json::Value> = w
                .times()
                .iter()
                .map(|&t| {
                    serde_json::json!([
                        t,
                        (last - (w.end - t) as f64 / 3_600.0).floor().to_string()
                    ])
                })
                .collect();
            serde_json::json!({ "metric": { "device": label }, "value": [w.end, last.to_string()], "values": vals })
        };
        return matrix(vec![hours("sda", 8_760.0), hours("sdb", 12_000.0)]);
    }
    if query.contains("pve_memory_usage_bytes") {
        return answer(
            "id",
            &[
                Shape::new("lxc/117", 2.0 * GIB).wave(0.04).jitter(0.02),
                Shape::new("lxc/118", GIB).wave(0.04).jitter(0.02),
            ],
            w,
            now,
        );
    }
    if query.contains("node_network_receive_bytes_total") {
        return answer(
            "device",
            &[
                Shape::new("vmbr0", 120_000.0)
                    .base(60_000.0)
                    .wave(0.6)
                    .jitter(0.5)
                    .bump(7_560, 2_400_000.0, 600.0),
                Shape::new("fwbr117i0", 4_200.0).wave(0.6).jitter(0.5),
                Shape::new("veth118i0", 1_800.0).wave(0.6).jitter(0.5),
            ],
            w,
            now,
        );
    }
    if query.contains("node_load") {
        return answer(
            "",
            &[Shape::new("", 0.8)
                .base(0.9)
                .wave(0.4)
                .jitter(0.3)
                .night(6.0)],
            w,
            now,
        );
    }
    if query.contains("node_filesystem") {
        return answer(
            "mountpoint",
            &[
                Shape::new("/", 31.0).base(30.6).jitter(0.002),
                Shape::new("/mnt/pve-backup", 58.0)
                    .base(55.0)
                    .jitter(0.002)
                    .bump(15_480, 3.0, 9_000.0),
            ],
            w,
            now,
        );
    }
    if query.contains("node_memory") {
        return answer(
            "",
            &[Shape::new("", 26_000.0 * 1_048_576.0)
                .base(24.5 * GIB)
                .wave(0.03)
                .jitter(0.01)
                .night(3.0 * GIB)],
            w,
            now,
        );
    }
    if query.contains("node_cpu_seconds_total") {
        return answer(
            "",
            &[Shape::new("", 7.0)
                .base(9.0)
                .wave(0.35)
                .jitter(0.25)
                .night(29.0)],
            w,
            now,
        );
    }
    answer("", &[Shape::new("", 12.5)], w, now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_answer_spans_the_window_and_ends_on_the_reported_value() {
        let now = 1_790_000_000;
        let w = Window::from_params(Some("1789913600"), Some("1790000000"), Some("360"), now);
        let b = body(
            "100 * (1 - avg(rate(node_cpu_seconds_total{host=\"demo\",mode=\"idle\"}[5m])))",
            &w,
            now,
        );
        let vals = b["data"]["result"][0]["values"].as_array().unwrap();
        assert_eq!(vals.len(), 241);
        assert_eq!(vals.last().unwrap()[1].as_str().unwrap(), "7");
    }

    #[test]
    fn a_fleet_query_answers_one_series_per_listed_stack() {
        let now = 1_790_000_000;
        let w = Window::from_params(None, None, None, now);
        let b = body(
            "100 * avg by (stack) (rate(node_cpu_seconds_total{stack=~\"a|b|c\",mode!=\"idle\"}[5m]))",
            &w,
            now,
        );
        let r = b["data"]["result"].as_array().unwrap();
        let names: Vec<&str> = r
            .iter()
            .map(|x| x["metric"]["stack"].as_str().unwrap())
            .collect();
        assert_eq!(names, ["a", "b", "c"]);
    }

    #[test]
    fn the_access_log_tables_answer_whole_window_counts() {
        let now = 1_790_000_000;
        let w = Window::from_params(Some("1789999999"), Some("1790000000"), Some("1s"), now);
        let q = "topk(20, sum by (RequestHost) (count_over_time({job=\"demo-traffic\"} | json | __error__=\"\" [86400s])))";
        let b = body(q, &w, now);
        let first = &b["data"]["result"][0];
        assert_eq!(first["metric"]["RequestHost"], "demo.example.org");
        let n: f64 = first["values"].as_array().unwrap().last().unwrap()[1]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!(n > 20_000.0, "{n}");
    }
}
