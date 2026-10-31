//! How full the pools are that stacks keep their data on.

use super::{Finding, GrowthLimits, Severity};

/// R13 · how full is a pool a stack keeps its data on.
///
/// It exists because the answer used to be nobody's business. Every disk
/// number this suite reads is a container's own rootfs; the libraries live on
/// ZFS pools bind-mounted in beside it, and those were measured by no check at
/// all. That was tolerable while the profiles said "any 1080p file is done".
/// It stopped being tolerable when Recyclarr started replacing 27 MB/min
/// releases with ones near Kenny's 95 MB/min preference (R13): the same 943
/// films go from 4.1 TB to roughly 10 TB, and the only thing standing between
/// that and a full pool was somebody happening to look.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PoolFact {
    /// The filesystem, as `df` names it — the key, because two stacks that
    /// declare different paths on the same pool are one pool, not two.
    pub filesystem: String,
    /// A declared path that lives on it, for a message that names something
    /// Kenny recognises rather than a dataset name.
    pub path: String,
    /// Which stacks keep data there.
    pub stacks: Vec<String>,
    pub used_pct: u8,
    pub free_gb: u64,
}

/// R13: turn one `df` transcript into pool facts.
///
/// Pure, and in core rather than beside the command that produces it, because
/// two things about it are easy to get wrong and impossible to see once wrong:
///
/// * **The path has to come from the line, not from the position.** A path
///   that does not exist produces no `df` row at all, so reading rows
///   positionally shifts every later one up and reports one stack's pool under
///   another stack's name — with entirely plausible numbers. The caller
///   prints the path it asked about at the start of each line for exactly this
///   reason.
/// * **A path can be declared by several stacks.** CT 105 and CT 106 both
///   mount `/HDD18TB/subvol-103-disk-0`. Taking the first declaration that
///   matches names one of them and silently drops the other, which is how a
///   finding sends somebody to the wrong container.
///
/// Line shape, as the caller emits it: `<path> <fs> <1k-blocks> <used>
/// <avail> <pct> <mountpoint>`. Anything shorter is a path `df` could not
/// answer for — a missing mount, which is a different check's business.
pub fn pool_facts_from_df(transcript: &str, declared: &[(String, String)]) -> Vec<PoolFact> {
    let mut by_fs: std::collections::BTreeMap<String, PoolFact> = std::collections::BTreeMap::new();
    for line in transcript.lines() {
        let c: Vec<&str> = line.split_whitespace().collect();
        let Some((owners, fact)) = pool_row(&c, declared) else {
            continue;
        };
        let e = by_fs.entry(fact.filesystem.clone()).or_insert(fact);
        for owner in owners {
            if !e.stacks.contains(owner) {
                e.stacks.push(owner.clone());
            }
        }
    }
    for f in by_fs.values_mut() {
        f.stacks.sort();
    }
    by_fs.into_values().collect()
}

/// One `df` row as the pool it measures and the stacks declaring its path;
/// None for a short row, an unreadable one, an empty pool or an undeclared
/// path.
fn pool_row<'a>(
    c: &[&str],
    declared: &'a [(String, String)],
) -> Option<(Vec<&'a String>, PoolFact)> {
    if c.len() < 7 {
        return None;
    }
    let path = c[0];
    let (Ok(used), Ok(avail)) = (c[3].parse::<u64>(), c[4].parse::<u64>()) else {
        return None;
    };
    let total = used + avail;
    if total == 0 {
        return None;
    }
    let owners: Vec<&String> = declared
        .iter()
        .filter(|(p, _)| p == path)
        .map(|(_, s)| s)
        .collect();
    if owners.is_empty() {
        return None;
    }
    let fact = PoolFact {
        filesystem: c[1].to_string(),
        path: path.to_string(),
        used_pct: (used * 100 / total) as u8,
        free_gb: avail / 1024 / 1024,
        ..Default::default()
    };
    Some((owners, fact))
}

/// R13: a finding per pool that is filling up. Deduplicated by filesystem by
/// the caller — this only judges.
pub fn evaluate_pools(facts: &[PoolFact], lim: GrowthLimits) -> Vec<Finding> {
    let mut out = Vec::new();
    for p in facts {
        let who = if p.stacks.is_empty() {
            p.path.clone()
        } else {
            format!("{} ({})", p.path, p.stacks.join(", "))
        };
        // Free space is in the message on purpose, next to the percentage.
        // A percentage scales across a 2 TB pool and an 18 TB one; "1.6 TB
        // left" is the number somebody actually reasons with.
        let size = format!("{}% full, {} GB free", p.used_pct, p.free_gb);
        if p.used_pct >= lim.pool_broken_pct {
            out.push(Finding {
                severity: Severity::Broken,
                subject: who,
                what: format!("data pool {}", size),
                remedy: "an import lands here and there is no room for it — free space, or \
                         stop the quality upgrades that are filling it"
                    .into(),
            });
        } else if p.used_pct >= lim.pool_drift_pct {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who,
                what: format!("data pool {}", size),
                remedy: "still fine, and worth knowing which way it is going before it is \
                         urgent — `zfs list -o name,used,avail` on the host"
                    .into(),
            });
        }
    }
    out
}
