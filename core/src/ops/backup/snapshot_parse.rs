//! Reading `restic snapshots --json` into plain values.

use super::trigger::BackupTrigger;

/// Parse `restic snapshots --json` into `(short_id, unix_time)` pairs.
/// Tolerant of extra fields; returns empty on malformed input (retention
/// then keeps everything — fail-safe direction).
pub(crate) fn parse_snapshots_json(raw: &str) -> Vec<(String, u64)> {
    #[derive(serde::Deserialize)]
    struct Snap {
        short_id: String,
        time: String,
    }
    let Ok(snaps) = serde_json::from_str::<Vec<Snap>>(raw.trim()) else {
        return Vec::new();
    };
    snaps
        .into_iter()
        .filter_map(|s| {
            // RFC3339 → unix without pulling in chrono: date parsing via the
            // subset restic emits (e.g. 2026-08-11T04:00:12.123+02:00).
            humantime_to_unix(&s.time).map(|t| (s.short_id, t))
        })
        .collect()
}

/// fix-238: `(short_id, unix_time, scheduled)` per snapshot. Scheduled means
/// a `trigger:nightly` tag or no trigger tag at all (snapshots from before
/// fix-223 were all nightly runs); `trigger:manual` and `trigger:pre-destroy`
/// are on demand. Malformed input gives an empty list, so retention keeps
/// everything, the same fail-safe direction as [`parse_snapshots_json`].
pub(crate) fn parse_snapshot_lanes(raw: &str) -> Vec<(String, u64, bool)> {
    #[derive(serde::Deserialize)]
    struct Snap {
        short_id: String,
        time: String,
        #[serde(default)]
        tags: Vec<String>,
    }
    let Ok(snaps) = serde_json::from_str::<Vec<Snap>>(raw.trim()) else {
        return Vec::new();
    };
    snaps
        .into_iter()
        .filter_map(|s| {
            let scheduled = s
                .tags
                .iter()
                .find_map(|t| BackupTrigger::parse_tag(t))
                .is_none_or(|k| k == BackupTrigger::Nightly);
            humantime_to_unix(&s.time).map(|t| (s.short_id, t, scheduled))
        })
        .collect()
}

/// fix-112: one snapshot as a restore sees it — its ids, its time, and the
/// night it belongs to when it carries a `run-<unix>` tag.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapRun {
    pub id: String,
    pub short_id: String,
    pub time: u64,
    pub run: Option<u64>,
    /// fix-223: restic's own `summary.total_bytes_processed` (restic ≥
    /// 0.17 includes a `summary` object per snapshot in `snapshots --json`;
    /// verified against restic 0.19.1). `None` for a snapshot an older
    /// restic took, or one restic's own JSON never gave a summary for — the
    /// picker shows "size not recorded" rather than guessing. This is NOT a
    /// second `restic stats` round trip: it is read from the same
    /// `snapshots --json` call `list_snapshots` already makes.
    pub size_bytes: Option<u64>,
    /// fix-223: restic's own `summary.total_files_processed`, same
    /// availability rule as `size_bytes`.
    pub file_count: Option<u64>,
    /// fix-223: the `trigger:<kind>` tag's kind (`"nightly"`, `"manual"`,
    /// `"pre-destroy"`), parsed back from the tags array. `None` for an
    /// older snapshot taken before this existed, or an unrecognised tag
    /// value — the picker shows "kind not recorded" rather than guessing.
    pub trigger: Option<String>,
}

/// fix-112: `restic snapshots --json`, with the night tag. Malformed input
/// gives an empty list, which the resolution below refuses.
pub fn parse_snapshot_runs(raw: &str) -> Vec<SnapRun> {
    let Ok(snaps) = serde_json::from_str::<Vec<RawRunSnap>>(raw.trim()) else {
        return Vec::new();
    };
    snaps.into_iter().filter_map(RawRunSnap::into_run).collect()
}

#[derive(serde::Deserialize)]
struct RawSummary {
    #[serde(default)]
    total_files_processed: Option<u64>,
    #[serde(default)]
    total_bytes_processed: Option<u64>,
}

#[derive(serde::Deserialize)]
struct RawRunSnap {
    id: String,
    short_id: String,
    time: String,
    #[serde(default)]
    tags: Option<Vec<String>>,
    // fix-223: restic ≥ 0.17 only; absent entirely on an older restic
    // or an older snapshot, which is exactly "not recorded".
    #[serde(default)]
    summary: Option<RawSummary>,
}

impl RawRunSnap {
    fn into_run(self) -> Option<SnapRun> {
        let time = humantime_to_unix(&self.time)?;
        let tags = self.tags.unwrap_or_default();
        let run = tags
            .iter()
            .find_map(|t| t.strip_prefix("run-")?.parse::<u64>().ok());
        let trigger = tags
            .iter()
            .find_map(|t| BackupTrigger::parse_tag(t))
            .map(|t| t.label().to_string());
        let (size_bytes, file_count) = match self.summary {
            Some(sum) => (sum.total_bytes_processed, sum.total_files_processed),
            None => (None, None),
        };
        Some(SnapRun {
            id: self.id,
            short_id: self.short_id,
            time,
            run,
            size_bytes,
            file_count,
            trigger,
        })
    }
}

/// Minimal RFC3339 → unix seconds (UTC), no external crates. Handles the
/// forms restic emits; returns None on anything unexpected.
pub(super) fn humantime_to_unix(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Timezone offset: trailing Z or ±HH:MM after the (optional) fraction.
    let rest = &s[19..];
    let offset_secs: i64 = if rest.ends_with('Z') || rest.is_empty() {
        0
    } else if let Some(pos) = rest.rfind(['+', '-']) {
        let sign = if rest.as_bytes()[pos] == b'+' { 1 } else { -1 };
        let tz = &rest[pos + 1..];
        let th = tz.get(0..2)?.parse::<i64>().ok()?;
        let tm = tz.get(3..5)?.parse::<i64>().ok()?;
        sign * (th * 3600 + tm * 60)
    } else {
        0
    };
    // Days since epoch (civil-from-days algorithm, Howard Hinnant).
    let (y, mo) = if mo <= 2 { (y - 1, mo + 12) } else { (y, mo) };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (mo - 3) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let unix = days * 86_400 + h * 3600 + mi * 60 + sec - offset_secs;
    u64::try_from(unix).ok()
}
