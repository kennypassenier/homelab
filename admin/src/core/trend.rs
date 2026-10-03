//! redesign-stacks (3.71.0, Kenny approved the Stacks demo 2026-10-03): the
//! fleet's recent past, for the sparklines on the Stacks page — the host's
//! CPU and load in its KPI strip, and every stack's CPU in its row. The host
//! sends one snapshot of the fleet at a time; this keeps the last day of
//! them, one point per bucket (the mean of the snapshots that fell in it),
//! so a page opened now still draws the shape of the last 24 hours.
//!
//! Pure: the caller hands in every snapshot with its time.

use std::collections::{BTreeMap, VecDeque};

use super::fleet::FleetView;

/// Seconds one point covers.
pub const STEP_S: u64 = 300;
/// Points kept: one day of five-minute buckets.
pub const POINTS: usize = 288;

/// The running sums of one bucket.
#[derive(Debug, Clone, Default, PartialEq)]
struct Bucket {
    /// Which bucket: `at / step`.
    slot: u64,
    host_cpu: Mean,
    load_x100: Mean,
    /// Per stack: CPU in permille of one core's worth of the host.
    stacks: BTreeMap<String, Mean>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Mean {
    sum: u64,
    n: u64,
}

impl Mean {
    fn add(&mut self, v: u64) {
        self.sum += v;
        self.n += 1;
    }
    fn get(self) -> Option<u64> {
        (self.n > 0).then(|| (self.sum + self.n / 2) / self.n)
    }
}

/// The last day of the fleet, one point per [`STEP_S`].
#[derive(Debug, Clone, PartialEq)]
pub struct Trend {
    step_s: u64,
    cap: usize,
    buckets: VecDeque<Bucket>,
}

impl Default for Trend {
    fn default() -> Self {
        Trend::new(STEP_S, POINTS)
    }
}

impl Trend {
    /// A trend of `cap` points of `step_s` seconds each.
    pub fn new(step_s: u64, cap: usize) -> Self {
        Trend {
            step_s: step_s.max(1),
            cap: cap.max(2),
            buckets: VecDeque::new(),
        }
    }

    /// Add one snapshot, read at `at` (unix seconds). A snapshot older than
    /// the newest bucket is dropped: the host never sends one back in time,
    /// and a reconnect must not bend the line.
    pub fn record(&mut self, fleet: &FleetView, at: u64) {
        let slot = at / self.step_s;
        match self.buckets.back() {
            Some(b) if b.slot > slot => return,
            Some(b) if b.slot == slot => {}
            _ => {
                self.buckets.push_back(Bucket {
                    slot,
                    ..Bucket::default()
                });
                while self.buckets.len() > self.cap {
                    self.buckets.pop_front();
                }
            }
        }
        let Some(b) = self.buckets.back_mut() else {
            return;
        };
        if let Some(c) = fleet.host.cpu_pct {
            b.host_cpu.add(c);
        }
        b.load_x100.add(u64::from(fleet.host.load1_x100));
        for s in &fleet.stacks {
            if let Some(c) = s.cpu_permille {
                b.stacks
                    .entry(s.name.clone())
                    .or_default()
                    .add(u64::from(c));
            }
        }
    }

    /// The trend as the page reads it: `points` slots from `from` (the
    /// oldest kept, unix seconds), `step_s` apart; every series has one
    /// value per slot, `null` where nothing was measured (a gap, never a
    /// made-up zero). Stacks are those of the newest snapshot that named
    /// them anywhere in the window.
    pub fn view(&self) -> serde_json::Value {
        let (Some(first), Some(last)) = (self.buckets.front(), self.buckets.back()) else {
            return serde_json::json!({
                "step_s": self.step_s, "from": null, "points": 0,
                "host": { "cpu_pct": [], "load_x100": [] }, "stacks": {},
            });
        };
        let n = (last.slot - first.slot + 1) as usize;
        let index = |slot: u64| (slot - first.slot) as usize;
        let mut cpu = vec![None; n];
        let mut load = vec![None; n];
        let mut stacks: BTreeMap<&str, Vec<Option<u64>>> = BTreeMap::new();
        for b in &self.buckets {
            let i = index(b.slot);
            cpu[i] = b.host_cpu.get();
            load[i] = b.load_x100.get();
            for (name, m) in &b.stacks {
                stacks.entry(name.as_str()).or_insert_with(|| vec![None; n])[i] = m.get();
            }
        }
        serde_json::json!({
            "step_s": self.step_s,
            "from": first.slot * self.step_s,
            "points": n,
            "host": { "cpu_pct": cpu, "load_x100": load },
            "stacks": stacks,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::fleet::{Counts, HostSummary, StackSummary};

    fn fleet(cpu: Option<u64>, load: u32, stacks: &[(&str, Option<u32>)]) -> FleetView {
        FleetView {
            measured_at: 0,
            host: HostSummary {
                name: "h".into(),
                cpu_pct: cpu,
                ram_used_mb: 0,
                ram_total_mb: 0,
                disk_pct: 0,
                ram_committed_mb: 0,
                cores_total: 1,
                load1_x100: load,
                tls_fingerprint: String::new(),
                disk_detail: None,
                uptime_s: None,
                release: None,
                guests_usage: None,
            },
            stacks: stacks
                .iter()
                .map(|(n, c)| StackSummary {
                    name: (*n).into(),
                    vmid: 1,
                    online: true,
                    enabled: true,
                    apps_running: 0,
                    apps_total: 0,
                    restarts: 0,
                    ram_used_mb: None,
                    ram_max_mb: None,
                    cpu_permille: *c,
                    hostname: String::new(),
                    apps: Vec::new(),
                    uptime_s: None,
                    applied_source: None,
                    env_sealed: true,
                    native: false,
                    applied_hash: String::new(),
                    component_digests: Default::default(),
                })
                .collect(),
            counts: Counts {
                stacks: stacks.len(),
                online: stacks.len(),
                parked: 0,
            },
        }
    }

    #[test]
    fn redesign_stacks_a_bucket_holds_the_mean_of_its_snapshots() {
        let mut t = Trend::new(300, 10);
        t.record(&fleet(Some(10), 100, &[("a", Some(20))]), 600);
        t.record(&fleet(Some(20), 300, &[("a", Some(40))]), 899);
        let v = t.view();
        assert_eq!(v["points"], 1);
        assert_eq!(v["from"], 600);
        assert_eq!(v["host"]["cpu_pct"], serde_json::json!([15]));
        assert_eq!(v["host"]["load_x100"], serde_json::json!([200]));
        assert_eq!(v["stacks"]["a"], serde_json::json!([30]));
    }

    #[test]
    fn redesign_stacks_a_missed_bucket_is_a_gap_never_a_zero() {
        let mut t = Trend::new(300, 10);
        t.record(&fleet(Some(10), 100, &[("a", Some(20))]), 0);
        t.record(&fleet(None, 100, &[("a", Some(40)), ("b", Some(5))]), 900);
        let v = t.view();
        assert_eq!(v["points"], 4);
        assert_eq!(
            v["host"]["cpu_pct"],
            serde_json::json!([10, null, null, null])
        );
        assert_eq!(v["stacks"]["a"], serde_json::json!([20, null, null, 40]));
        assert_eq!(v["stacks"]["b"], serde_json::json!([null, null, null, 5]));
    }

    #[test]
    fn redesign_stacks_the_trend_keeps_only_its_last_day_and_never_goes_back() {
        let mut t = Trend::new(300, 3);
        for i in 0..5 {
            t.record(&fleet(Some(i), 0, &[]), i * 300);
        }
        // Older than the newest bucket: dropped.
        t.record(&fleet(Some(99), 0, &[]), 0);
        let v = t.view();
        assert_eq!(v["from"], 600);
        assert_eq!(v["host"]["cpu_pct"], serde_json::json!([2, 3, 4]));
    }

    #[test]
    fn redesign_stacks_an_empty_trend_says_so() {
        let v = Trend::default().view();
        assert_eq!(v["points"], 0);
        assert_eq!(v["step_s"], STEP_S);
    }
}
