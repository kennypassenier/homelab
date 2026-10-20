//! ask-kp11be-2 (watch and ping history): what the minute watch measured,
//! kept as hourly cells so the Apps tiles can draw a 24 h / 90 d strip and
//! the Host page a reachability figure. Pure: no clock, no I/O.
//!
//! Bounded up front (Kenny's rule: anything that grows has a cap): a cell
//! is one hour, a key keeps [`KEEP_HOURS`] of them (90 days), the store keeps
//! at most [`MAX_KEYS`] keys, and a sample only ever adds counts to the
//! current hour's cell, so a busy watch grows nothing.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// 90 days of hourly cells per key.
pub const KEEP_HOURS: i64 = 90 * 24;
/// Most keys held: tiles + stack containers + the host stay far below it;
/// a stack that is removed loses its cells with age, and past this number
/// the key whose newest cell is oldest goes first.
pub const MAX_KEYS: usize = 300;

/// One watch reading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sample {
    Up,
    /// Known outage: its stack was deploying.
    Deploying,
    Down,
}

/// One hour of one key. `ms_*` are the round trips of the pings measured in
/// that hour (only the `ping:host` key carries them).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Cell {
    /// Hours since the epoch.
    pub h: i64,
    #[serde(default)]
    pub up: u32,
    #[serde(default)]
    pub dep: u32,
    #[serde(default)]
    pub down: u32,
    #[serde(default)]
    pub ms_sum: u64,
    #[serde(default)]
    pub ms_n: u32,
    #[serde(default)]
    pub ms_max: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Store {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub keys: BTreeMap<String, Vec<Cell>>,
}

impl Store {
    /// Drop what is older than [`KEEP_HOURS`] and, past [`MAX_KEYS`], the
    /// keys that have been quiet longest. Run after every load and record.
    pub fn trim(&mut self, now: i64) {
        let oldest = now.div_euclid(3600) - KEEP_HOURS;
        self.keys.retain(|_, cells| {
            cells.retain(|c| c.h > oldest);
            !cells.is_empty()
        });
        while self.keys.len() > MAX_KEYS {
            let Some(victim) = self
                .keys
                .iter()
                .min_by_key(|(_, c)| c.last().map(|c| c.h).unwrap_or(i64::MIN))
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            self.keys.remove(&victim);
        }
    }

    fn cell(&mut self, key: &str, now: i64) -> &mut Cell {
        let h = now.div_euclid(3600);
        let cells = self.keys.entry(key.to_string()).or_default();
        if cells.last().is_none_or(|c| c.h < h) {
            cells.push(Cell {
                h,
                ..Default::default()
            });
        }
        // The clock stepping back puts the sample in the newest cell
        // rather than reordering history.
        let last = cells.len() - 1;
        &mut cells[last]
    }

    /// Count one watch reading into the current hour.
    pub fn record(&mut self, key: &str, now: i64, s: Sample) {
        let c = self.cell(key, now);
        match s {
            Sample::Up => c.up = c.up.saturating_add(1),
            Sample::Deploying => c.dep = c.dep.saturating_add(1),
            Sample::Down => c.down = c.down.saturating_add(1),
        }
        self.trim_if_new(now);
    }

    /// Count one measured ping: `Some(ms)` answered, `None` not.
    pub fn record_ping(&mut self, key: &str, now: i64, ms: Option<u64>) {
        let c = self.cell(key, now);
        match ms {
            Some(ms) => {
                c.up = c.up.saturating_add(1);
                c.ms_sum = c.ms_sum.saturating_add(ms);
                c.ms_n = c.ms_n.saturating_add(1);
                c.ms_max = c.ms_max.max(u32::try_from(ms).unwrap_or(u32::MAX));
            }
            None => c.down = c.down.saturating_add(1),
        }
        self.trim_if_new(now);
    }

    /// Trimming walks every key, so it runs once an hour (when a cell was
    /// just opened), not per sample.
    fn trim_if_new(&mut self, now: i64) {
        let h = now.div_euclid(3600);
        let fresh = self.keys.values().any(|c| {
            c.last()
                .is_some_and(|c| c.h == h && c.up + c.dep + c.down <= 1)
        });
        if fresh || self.keys.len() > MAX_KEYS {
            self.trim(now);
        }
    }
}

/// One bucket of a strip.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Period {
    /// Unix seconds of the bucket's start.
    pub start: i64,
    pub state: &'static str,
    /// Share of the readings that were up, 0..1; None without readings.
    pub uptime: Option<f64>,
    pub up: u32,
    pub dep: u32,
    pub down: u32,
    pub ms_avg: Option<u32>,
    pub ms_max: Option<u32>,
}

/// How one bucket reads. A reading while its stack deployed is a known
/// outage (degraded, never down); under one lost reading in a hundred is
/// degraded too, so a single flaky probe does not paint a whole day red.
pub fn state_of(up: u32, dep: u32, down: u32) -> &'static str {
    let total = up + dep + down;
    if total == 0 {
        "none"
    } else if down == 0 && dep == 0 {
        "up"
    } else if down == 0 || f64::from(up + dep) / f64::from(total) >= 0.99 {
        "degraded"
    } else {
        "down"
    }
}

/// The last `hours` before `now`, cut into buckets of `bucket_s` seconds
/// (a multiple of an hour), the bucket edges shifted by `offset_s` (the
/// reader's UTC offset, so a day is the reader's day). Oldest first;
/// empty buckets are kept as "none" so a strip has a fixed width.
pub fn periods(cells: &[Cell], now: i64, hours: i64, bucket_s: i64, offset_s: i64) -> Vec<Period> {
    let hours = hours.clamp(1, KEEP_HOURS);
    let bucket_s = bucket_s.max(3600) / 3600 * 3600;
    let end = (now + offset_s).div_euclid(bucket_s) * bucket_s + bucket_s;
    let first = (now - hours * 3600 + offset_s).div_euclid(bucket_s) * bucket_s;
    let mut out = Vec::new();
    let mut b = first;
    while b < end {
        let (lo, hi) = (b - offset_s, b - offset_s + bucket_s);
        let (mut up, mut dep, mut down, mut sum, mut n, mut max) = (0, 0, 0, 0u64, 0u32, 0u32);
        for c in cells.iter().filter(|c| c.h * 3600 >= lo && c.h * 3600 < hi) {
            up += c.up;
            dep += c.dep;
            down += c.down;
            sum += c.ms_sum;
            n += c.ms_n;
            max = max.max(c.ms_max);
        }
        let total = up + dep + down;
        out.push(Period {
            start: lo,
            state: state_of(up, dep, down),
            uptime: (total > 0).then(|| f64::from(up) / f64::from(total)),
            up,
            dep,
            down,
            ms_avg: (n > 0).then(|| u32::try_from(sum / u64::from(n)).unwrap_or(u32::MAX)),
            ms_max: (n > 0).then_some(max),
        });
        b += bucket_s;
    }
    out
}

/// The share up over a whole answer, None without readings.
pub fn overall(p: &[Period]) -> Option<f64> {
    let (up, total) = p.iter().fold((0u64, 0u64), |(u, t), p| {
        (
            u + u64::from(p.up),
            t + u64::from(p.up) + u64::from(p.dep) + u64::from(p.down),
        )
    });
    (total > 0).then(|| up as f64 / total as f64)
}

/// The `/data/uptime` answer for every key.
pub fn view(
    store: &Store,
    now: i64,
    hours: i64,
    bucket_s: i64,
    offset_s: i64,
) -> serde_json::Value {
    let mut keys = serde_json::Map::new();
    for (k, cells) in &store.keys {
        let p = periods(cells, now, hours, bucket_s, offset_s);
        keys.insert(
            k.clone(),
            serde_json::json!({ "periods": p, "uptime": overall(&p) }),
        );
    }
    serde_json::json!({
        "keys": keys,
        "hours": hours.clamp(1, KEEP_HOURS),
        "bucket_s": bucket_s.max(3600) / 3600 * 3600,
        "kept_hours": KEEP_HOURS,
        "measured_at": now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_fold_into_the_hour() {
        let mut s = Store::default();
        s.record("a", 7200, Sample::Up);
        s.record("a", 7300, Sample::Down);
        s.record("a", 10_900, Sample::Up);
        let c = &s.keys["a"];
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].up, c[0].down), (1, 1));
        assert_eq!(c[1].h, 3);
    }

    #[test]
    fn age_and_key_count_are_capped() {
        let mut s = Store::default();
        s.record("old", 0, Sample::Up);
        let now = (KEEP_HOURS + 5) * 3600;
        s.record("new", now, Sample::Up);
        assert!(!s.keys.contains_key("old"));
        for i in 0..(MAX_KEYS + 20) {
            s.record(&format!("k{i}"), now + i as i64 * 3600, Sample::Up);
        }
        assert!(s.keys.len() <= MAX_KEYS);
    }

    #[test]
    fn a_day_with_one_lost_reading_is_degraded_not_down() {
        assert_eq!(state_of(500, 0, 1), "degraded");
        assert_eq!(state_of(10, 0, 10), "down");
        assert_eq!(state_of(10, 2, 0), "degraded");
        assert_eq!(state_of(0, 0, 0), "none");
    }

    #[test]
    fn periods_have_a_fixed_width_and_a_reader_day() {
        let mut s = Store::default();
        let now = 100 * 86_400 + 5 * 3600;
        s.record("a", now, Sample::Up);
        let p = periods(&s.keys["a"], now, 48, 86_400, 3600);
        assert_eq!(p.len(), 3);
        assert_eq!(p.last().map(|p| p.state), Some("up"));
        assert_eq!(p[0].state, "none");
        let hourly = periods(&s.keys["a"], now, 24, 3600, 0);
        assert_eq!(hourly.len(), 25);
    }

    #[test]
    fn pings_keep_average_and_slowest() {
        let mut s = Store::default();
        s.record_ping("ping:host", 10, Some(10));
        s.record_ping("ping:host", 20, Some(30));
        s.record_ping("ping:host", 30, None);
        let p = periods(&s.keys["ping:host"], 40, 1, 3600, 0);
        let last = p.last().unwrap();
        assert_eq!(
            (last.ms_avg, last.ms_max, last.down),
            (Some(20), Some(30), 1)
        );
    }
}
