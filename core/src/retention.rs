//! Tiered snapshot retention (G8). Restic's own `forget` only knows
//! daily/weekly/monthly buckets; Kenny wants arbitrary tiers ("daily for a
//! week, then every 14 days for 2 months, then every 60 days forever"), so
//! we compute the keep-set ourselves and hand restic an explicit forget list.
//!
//! Pure logic — no I/O, fully unit-tested (AR1).

use serde::{Deserialize, Serialize};

/// One tier: within its age window, keep one snapshot per `every_days`.
/// Tiers are ordered newest-first; each starts where the previous ended.
/// `span_days: None` = this tier extends forever (must be the last).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionTier {
    pub every_days: u32,
    #[serde(default)]
    pub span_days: Option<u32>,
}

/// Kenny's default: daily for a week, every 2 weeks for 2 months, then
/// every 2 months forever.
pub fn default_tiers() -> Vec<RetentionTier> {
    vec![
        RetentionTier {
            every_days: 1,
            span_days: Some(7),
        },
        RetentionTier {
            every_days: 14,
            span_days: Some(60),
        },
        RetentionTier {
            every_days: 60,
            span_days: None,
        },
    ]
}

const DAY: u64 = 86_400;

/// Given `(id, unix_time)` snapshots and the tier policy, return the ids to
/// FORGET. The newest snapshot is always kept, whatever the policy says.
///
/// Bucketing: each tier's age window is divided into `every_days` buckets
/// (aligned to `now`); the newest snapshot in each bucket is kept. Snapshots
/// older than the last bounded tier fall into the unbounded tier if present,
/// otherwise they are forgotten.
pub fn forget_list(snapshots: &[(String, u64)], tiers: &[RetentionTier], now: u64) -> Vec<String> {
    if snapshots.is_empty() {
        return Vec::new();
    }
    let newest_id = snapshots
        .iter()
        .max_by_key(|(_, t)| *t)
        .map(|(id, _)| id.clone())
        .unwrap();

    // Precompute tier windows as [start_age, end_age) in days.
    let mut windows: Vec<(u64, Option<u64>, u64)> = Vec::new(); // (start, end, every)
    let mut cursor = 0u64;
    for tier in tiers {
        let every = tier.every_days.max(1) as u64;
        match tier.span_days {
            Some(span) => {
                windows.push((cursor, Some(cursor + span as u64), every));
                cursor += span as u64;
            }
            None => {
                windows.push((cursor, None, every));
                break; // unbounded tier swallows the rest
            }
        }
    }

    use std::collections::HashMap;
    // (window index, bucket index) -> (id, time) of newest snapshot seen.
    let mut keep: HashMap<(usize, u64), (String, u64)> = HashMap::new();
    let mut forget: Vec<String> = Vec::new();

    for (id, t) in snapshots {
        let age_days = now.saturating_sub(*t) / DAY;
        let mut placed = false;
        for (wi, (start, end, every)) in windows.iter().enumerate() {
            let in_window = age_days >= *start && end.is_none_or(|e| age_days < e);
            if !in_window {
                continue;
            }
            let bucket = (age_days - start) / every;
            match keep.get_mut(&(wi, bucket)) {
                Some(existing) => {
                    // Keep the newest in the bucket; the other is forgotten.
                    if *t > existing.1 {
                        forget.push(existing.0.clone());
                        *existing = (id.clone(), *t);
                    } else {
                        forget.push(id.clone());
                    }
                }
                None => {
                    keep.insert((wi, bucket), (id.clone(), *t));
                }
            }
            placed = true;
            break;
        }
        if !placed {
            // Older than every bounded tier and no unbounded tier.
            forget.push(id.clone());
        }
    }

    // The newest snapshot is sacred.
    forget.retain(|id| *id != newest_id);
    forget
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snaps(days_ago: &[u64], now: u64) -> Vec<(String, u64)> {
        days_ago
            .iter()
            .enumerate()
            .map(|(i, d)| (format!("s{}", i), now - d * DAY))
            .collect()
    }

    #[test]
    fn keeps_all_dailies_within_first_tier() {
        let now = 1_800_000_000;
        let s = snaps(&[0, 1, 2, 3, 4, 5, 6], now);
        let forget = forget_list(&s, &default_tiers(), now);
        assert!(
            forget.is_empty(),
            "daily snapshots within 7d all kept: {:?}",
            forget
        );
    }

    #[test]
    fn thins_second_tier_to_one_per_14_days() {
        let now = 1_800_000_000;
        // Ages 8..=60: daily snapshots — tier 2 keeps ~one per 14-day bucket.
        let ages: Vec<u64> = (8..=60).collect();
        let s = snaps(&ages, now);
        let forget = forget_list(&s, &default_tiers(), now);
        let kept = s.len() - forget.len();
        // Window [7,67) has buckets at 7,21,35,49,63 → ages 8-60 span 4 buckets.
        assert!((3..=5).contains(&kept), "kept {} of {}", kept, s.len());
    }

    #[test]
    fn unbounded_tier_keeps_sparse_history_forever() {
        let now = 1_800_000_000;
        // Very old snapshots, ~60 days apart: all in distinct buckets → kept.
        let s = snaps(&[100, 160, 220, 280], now);
        let forget = forget_list(&s, &default_tiers(), now);
        assert!(forget.is_empty(), "sparse old history kept: {:?}", forget);
    }

    #[test]
    fn dense_old_history_is_thinned() {
        let now = 1_800_000_000;
        // 10 snapshots within one 60-day bucket → only newest survives.
        let ages: Vec<u64> = (100..110).collect();
        let s = snaps(&ages, now);
        let forget = forget_list(&s, &default_tiers(), now);
        assert_eq!(forget.len(), 9);
        // The newest of the group (age 100 = s0) is the survivor.
        assert!(!forget.contains(&"s0".to_string()));
    }

    #[test]
    fn newest_snapshot_is_always_kept() {
        let now = 1_800_000_000;
        // Policy that would forget everything old: single tiny bounded tier.
        let tiers = vec![RetentionTier {
            every_days: 1,
            span_days: Some(1),
        }];
        let s = snaps(&[50], now);
        let forget = forget_list(&s, &tiers, now);
        assert!(
            forget.is_empty(),
            "sole/newest snapshot survives any policy"
        );
    }

    #[test]
    fn empty_input_is_fine() {
        assert!(forget_list(&[], &default_tiers(), 1_800_000_000).is_empty());
    }
}
