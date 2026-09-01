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
        }
    }
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

/// The whole comparison, as one pure function.
pub fn evaluate(
    state: &HostState,
    live: &LiveFacts,
    now_unix: u64,
    backup_max_age_s: u64,
    growth_limits: GrowthLimits,
) -> Vec<Finding> {
    let mut out = Vec::new();

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
        if st.last_backup == 0 {
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

    out.extend(evaluate_growth(&live.growth, growth_limits));
    out.extend(evaluate_coverage(&live.coverage));
    out.extend(evaluate_boot(state, &live.boot));

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
pub fn evaluate_growth(facts: &[GrowthFact], lim: GrowthLimits) -> Vec<Finding> {
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
                remedy: "swap turns memory pressure into slow degradation instead of a loud failure — give the container more memory rather than more swap".into(),
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
