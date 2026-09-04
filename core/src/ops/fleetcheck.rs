//! Y4: hold the repository against reality and report every difference.
//!
//! Everything found by hand on 2026-08-30 had one thing in common: none of it
//! was a failure. kyu's stack record still described a container that had been
//! renamed weeks earlier, so its nightly run failed the hostname guard and the
//! stack quietly auto-disabled. A settings key in `host.toml` replaced the
//! compiled no-touch list rather than adding to it, so a code change had no
//! effect. Three stack files claimed vmids that live containers were using. A
//! Traefik route pointed at an empty container and another at a workstation.
//! Uptime Kuma watched exactly one target. Every one of those looked healthy,
//! and the only reason any of them surfaced is that somebody spent a day
//! looking.
//!
//! This module makes the looking a function. The comparison is pure so it can
//! be exercised without a fleet; gathering the facts is the shell's job.

use serde::{Deserialize, Serialize};

use crate::state::HostState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Something is not doing its job right now.
    Broken,
    /// Working, but drifting — it will bite on the next deploy or outage.
    Drift,
    /// Not a problem: something deliberately arranged, printed so it stays
    /// visible (Kenny, form Z3, 2026-09-02).
    ///
    /// The case it exists for: a stack whose data is declared reproducible
    /// has no backup, and a check that only knows Broken and Drift would
    /// either shout "never been backed up" at a decision, or say nothing at
    /// all — and silence makes a deliberate gap indistinguishable from a
    /// forgotten one. Neither is the truth; this is.
    Noted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    pub severity: Severity,
    /// Which stack, container or file this is about.
    pub subject: String,
    /// What is wrong, in one sentence.
    pub what: String,
    /// What to do about it. Every finding carries one (standing rule 11).
    pub remedy: String,
}

/// What the shell reads off the machine so the comparison can stay pure.
#[derive(Debug, Clone, Default)]
pub struct LiveFacts {
    /// vmid → hostname, for every container that exists on the hypervisor.
    pub containers: Vec<(u16, String)>,
    /// Route file name → the address it forwards to, and whether anything
    /// answered there.
    pub routes: Vec<RouteFact>,
    /// Stack directories found in the repository, with the vmid they claim.
    pub stack_files: Vec<(String, u16)>,
    /// G3: what every managed container's resources look like right now.
    pub growth: Vec<GrowthFact>,
    /// Whether each managed stack's safety nets are actually attached.
    pub coverage: Vec<CoverageFact>,
    /// W3: what `pct config` says about boot policy and resources, per
    /// managed container.
    pub boot: Vec<BootFact>,
    /// F184: what the HOST itself has, so a per-container remedy cannot
    /// advise something the machine cannot give. `(total_mb, committed_mb,
    /// swap_used_mb, swap_total_mb)`, or None when it could not be read —
    /// which is not the same as a healthy host and is treated as unknown.
    pub host_memory: Option<(u32, u32, u32, u32)>,
    /// O1: backups made OUTSIDE this suite that it nevertheless watches —
    /// today the router's own nightly upload to Google Drive. Empty = none
    /// declared, which is what every fleet had before this existed.
    pub watched_backups: Vec<WatchedBackupFact>,
    /// T49: the seeder's last verdict about the watch list.
    pub seed: SeedFact,
    /// R13: how full the pools are that the stacks keep their data on. Empty
    /// when no stack declares a data mount, which is what every fleet looked
    /// like before this existed.
    pub pools: Vec<PoolFact>,
}

/// W3: the configured shape of a container that exists, next to the stack
/// file that is supposed to describe it.
///
/// Read from `pct config` rather than from inside the container, so a
/// stopped guest answers as well as a running one — which matters, because
/// "does not start on boot" is precisely the state you find a container in
/// after the reboot that should have started it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BootFact {
    pub vmid: u16,
    pub hostname: String,
    pub live: crate::ops::reconcile::LiveConfig,
}

/// Is this stack actually being measured and actually shipping logs?
///
/// The most expensive class of failure in this fleet is not a service that
/// falls over — it is a mechanism that runs, reports success and is wired to
/// nothing. On 2026-08-31 alone: log caps that ran on five of nine
/// containers, a growth check that watched five of nine, a discovery file the
/// orchestrator wrote for weeks that Prometheus was never told to read, a
/// promtail pipeline reading a field docker does not write, a database
/// answering its healthcheck while every query failed, and an alert chain
/// finished on every side but the middle. Not one was caught by a test. Every
/// one was found by somebody looking.
///
/// So the check looks. Both fields are `Option`: `None` means the question
/// was not asked — no Prometheus or Loki address is configured, or the stack
/// ships no logs by design — and an unasked question must never become a
/// finding.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CoverageFact {
    pub stack: String,
    /// A Prometheus target for this stack answered `up == 1`.
    pub scraped: Option<bool>,
    /// Loki holds at least one line from this stack, carrying a container
    /// name, inside the configured window.
    ///
    /// Labelled, because F79 was not silence. Promtail read `attrs.name` —
    /// a field docker does not write — so lines arrived for months with an
    /// empty `container_name` and the three dashboards querying it stayed
    /// blank. A plain line count would have been green throughout.
    ///
    /// The window is a host setting (`logs_window`, default 24h) rather than
    /// the hour it started as. On 2026-09-01 the hour version reported
    /// `home` and `kp-soft` as "going nowhere" while both were healthy and
    /// merely quiet — a check that alarms on healthy silence is a check that
    /// gets switched off, and then the real silence goes unnoticed too.
    pub logs_recent: Option<bool>,
    /// Grafana holds the dashboard this stack's deploy generated.
    ///
    /// Not "the file was written" — the deploy has believed that for weeks.
    /// It wrote seven dashboards into `/opt/grafana/provisioning/dashboards`
    /// while Grafana mounted `/opt/gateway/grafana/provisioning`, reported
    /// success every time, and its only failure message reads "this stack has
    /// no generated dashboard yet" (F149). So the reader is asked, not the
    /// writer: does Grafana list a dashboard with this stack's uid.
    pub dashboard_provisioned: Option<bool>,
}

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
        if c.len() < 7 {
            continue;
        }
        let path = c[0];
        let (Ok(used), Ok(avail)) = (c[3].parse::<u64>(), c[4].parse::<u64>()) else {
            continue;
        };
        let total = used + avail;
        if total == 0 {
            continue;
        }
        let owners: Vec<&String> = declared
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, s)| s)
            .collect();
        if owners.is_empty() {
            continue;
        }
        let e = by_fs.entry(c[1].to_string()).or_insert_with(|| PoolFact {
            filesystem: c[1].to_string(),
            path: path.to_string(),
            used_pct: (used * 100 / total) as u8,
            free_gb: avail / 1024 / 1024,
            ..Default::default()
        });
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
                         stop the upgrades that are filling it (`upgrade.until_score` in \
                         `stacks/media/recyclarr/recyclarr.yml`)"
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFact {
    pub file: String,
    pub target: String,
    pub answered: bool,
}

/// How stale a backup may be before it counts as a finding. A nightly run
/// that missed one night is noise; two is a pattern (standing rule 27 — a
/// number about patience belongs in configuration, and this is its default).
pub const DEFAULT_BACKUP_MAX_AGE_S: u64 = 48 * 3600;

/// Which findings are worth waking somebody for.
///
/// Z3: a `Noted` finding is a decision, not a fault — the registry cache
/// that is deliberately not backed up, a mount declared unkept. Letting one
/// raise the alarm trains the reader to ignore the notification, and that
/// notification is the one that has to be believed when it IS real.
///
/// Extracted from the nightly loop on 2026-09-02 (G15 of the Phase-7 gate).
/// It lived inside a 340-line async function that no test could reach, which
/// meant the rule deciding what counts as an alarm was itself unguarded.
pub fn alarming(findings: &[Finding]) -> Vec<Finding> {
    findings
        .iter()
        .filter(|f| f.severity != Severity::Noted)
        .cloned()
        .collect()
}

/// The whole comparison, as one pure function.
pub fn evaluate(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
    growth_limits: GrowthLimits,
) -> Vec<Finding> {
    let mut out = Vec::new();

    // F184: is the HOST itself short? Read once, so every per-container
    // remedy below can say something the machine can actually do.
    let host_short: Option<String> = live.host_memory.and_then(|(total, committed, su, st)| {
        let oversubscribed = committed > total;
        let swap_pressed = st > 0 && su * 100 / st.max(1) >= 50;
        (oversubscribed || swap_pressed).then(|| {
            format!(
                "the host has {} MB of RAM with {} MB promised to guests, and {} of its {} MB \
                 of swap in use",
                total, committed, su, st
            )
        })
    });

    // A stack that clones a template which is not there rebuilds into
    // nothing — and it fails at the moment you need the rebuild most. The
    // shape this guards against was found on 2026-09-01: the scaffold default
    // still named `clone:999`, the v1 golden image, two generations after
    // every live stack had moved to 997/998. Nobody noticed because nobody
    // scaffolds a stack often, and the eleven that exist carry their template
    // by hand.
    for (name, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        let Some(rest) = m.lxc.template.trim_matches('"').strip_prefix("clone:") else {
            continue;
        };
        let Ok(tmpl_vmid) = rest.trim().parse::<u16>() else {
            continue;
        };
        if !live.containers.iter().any(|(v, _)| *v == tmpl_vmid) {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "clones template {}, which does not exist on the hypervisor",
                    tmpl_vmid
                ),
                remedy: format!(
                    "point {}'s lxc.template at a golden template that is there, \
                     or rebuild {} with `homelab template-build`",
                    name, tmpl_vmid
                ),
            });
        }
    }

    for (name, st) in &state.stacks {
        match live.containers.iter().find(|(v, _)| *v == st.vmid) {
            None => out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!("recorded on vmid {}, which does not exist", st.vmid),
                remedy: "the container was removed outside the orchestrator — redeploy it, or remove the stack from host state".into(),
            }),
            Some((_, hostname)) if hostname != &st.hostname => out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!(
                    "recorded as '{}' but vmid {} is really '{}' — every operation on this stack fails the hostname guard",
                    st.hostname, st.vmid, hostname
                ),
                remedy: "re-adopt or redeploy the stack so its record matches the container; this is how kyu's backup stopped for eight weeks".into(),
            }),
            Some(_) => {}
        }

        if !st.enabled {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: "disabled — the nightly run skips it entirely".into(),
                remedy: format!(
                    "a failed nightly run auto-disables a stack (H8); fix the cause, then `homelab enable {}`",
                    name
                ),
            });
        }

        let age = now_unix.saturating_sub(st.last_backup);
        // Z3: a stack that keeps nothing worth keeping, by declaration, is
        // not a stack that was forgotten. Saying "never been backed up"
        // about a decision trains the reader to ignore the line — and that
        // line is the one that has to be believed when it IS real.
        let declared_unkept: Vec<&crate::manifest::MountSpec> = st
            .manifest
            .as_ref()
            .map(|m| m.storage.iter().filter(|s| s.no_backup.is_some()).collect())
            .unwrap_or_default();
        let keeps_nothing = st
            .manifest
            .as_ref()
            .map(|m| {
                !m.storage.is_empty()
                    && m.storage.iter().all(|s| s.no_data || s.no_backup.is_some())
            })
            .unwrap_or(false);
        for mount in &declared_unkept {
            out.push(Finding {
                severity: Severity::Noted,
                subject: name.clone(),
                what: format!(
                    "{} is deliberately not backed up — {}",
                    mount.host_path,
                    mount.no_backup.as_deref().unwrap_or("")
                ),
                remedy:
                    "nothing to do; listed so a deliberate gap never looks like a forgotten one"
                        .into(),
            });
        }
        if keeps_nothing {
            // Deliberate, already said above per mount.
        } else if st.last_backup == 0 {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: "has never been backed up".into(),
                remedy: "run a backup now and check why the nightly one never did".into(),
            });
        } else if age > backup_max_age_s {
            out.push(Finding {
                severity: Severity::Broken,
                subject: name.clone(),
                what: format!("last backup was {} hours ago", age / 3600),
                remedy: "check the nightly run and the backup target".into(),
            });
        }
    }

    // A stack file that claims a vmid belonging to something else is one
    // hostname guard away from a deploy landing on a live container.
    for (dir, vmid) in &live.stack_files {
        let owned_by_state = state.stacks.values().any(|s| s.vmid == *vmid);
        let exists = live.containers.iter().any(|(v, _)| v == vmid);
        if exists && !owned_by_state {
            out.push(Finding {
                severity: Severity::Drift,
                subject: dir.clone(),
                what: format!(
                    "claims vmid {}, which is a live container this orchestrator does not manage",
                    vmid
                ),
                remedy: "only the hostname guard stands between this file and a deploy onto that container — delete the file or point it at a free vmid".into(),
            });
        }
    }

    out.extend(evaluate_growth(
        &live.growth,
        growth_limits,
        host_short.as_deref(),
    ));
    out.extend(evaluate_pools(&live.pools, growth_limits));
    out.extend(evaluate_coverage(&live.coverage));
    out.extend(evaluate_boot(state, &live.boot));
    out.extend(evaluate_watched_backups(&live.watched_backups));
    out.extend(evaluate_incomplete(state));
    out.extend(evaluate_notify(state, now_unix));
    // A seeder that ran more than a day ago has stopped keeping the watch
    // list in step, which is the same window the backups use.
    out.extend(evaluate_seed(&live.seed, 26 * 3600));
    out.extend(crate::ops::restoredrill::evaluate_drill(
        state,
        now_unix,
        crate::ops::restoredrill::DEFAULT_DRILL_INTERVAL_S,
    ));
    out.extend(crate::ops::manualchecks::evaluate_manual(
        state,
        now_unix,
        crate::ops::manualchecks::DEFAULT_ANSWER_MAX_AGE_S,
    ));

    for r in &live.routes {
        if !r.answered {
            out.push(Finding {
                severity: Severity::Broken,
                subject: r.file.clone(),
                what: format!("routes to {}, where nothing answers", r.target),
                remedy: "fix the address or delete the route; a dead route is only found when someone needs it".into(),
            });
        }
    }

    out
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
        if g.disk_used_pct >= lim.disk_broken_pct {
            out.push(Finding {
                severity: Severity::Broken,
                subject: who.clone(),
                what: format!("rootfs is {}% full", g.disk_used_pct),
                remedy: "an image pull or an apt upgrade can fail halfway from here — free space or grow the disk with `pct resize`".into(),
            });
        } else if g.disk_used_pct >= lim.disk_drift_pct {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("rootfs is {}% full", g.disk_used_pct),
                remedy: "still fine, but find out what is growing before it is urgent — `du -xh --max-depth=2 /` inside the container".into(),
            });
        }
        if g.mem_used_pct >= lim.mem_drift_pct {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("using {}% of its memory allocation", g.mem_used_pct),
                remedy: "raise memory_mb in the stack manifest and redeploy; the alternative is swapping, which hides the pressure instead of reporting it".into(),
            });
        }
        if g.swap_used_mb >= lim.swap_drift_mb {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
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
            });
        }
        if g.journal_mb >= lim.journal_mb {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("journal is {} MB", g.journal_mb),
                remedy: format!("the guards cap it well below this — run `homelab guards {}` and check the cap took effect", g.vmid),
            });
        }
        if g.docker_logs_mb >= lim.docker_logs_mb {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who.clone(),
                what: format!("docker container logs total {} MB", g.docker_logs_mb),
                remedy: format!("the cap only applies to containers created after it was set — run `homelab guards {}`, then recreate the noisiest container so it picks the cap up", g.vmid),
            });
        }
        if !g.guards {
            out.push(Finding {
                severity: Severity::Drift,
                subject: who,
                what: "has no runaway guards: no journald cap, no docker log cap, or both".into(),
                remedy: format!("`homelab guards {}` — without them nothing bounds log growth, which is how one service reached 923 MB unnoticed", g.vmid),
            });
        }
    }
    out
}

/// Is each stack's safety net attached? See `CoverageFact` for why this
/// exists at all.
/// O1 (Kenny, 2026-09-02): a backup this orchestrator does not MAKE but does
/// WATCH.
///
/// OPNsense is on the no-touch list and backs itself up: a plugin uploads an
/// encrypted copy of the router's configuration to Google Drive every night.
/// Nothing in this suite makes that backup and nothing should. But a backup
/// nobody watches is a backup you discover is broken on the day you need it,
/// so the nightly round that already asks "when was this last backed up" of
/// every stack asks it of that folder too.
///
/// Deliberately not a Kuma push monitor: that needs a script on the host
/// calling in on a timer, and a separately maintained file on a machine is
/// the thing Kenny ruled out on 2026-09-02. This reuses the round that
/// already runs, the finding shape that already exists, and the notification
/// path that already reaches him.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WatchedBackupFact {
    /// What to call it in the report.
    pub name: String,
    /// Age of the NEWEST file found, in seconds. None = nothing was found,
    /// which is not the same as "old" and is reported differently.
    pub newest_age_s: Option<u64>,
    /// Older than this and it is a finding.
    pub max_age_s: u64,
    /// The listing itself failed — no answer is not a healthy answer.
    pub error: Option<String>,
}

/// Broken rather than Drift throughout: unlike a missing metric, a backup
/// that stopped is not a slower kind of wrong. It is the thing itself.
pub fn evaluate_watched_backups(facts: &[WatchedBackupFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for f in facts {
        if let Some(e) = &f.error {
            out.push(Finding {
                severity: Severity::Broken,
                subject: f.name.clone(),
                what: format!("could not be listed ({})", e),
                remedy: "check the rclone remote and the path in host.toml — a listing that \
                         does not answer says nothing about the backup either way"
                    .into(),
            });
            continue;
        }
        match f.newest_age_s {
            None => out.push(Finding {
                severity: Severity::Broken,
                subject: f.name.clone(),
                what: "holds no files at all".into(),
                remedy: "the device that writes here has never succeeded, or writes somewhere \
                         else than host.toml says"
                    .into(),
            }),
            Some(age) if age > f.max_age_s => out.push(Finding {
                severity: Severity::Broken,
                subject: f.name.clone(),
                what: format!(
                    "newest file is {} hours old, expected one within {}",
                    age / 3600,
                    f.max_age_s / 3600
                ),
                remedy: "the device stopped uploading — check its backup settings and its \
                         credentials before the next config change is the one you need back"
                    .into(),
            }),
            Some(_) => {}
        }
    }
    out
}

/// G8: a stack whose last deploy stopped halfway, reported out loud.
///
/// `incomplete_step` has been written since S2 and read by nobody. That is
/// the exact shape this whole audit keeps finding: a mechanism that runs,
/// records the truth, and is wired to nothing. The record was added because
/// the media stack failed at "start apps" and therefore did not exist as far
/// as the orchestrator was concerned — 12 GB of configuration with no nightly
/// backup, and nothing anywhere saying so. Writing the field fixed the
/// backup; it did not make anyone aware. This does.
///
/// Severity is Broken rather than Drift on purpose: a half-applied stack is
/// not a container that will bite later, it is one whose running state nobody
/// has claimed. The remedy names the step, because "the deploy failed" a week
/// ago is not something Kenny can act on and "it stopped at start apps" is.
/// G16: is the path by which Kenny learns anything still working?
///
/// The circularity is the point and cannot be engineered away: if every
/// notification route is down, this finding cannot reach him by notification
/// either. What it can do is be there in `homelab check` and the TUI, so the
/// question "why has it been so quiet" has an answer other than a guess.
pub fn evaluate_notify(state: &HostState, now: u64) -> Vec<Finding> {
    if state.last_notify_failed <= state.last_notify_ok {
        return Vec::new();
    }
    let ago = now.saturating_sub(state.last_notify_failed) / 60;
    let last_ok = if state.last_notify_ok == 0 {
        "and none has ever arrived".to_string()
    } else {
        format!(
            "the last one that arrived was {} h earlier",
            state
                .last_notify_failed
                .saturating_sub(state.last_notify_ok)
                / 3600
        )
    };
    vec![Finding {
        severity: Severity::Broken,
        subject: "notifications".into(),
        what: format!(
            "no route accepted the last notification ({} min ago{}){}",
            ago,
            state
                .last_notify_error
                .as_ref()
                .map(|e| format!(": {}", e))
                .unwrap_or_default(),
            format_args!(" — {}", last_ok)
        ),
        remedy: "check kyu on 10.10.10.9 and `notify_fallback_webhook` in host.toml — \
                 while this stands, every warning this host produces is going nowhere, \
                 including this one"
            .into(),
    }]
}

/// T49: what the Uptime Kuma seeder concluded on its last run.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SeedFact {
    /// Monitors watching a stack the fleet no longer has.
    pub stale: Vec<String>,
    /// How long ago the seeder last ran, in seconds. None = never, or the
    /// status file could not be read.
    pub age_s: Option<u64>,
    /// Whether it had a generated list to judge against. Without one it
    /// judges nothing, which is right and must not read as "all clear".
    pub judged: bool,
    /// The file could not be read or understood.
    pub error: Option<String>,
}

/// A monitor that outlives its stack, reported where somebody sees it.
///
/// The seeder has always said this in its own log — it is deliberately
/// report-only, because deleting somebody's monitor is the irreversible
/// direction (Kenny, form H2b). What it did not have was a reader. On
/// 2026-09-02 Kenny found a monitor still pinging a drill container that had
/// been destroyed hours earlier, and he found it by noticing ping errors on a
/// Grafana panel. The seeder had been saying so every minute since.
pub fn evaluate_seed(fact: &SeedFact, max_age_s: u64) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(e) = &fact.error {
        out.push(Finding {
            severity: Severity::Broken,
            subject: "uptime seeder".into(),
            what: format!("said nothing about the watch list ({})", e),
            remedy: "check the kuma-seeder container on the uptime stack — while this \
                     stands, a monitor can outlive its stack with nothing saying so"
                .into(),
        });
        return out;
    }
    match fact.age_s {
        None => out.push(Finding {
            severity: Severity::Broken,
            subject: "uptime seeder".into(),
            what: "has never recorded a run".into(),
            remedy: "the watch list is not being kept in step with the fleet".into(),
        }),
        Some(age) if age > max_age_s => out.push(Finding {
            severity: Severity::Broken,
            subject: "uptime seeder".into(),
            what: format!("last ran {} h ago", age / 3600),
            remedy: "a new stack arrives unwatched until this runs again".into(),
        }),
        _ => {}
    }
    if !fact.judged {
        out.push(Finding {
            severity: Severity::Drift,
            subject: "uptime seeder".into(),
            what: "ran without a generated monitor list, so it judged nothing stale".into(),
            remedy: "absent data is not an empty fleet (F175) — check that the deploy is \
                     writing the host-monitors file"
                .into(),
        });
    } else if !fact.stale.is_empty() {
        out.push(Finding {
            severity: Severity::Drift,
            subject: "uptime kuma".into(),
            what: format!(
                "{} monitor(s) watch a stack the fleet no longer has: {}",
                fact.stale.len(),
                fact.stale.join(", ")
            ),
            remedy: "remove them in Uptime Kuma if that is right — this suite never deletes \
                     somebody's monitor (form H2b), so it can only tell you"
                .into(),
        });
    }
    out
}

pub fn evaluate_incomplete(state: &HostState) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(step) = st.incomplete_step.as_ref() else {
            continue;
        };
        out.push(Finding {
            severity: Severity::Broken,
            subject: name.clone(),
            what: format!("its last deploy stopped at \"{}\" and never finished", step),
            remedy: "run the deploy again and watch that step — until it completes, what runs \
                     on the container and what the orchestrator has on record are two different \
                     things, and only the record drives drift detection and retention"
                .into(),
        });
    }
    out.sort_by(|a, b| a.subject.cmp(&b.subject));
    out
}

///
/// Drift rather than Broken throughout: nothing is failing: the stack runs
/// fine. What is missing is the ability to find out when it stops, which is a
/// slower and more expensive kind of wrong.
pub fn evaluate_coverage(facts: &[CoverageFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for c in facts {
        if c.scraped == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "no Prometheus target answers for this stack — it is not being measured".into(),
                remedy: "check /appdata/metrics/prometheus-config/targets/<stack>.json exists and that Prometheus reads that directory; a deploy writes the file, and for weeks nothing read it".into(),
            });
        }
        if c.dashboard_provisioned == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "Grafana does not have this stack's generated dashboard — the deploy wrote it somewhere Grafana never reads".into(),
                remedy: "check that `grafana_dashboards_dir` in host.toml is a directory the Grafana container actually mounts; the deploy's own report cannot tell you, because writing the file is all it checks (F149)".into(),
            });
        }
        if c.logs_recent == Some(false) {
            out.push(Finding {
                severity: Severity::Drift,
                subject: c.stack.clone(),
                what: "no LABELLED log line reached Loki from this stack recently — either nothing is shipping, or it ships without the container name (F79)".into(),
                remedy: "check promtail is running there and that its pipeline matches what docker writes; the container name came from a field docker does not produce for the life of this fleet".into(),
            });
        }
    }
    out
}

/// The address a route forwards to, as `host/port` for a `/dev/tcp` probe.
///
/// Extracted from the shell so it can be tested: the first live run reported
/// every route in the house dead, and one of the two reasons was here —
/// `https://10.10.5.1` carries no port, so probing it as written asks for a
/// path rather than a socket. The other reason was the shell itself (dash has
/// no /dev/tcp), which no amount of testing this function would have caught.
/// Both were invisible until the check ran against the real fleet once.
pub fn probe_hostport(target: &str) -> String {
    let (scheme, rest) = match target.split_once("://") {
        Some((s, r)) => (s, r),
        None => ("", target),
    };
    let authority = rest.split('/').next().unwrap_or(rest);
    match authority.rsplit_once(':') {
        // An IPv6 literal has colons of its own; only a numeric tail is a port.
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => {
            format!("{}/{}", host, port)
        }
        _ => format!("{}/{}", authority, if scheme == "https" { 443 } else { 80 }),
    }
}

/// W3: report a container whose boot policy or resources no longer match the
/// stack file. Boot policy is a drift a deploy repairs on its own; memory and
/// cores are named with the remedy that actually applies, because raising
/// them is a deliberate operation and lowering them is a rebuild.
///
/// A stack whose state record carries no manifest is skipped rather than
/// guessed at — the same rule as everywhere else here: a question that was
/// not asked never becomes a finding.
pub fn evaluate_boot(state: &HostState, facts: &[BootFact]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (name, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        let Some(fact) = facts.iter().find(|f| f.vmid == st.vmid) else {
            continue;
        };
        for d in crate::ops::reconcile::divergences(m, &fact.live) {
            let boot_related = d.starts_with("starts on boot") || d.starts_with("boot order");
            out.push(Finding {
                severity: Severity::Drift,
                subject: name.clone(),
                what: d,
                remedy: if boot_related {
                    "a deploy puts the boot policy back; until then a reboot starts the fleet in the wrong order".into()
                } else {
                    "raise it with `homelab resize`, or lower it by rebuilding the container — a deploy deliberately does not change resources under a running service".into()
                },
            });
        }
    }
    out
}
