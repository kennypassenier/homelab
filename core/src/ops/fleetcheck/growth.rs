//! Container growth: rootfs, memory, swap and log sizes against the limits.

use super::{Finding, Severity};

/// One container's resource picture, as read off the machine.
///
/// G3 exists because of what G1 found: the guards that cap logs had been
/// written months earlier and ran on almost nothing, and Loki had quietly
/// written 923 MB of its own output. Nothing was watching. Kenny's bar is
/// that a container should be able to run for a hundred years — which is
/// only meaningful if something notices when it starts trending otherwise.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GrowthFact {
    pub vmid: u16,
    pub hostname: String,
    /// Percentage of the container's rootfs in use.
    pub disk_used_pct: u8,
    /// Percentage of the container's memory allocation in use.
    pub mem_used_pct: u8,
    /// Swap actually in use, in MB.
    pub swap_used_mb: u32,
    /// Size of the systemd journal, in MB.
    pub journal_mb: u32,
    /// Size of docker's container log directory, in MB.
    pub docker_logs_mb: u32,
    /// Whether the runaway guards are installed *now* — a journald cap and
    /// a docker log cap present on disk. Not whether they were ever applied:
    /// that is exactly the distinction that let five containers run without
    /// them while the code that writes them had existed all along.
    pub guards: bool,
}

/// Where "growing" turns into "worth telling Kenny about".
///
/// Standing rule 27: a number expressing tolerance belongs in configuration,
/// and these are its defaults. They are deliberately far below the point of
/// failure — the whole idea is to see the trend, not the wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrowthLimits {
    /// Rootfs full enough that the next update can fail.
    pub disk_broken_pct: u8,
    /// Rootfs trending toward that.
    pub disk_drift_pct: u8,
    /// Memory this close to the allocation will start swapping.
    pub mem_drift_pct: u8,
    /// Any swap beyond this means pressure is being hidden rather than felt.
    pub swap_drift_mb: u32,
    /// A journal past this is not being capped effectively.
    pub journal_mb: u32,
    /// Docker logs past this are not being rotated effectively.
    pub docker_logs_mb: u32,
    /// A data pool this full is one where the next import can fail.
    pub pool_broken_pct: u8,
    /// A data pool trending toward that.
    pub pool_drift_pct: u8,
}

impl Default for GrowthLimits {
    fn default() -> Self {
        Self {
            // 85% of a 32 GB rootfs still leaves 4.8 GB, which is one image
            // pull; below that an update can fail halfway.
            disk_broken_pct: 85,
            disk_drift_pct: 70,
            mem_drift_pct: 90,
            // Measured on this fleet: a healthy container sits at 0. CT 106
            // sat at 1028 MB, which is what prompted G2.
            swap_drift_mb: 64,
            // The guards cap journald at 100 MB; 150 means the cap is absent
            // or not taking effect.
            journal_mb: 150,
            // 10m x 3 files per container: 250 MB is roughly eight busy
            // containers' worth, or one that is not being rotated.
            docker_logs_mb: 250,
            // R13: nothing in this suite looked at the pools the libraries
            // actually live on — `disk_*_pct` above is the container's own
            // rootfs, which on CT 106 is 80 GB while the films sit on 16.4 TB
            // of ZFS beside it.
            //
            // HIGHER than the rootfs thresholds, and the first draft had that
            // backwards on the reasoning that freeing a media pool takes days
            // rather than seconds — true, and served by warning at drift long
            // before broken, not by a lower percentage. At the rootfs's 70%
            // this pool would report 4.9 TB free as a problem.
            //
            // A percentage rather than "warn under 2 TB", which is what Kenny
            // asked for in R13 and is the wrong shape once measured: HDD2TB
            // holds 1798 GB free while being 1% used, so an absolute rule
            // would open with a finding about a pool that is empty. The free
            // space he wants to see is in the message instead, beside the
            // percentage. Measured 2026-09-04: 30% / 79% / 1% / 1% across the
            // four declared pools, so this opens silent and HDD12TB is the
            // one to cross first.
            pool_broken_pct: 90,
            pool_drift_pct: 80,
        }
    }
}

/// G3: the growth half of the check, split out so it can be tested on its
/// own and reused by anything that has the facts.
///
/// Every finding here is Drift rather than Broken except a nearly-full disk,
/// because that is the honest reading: nothing is failing yet. The point is
/// to see it while it is still cheap.
/// F184: `host_short` is why the host itself cannot help — Some when the
/// machine is oversubscribed or already swapping hard. Passed in rather than
/// read here, because a per-guest judgement that silently reads global state
/// is a judgement nobody can test.
pub fn evaluate_growth(
    facts: &[GrowthFact],
    lim: GrowthLimits,
    host_short: Option<&str>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    for g in facts {
        let who = format!("{} (CT {})", g.hostname, g.vmid);
        out.extend(disk_finding(g, &who, lim));
        out.extend(memory_finding(g, &who, lim));
        out.extend(swap_finding(g, &who, lim, host_short));
        out.extend(journal_finding(g, &who, lim));
        out.extend(docker_logs_finding(g, &who, lim));
        out.extend(guards_finding(g, who));
    }
    out
}

fn disk_finding(g: &GrowthFact, who: &str, lim: GrowthLimits) -> Option<Finding> {
    let (severity, remedy) = if g.disk_used_pct >= lim.disk_broken_pct {
        (
            Severity::Broken,
            "an image pull or an apt upgrade can fail halfway from here — free space or grow the disk with `pct resize`",
        )
    } else if g.disk_used_pct >= lim.disk_drift_pct {
        (
            Severity::Drift,
            "still fine, but find out what is growing before it is urgent — `du -xh --max-depth=2 /` inside the container",
        )
    } else {
        return None;
    };
    Some(Finding {
        severity,
        subject: who.to_string(),
        what: format!("rootfs is {}% full", g.disk_used_pct),
        remedy: remedy.into(),
    })
}

fn memory_finding(g: &GrowthFact, who: &str, lim: GrowthLimits) -> Option<Finding> {
    (g.mem_used_pct >= lim.mem_drift_pct).then(|| Finding {
        severity: Severity::Drift,
        subject: who.to_string(),
        what: format!("using {}% of its memory allocation", g.mem_used_pct),
        remedy: "raise memory_mb in the stack manifest and redeploy; the alternative is swapping, which hides the pressure instead of reporting it".into(),
    })
}

fn swap_finding(
    g: &GrowthFact,
    who: &str,
    lim: GrowthLimits,
    host_short: Option<&str>,
) -> Option<Finding> {
    (g.swap_used_mb >= lim.swap_drift_mb).then(|| Finding {
        severity: Severity::Drift,
        subject: who.to_string(),
        what: format!("{} MB of swap in use", g.swap_used_mb),
        // F184: the old remedy said "give the container more memory"
        // unconditionally. On 2026-09-02 eight containers reported
        // this at once on a host with 31 GB of RAM, 47 GB committed
        // to guests, and 7 of its 8 GB of swap already used. Following
        // that advice eight times would have made the machine worse,
        // and the check would have kept saying it. A remedy that
        // cannot be carried out is not a remedy.
        remedy: host_short
            .map(|why| {
                format!(
                    "the container is not the problem: {} — reduce what is promised \
                             to guests, or give the host more RAM. Raising this container's \
                             memory takes it from another one",
                    why
                )
            })
            .unwrap_or_else(|| {
                "swap turns memory pressure into slow degradation instead of a loud \
                         failure — give the container more memory rather than more swap"
                    .into()
            }),
    })
}

fn journal_finding(g: &GrowthFact, who: &str, lim: GrowthLimits) -> Option<Finding> {
    (g.journal_mb >= lim.journal_mb).then(|| Finding {
        severity: Severity::Drift,
        subject: who.to_string(),
        what: format!("journal is {} MB", g.journal_mb),
        remedy: format!("the guards cap it well below this — run `homelab guards {}` and check the cap took effect", g.vmid),
    })
}

fn docker_logs_finding(g: &GrowthFact, who: &str, lim: GrowthLimits) -> Option<Finding> {
    (g.docker_logs_mb >= lim.docker_logs_mb).then(|| Finding {
        severity: Severity::Drift,
        subject: who.to_string(),
        what: format!("docker container logs total {} MB", g.docker_logs_mb),
        remedy: format!("the cap only applies to containers created after it was set — run `homelab guards {}`, then recreate the noisiest container so it picks the cap up", g.vmid),
    })
}

fn guards_finding(g: &GrowthFact, who: String) -> Option<Finding> {
    (!g.guards).then(|| Finding {
        severity: Severity::Drift,
        subject: who,
        what: "has no runaway guards: no journald cap, no docker log cap, or both".into(),
        remedy: format!("`homelab guards {}` — without them nothing bounds log growth, which is how one service reached 923 MB unnoticed", g.vmid),
    })
}
