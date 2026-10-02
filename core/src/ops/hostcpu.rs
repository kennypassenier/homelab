//! fix-175: the host's own CPU busy share, measured the same way the
//! per-guest `cpu_permille` is — a reading on a timer — but from
//! `/proc/stat` instead of `pvesh`, since the daemon runs on pve itself.
//!
//! `/proc/stat`'s `cpu ` line is a running counter since boot, not an
//! instantaneous percentage, so one reading says nothing: the busy share
//! only shows up as the delta between two readings taken `status_interval_s`
//! apart. The host keeps the previous reading in `AppState` and this module
//! does the pure, testable arithmetic on the pair.

use serde::{Deserialize, Serialize};

/// One `/proc/stat` `cpu ` line, summed into the two numbers the delta needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuSnapshot {
    /// Sum of every field on the `cpu ` line (user+nice+system+idle+iowait+
    /// irq+softirq+steal[+guest+guest_nice]), in USER_HZ ticks since boot.
    pub total: u64,
    /// idle + iowait, the two fields that are not "busy".
    pub idle: u64,
}

/// Parse the `cpu ` (aggregate, not `cpu0`/`cpu1`/…) line of `/proc/stat`.
/// `None` on anything that is not the shape `/proc/stat` always has — a
/// kernel that changed its format, or a probe that returned garbage, is
/// "unknown", never a fabricated number.
pub fn parse_proc_stat(text: &str) -> Option<CpuSnapshot> {
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let fields: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|f| f.parse::<u64>().ok())
        .collect();
    // user nice system idle iowait [irq softirq steal guest guest_nice] — at
    // least the first four are documented back to Linux 2.4; fewer than that
    // is not a `/proc/stat` this can read.
    if fields.len() < 4 {
        return None;
    }
    let idle = fields[3] + fields.get(4).copied().unwrap_or(0);
    let total: u64 = fields.iter().sum();
    Some(CpuSnapshot { total, idle })
}

/// Busy percent (0-100) between two snapshots of the same host, or `None`
/// when the pair cannot be trusted: the daemon restarted and `/proc/stat`
/// reset (`curr.total <= prev.total`), or either reading is internally
/// inconsistent (`idle > total`, which a genuine `/proc/stat` never is). The
/// first poll after start has no `prev` at all, which the caller handles by
/// not calling this yet — that is "unknown" too, just one level up.
pub fn cpu_pct(prev: &CpuSnapshot, curr: &CpuSnapshot) -> Option<u32> {
    if prev.idle > prev.total || curr.idle > curr.total {
        return None;
    }
    let total_delta = curr.total.checked_sub(prev.total)?;
    if total_delta == 0 {
        return None;
    }
    let idle_delta = curr.idle.checked_sub(prev.idle)?;
    if idle_delta > total_delta {
        return None;
    }
    let busy_delta = total_delta - idle_delta;
    Some(((busy_delta as u128 * 100) / total_delta as u128) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE1: &str = "cpu  100 0 100 800 0 0 0 0 0 0\nintr 0\n";

    #[test]
    fn fix_175_parses_the_aggregate_cpu_line() {
        let s = parse_proc_stat(LINE1).unwrap();
        // total = 100+0+100+800 = 1000, idle = 800
        assert_eq!(s.total, 1000);
        assert_eq!(s.idle, 800);
    }

    #[test]
    fn fix_175_ignores_per_core_lines_and_reads_only_the_aggregate() {
        let text = "cpu0 1 2 3 4\ncpu  10 0 10 80 0 0 0 0 0 0\ncpu1 1 2 3 4\n";
        let s = parse_proc_stat(text).unwrap();
        assert_eq!(s.total, 100);
    }

    #[test]
    fn fix_175_missing_cpu_line_is_unknown() {
        assert!(parse_proc_stat("intr 0\nctxt 1\n").is_none());
    }

    #[test]
    fn fix_175_too_few_fields_is_unknown() {
        assert!(parse_proc_stat("cpu  1 2\n").is_none());
    }

    #[test]
    fn fix_175_two_snapshots_give_the_busy_percent() {
        // 25% idle over the interval: busy delta 300, total delta 400.
        let prev = CpuSnapshot {
            total: 1000,
            idle: 800,
        };
        let curr = CpuSnapshot {
            total: 1400,
            idle: 900,
        };
        assert_eq!(cpu_pct(&prev, &curr), Some(75));
    }

    #[test]
    fn fix_175_fully_idle_interval_is_zero_percent() {
        let prev = CpuSnapshot {
            total: 1000,
            idle: 800,
        };
        let curr = CpuSnapshot {
            total: 1200,
            idle: 1000,
        };
        assert_eq!(cpu_pct(&prev, &curr), Some(0));
    }

    #[test]
    fn fix_175_fully_busy_interval_is_a_hundred_percent() {
        let prev = CpuSnapshot {
            total: 1000,
            idle: 800,
        };
        let curr = CpuSnapshot {
            total: 1200,
            idle: 800,
        };
        assert_eq!(cpu_pct(&prev, &curr), Some(100));
    }

    /// Counter wrap / a daemon restart: `/proc/stat` is a since-boot
    /// counter, so a smaller `curr.total` than `prev.total` means the
    /// readings are not comparable at all, not that CPU went negative.
    #[test]
    fn fix_175_counter_that_went_backwards_is_unknown() {
        let prev = CpuSnapshot {
            total: 2000,
            idle: 1000,
        };
        let curr = CpuSnapshot {
            total: 1000,
            idle: 500,
        };
        assert_eq!(cpu_pct(&prev, &curr), None);
    }

    #[test]
    fn fix_175_no_time_elapsed_is_unknown_not_zero() {
        let s = CpuSnapshot {
            total: 1000,
            idle: 800,
        };
        assert_eq!(cpu_pct(&s, &s), None);
    }

    /// Garbage: idle grew by more than total did, which a real kernel never
    /// reports. Must not silently clamp or panic on the subtraction.
    #[test]
    fn fix_175_garbage_idle_delta_exceeding_total_delta_is_unknown() {
        let prev = CpuSnapshot {
            total: 1000,
            idle: 800,
        };
        let curr = CpuSnapshot {
            total: 1100,
            idle: 950,
        };
        assert_eq!(cpu_pct(&prev, &curr), None);
    }

    /// Garbage: a snapshot whose own idle exceeds its own total (should
    /// never come out of `parse_proc_stat`, but `cpu_pct` does not trust its
    /// inputs either).
    #[test]
    fn fix_175_internally_inconsistent_snapshot_is_unknown() {
        let prev = CpuSnapshot {
            total: 100,
            idle: 200,
        };
        let curr = CpuSnapshot {
            total: 200,
            idle: 250,
        };
        assert_eq!(cpu_pct(&prev, &curr), None);
    }
}
