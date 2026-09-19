//! G6 · the fact gatherer behind every nightly finding, behind an executor.
//!
//! This used to be a 345-line function in the host binary, bound to
//! `RealExecutor` and reading its settings straight out of the host's
//! `AppState`. Every finding the fleet check produces starts here, and it
//! had no test at all: the pure judgement in `fleetcheck` was well covered,
//! and the readings it judges were taken by code nothing could run without
//! a fleet. Phase 7 (T79) named it the one gap worth deferring; Kenny turned
//! that into "build it" on 2026-09-19.
//!
//! Nothing here judges. It asks the machine questions and writes the answers
//! into `LiveFacts`; `fleetcheck` decides what they mean. Two rules carried
//! over from the host version, both learned the expensive way:
//!
//! - **An unasked question never becomes a finding.** A setting that is not
//!   configured leaves its fact absent, not failed.
//! - **Not examined is not the same as examined and found wanting.** A probe
//!   that produced nothing (a stopped guest, an exec that failed) adds no
//!   fact, because a zero read out of a failed measurement is a false
//!   finding wearing plausible numbers.

use crate::executor::{Cmd, Executor};
use crate::ops::fleetcheck::{
    BootFact, CoverageFact, GrowthFact, LiveFacts, RouteFact, SeedFact, WatchedBackupFact,
};

/// A backup made outside this suite that it nevertheless watches (O1).
#[derive(Debug, Clone)]
pub struct WatchedBackupSpec {
    pub name: String,
    pub rclone_path: String,
    pub max_age_hours: u64,
}

/// Everything the gatherer needs from the host's configuration — passed in
/// rather than read from `AppState`, so a test can hand it a fleet of its
/// own.
#[derive(Debug, Clone)]
pub struct FactsInputs {
    pub watched_backups: Vec<WatchedBackupSpec>,
    pub kuma_monitors_file: Option<String>,
    pub state_dir: String,
    pub gateway_vmid: u16,
    pub gateway_routes_dir: String,
    pub no_touch: Vec<u16>,
    pub prometheus_url: Option<String>,
    pub loki_url: Option<String>,
    pub logs_window: String,
    pub grafana_dashboards_dir: Option<String>,
    /// Now, in seconds since the epoch. Core never reads a clock.
    pub now_unix: u64,
}

/// What the gatherer measured, said out loud even when nothing is wrong.
///
/// A pool check that only speaks up when a pool is filling is
/// indistinguishable from one that reads nothing at all (F263). These are
/// the lines the host logs at info level after the facts come back.
pub type Notes = Vec<String>;

/// Only `<digits><unit>` reaches a query string. The window is pasted into a
/// hand-built URL, and a value out of a config file is not a value to trust
/// with that: anything else falls back to the default rather than producing
/// a query that silently means something other than it says.
pub fn sane_window(w: &str, default: &str) -> String {
    let ok = w.len() >= 2
        && w.chars().next().is_some_and(|c| c.is_ascii_digit())
        && w[..w.len() - 1].chars().all(|c| c.is_ascii_digit())
        && matches!(w.chars().last(), Some('s' | 'm' | 'h' | 'd' | 'w'));
    if ok {
        w.to_string()
    } else {
        default.to_string()
    }
}

/// F184: total RAM, memory promised to guests, and swap in use — the three
/// numbers that decide whether "give this container more memory" is advice
/// or nonsense. None when any of them cannot be read, which is deliberately
/// not the same as a healthy host. `(total_mb, committed_mb, swap_used_mb,
/// swap_total_mb)`.
pub async fn read_host_memory(exec: &dyn Executor) -> Option<(u32, u32, u32, u32)> {
    let out = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                // free gives total and swap; pct/qm give what is promised.
                "free -m | awk '/^Mem:/{print $2} /^Swap:/{print $3\" \"$2}'; \
                 pct list 2>/dev/null | awk 'NR>1{print $1}' | \
                   xargs -r -n1 pct config 2>/dev/null | awk '/^memory:/{s+=$2} END{print s+0}'; \
                 qm list 2>/dev/null | awk 'NR>1{s+=$4} END{print s+0}'",
            ],
            60,
        ))
        .await
        .ok()?;
    parse_host_memory(&out.stdout)
}

/// The five numbers `read_host_memory` asks for, in the order its script
/// prints them: total, swap_used, swap_total, lxc_committed, vm_committed.
pub fn parse_host_memory(stdout: &str) -> Option<(u32, u32, u32, u32)> {
    let n: Vec<&str> = stdout.split_whitespace().collect();
    if n.len() < 5 {
        return None;
    }
    let p = |i: usize| n.get(i)?.parse::<u32>().ok();
    Some((p(0)?, p(3)? + p(4)?, p(1)?, p(2)?))
}

/// `pct list` → (vmid, hostname) per container on the hypervisor.
pub fn parse_pct_list(stdout: &str) -> Vec<(u16, String)> {
    let mut out = Vec::new();
    for line in stdout.lines().skip(1) {
        let mut cols = line.split_whitespace();
        if let (Some(vmid), Some(_status)) = (cols.next(), cols.next()) {
            if let (Ok(vmid), Some(name)) = (vmid.parse::<u16>(), cols.last()) {
                out.push((vmid, name.to_string()));
            }
        }
    }
    out
}

/// G3: the one-shot probe every managed container answers with key=value
/// lines. The `guards` line is unconditional, so its absence means the shell
/// never got there.
pub const GROWTH_PROBE: &str = concat!(
    "df -P / | awk 'NR==2{gsub(\"%\",\"\",$5); print \"disk=\"$5}'; ",
    "free -m | awk '/^Mem:/{if($2>0) printf \"mem=%d\\n\", ($3*100)/$2} /^Swap:/{print \"swap=\"$3}'; ",
    "echo \"journal=$(du -sm /var/log/journal 2>/dev/null | cut -f1)\"; ",
    "echo \"dockerlogs=$(du -sm /var/lib/docker/containers 2>/dev/null | cut -f1)\"; ",
    // Both halves of the guard must be present. Checking only one is how
    // a half-guarded container reads as guarded.
    "if ls /etc/systemd/journald.conf.d/*.conf >/dev/null 2>&1 && ",
    "grep -q max-size /etc/docker/daemon.json 2>/dev/null; ",
    "then echo guards=1; else echo guards=0; fi"
);

/// The probe's answer as a fact — `None` when the probe never ran inside the
/// container (no `guards` line), which is the stopped-template case that
/// once reported three golden images as unguarded.
pub fn parse_growth(vmid: u16, hostname: &str, stdout: &str) -> Option<GrowthFact> {
    let mut g = GrowthFact {
        vmid,
        hostname: hostname.to_string(),
        ..Default::default()
    };
    let mut probed = false;
    for line in stdout.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k {
            "disk" => g.disk_used_pct = v.parse().unwrap_or(0),
            "mem" => g.mem_used_pct = v.parse().unwrap_or(0),
            "swap" => g.swap_used_mb = v.parse().unwrap_or(0),
            "journal" => g.journal_mb = v.parse().unwrap_or(0),
            "dockerlogs" => g.docker_logs_mb = v.parse().unwrap_or(0),
            "guards" => {
                g.guards = v == "1";
                probed = true;
            }
            _ => {}
        }
    }
    probed.then_some(g)
}

/// The uids of the generated dashboards Grafana is actually serving.
///
/// Asked of Grafana over its own API, with the credentials read from the
/// app's `.env` at the moment of asking — a credential handed to a check
/// goes stale without telling anybody (F131), and the service itself is the
/// only source that cannot. The `.env` path is DERIVED from the dashboards
/// directory rather than typed, because a second typed path is exactly what
/// caused the fault this question exists to catch (F149).
///
/// `None` means the question could not be asked at all — never an empty
/// answer, so a gateway that is down does not turn every stack into a
/// finding.
pub async fn grafana_generated_uids(
    exec: &dyn Executor,
    gateway_vmid: u16,
    dashboards_dir: &str,
) -> Option<Vec<String>> {
    let app_dir = std::path::Path::new(dashboards_dir).parent()?.to_str()?;
    let script = format!(
        "U=$(grep -h GRAFANA_GF_ADMIN_USER {0}/.env | cut -d= -f2); \
         P=$(grep -h GRAFANA_GF_ADMIN_PASSWORD {0}/.env | cut -d= -f2); \
         curl -s -m 15 -u \"$U:$P\" 'http://127.0.0.1:3000/api/search?tag=generated&limit=500'",
        app_dir
    );
    let out = exec
        .run(&Cmd::new(
            "pct",
            &["exec", &gateway_vmid.to_string(), "--", "sh", "-c", &script],
            60,
        ))
        .await
        .ok()?;
    if !out.stdout.contains("\"uid\"") {
        return None;
    }
    Some(
        out.stdout
            .split("\"uid\":\"")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .map(str::to_string)
            .collect(),
    )
}

/// Read off the machine everything `fleetcheck` judges.
pub async fn gather_live_facts(
    exec: &dyn Executor,
    inp: &FactsInputs,
    stack_files: &[(String, u16)],
) -> (LiveFacts, Notes) {
    let mut notes: Notes = Vec::new();

    // O1: what the router (and anything else outside this suite) uploaded
    // last night. Read through the rclone remote that already exists for the
    // restic repositories — no new credential, no new timer.
    let mut watched = Vec::new();
    for w in &inp.watched_backups {
        let out = exec
            .run(&Cmd::new(
                "sh",
                &[
                    "-c",
                    &format!(
                        "rclone lsjson --files-only '{}' 2>&1 | \
                         sed -n 's/.*\"ModTime\":\"\\([^\"]*\\)\".*/\\1/p' | sort | tail -1",
                        w.rclone_path
                    ),
                ],
                180,
            ))
            .await;
        let mut fact = WatchedBackupFact {
            name: w.name.clone(),
            max_age_s: w.max_age_hours * 3600,
            ..Default::default()
        };
        match out {
            Err(e) => fact.error = Some(e.to_string()),
            Ok(o) if !o.success() => fact.error = Some(o.stderr.trim().to_string()),
            Ok(o) => {
                let newest = o.stdout.trim().to_string();
                if !newest.is_empty() {
                    // rclone prints RFC3339; the host has `date` and this
                    // avoids a chrono dependency in a place that has none.
                    if let Ok(d) = exec
                        .run(&Cmd::new("date", &["-d", &newest, "+%s"], 30))
                        .await
                    {
                        if let Ok(t) = d.stdout.trim().parse::<u64>() {
                            fact.newest_age_s = Some(inp.now_unix.saturating_sub(t));
                        }
                    }
                }
            }
        }
        watched.push(fact);
    }

    // T49: the seeder's verdict, read from the file it writes beside the
    // generated monitor list. Same directory, so there is no second setting
    // to keep in step with the first.
    let seed = match inp.kuma_monitors_file.as_deref() {
        None => SeedFact {
            judged: true,
            age_s: Some(0),
            ..Default::default()
        },
        Some(monitors) => {
            let dir = monitors.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
            let path = format!("{}/last-seed.json", dir);
            match exec.read_file(&path).await {
                Err(e) => SeedFact {
                    error: Some(format!("{}: {}", path, e)),
                    ..Default::default()
                },
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Err(e) => SeedFact {
                        error: Some(format!("{} is not JSON: {}", path, e)),
                        ..Default::default()
                    },
                    Ok(v) => SeedFact {
                        stale: v["stale"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|x| x.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        age_s: v["at"].as_u64().map(|at| inp.now_unix.saturating_sub(at)),
                        judged: v["judged"].as_bool().unwrap_or(false),
                        error: None,
                    },
                },
            }
        }
    };

    let mut facts = LiveFacts {
        seed,
        stack_files: stack_files.to_vec(),
        watched_backups: watched,
        // F184: the host's own numbers, so a per-container remedy cannot
        // advise memory the machine does not have.
        host_memory: read_host_memory(exec).await,
        ..Default::default()
    };

    // The stacks the host has recorded — read once, used twice below.
    let snapshot = crate::state::StateStore::new(exec, &inp.state_dir)
        .load()
        .await
        .ok();

    // R13: how full are the pools the libraries actually live on. The paths
    // come from the stacks' own `data_mounts`, so this watches what is
    // declared rather than a list somebody keeps in step by hand. One `df`
    // for all of them, keyed by filesystem: two stacks that name different
    // paths on one pool are one pool.
    if let Some(snapshot) = snapshot.as_ref() {
        let mut declared: Vec<(String, String)> = Vec::new();
        for (name, st) in &snapshot.stacks {
            if let Some(m) = &st.manifest {
                for dm in &m.data_mounts {
                    declared.push((dm.host_path.clone(), name.clone()));
                }
            }
        }
        if !declared.is_empty() {
            // Each line carries the path we ASKED about, printed by us, not
            // df's own first column: a path that does not exist produces no
            // row at all, every later row shifts up one, and the pool of one
            // stack gets reported under the name of another.
            let mut paths: Vec<&String> = declared.iter().map(|(p, _)| p).collect();
            paths.sort();
            paths.dedup();
            let script = paths
                .iter()
                .map(|p| {
                    format!(
                        "printf '%s ' '{}'; df -Pk '{}' 2>/dev/null | tail -n +2 | head -1; echo",
                        p, p
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            if let Ok(out) = exec.run(&Cmd::new("sh", &["-c", &script], 60)).await {
                facts.pools = crate::ops::fleetcheck::pool_facts_from_df(&out.stdout, &declared);
                notes.push(format!(
                    "fleet check: {} data pool(s) measured — {}",
                    facts.pools.len(),
                    facts
                        .pools
                        .iter()
                        .map(|p| format!(
                            "{} {}% full, {} GB free ({})",
                            p.path,
                            p.used_pct,
                            p.free_gb,
                            p.stacks.join(", ")
                        ))
                        .collect::<Vec<_>>()
                        .join(" · ")
                ));
            }
        }
    }

    if let Ok(out) = exec.run(&Cmd::new("pct", &["list"], 30)).await {
        facts.containers = parse_pct_list(&out.stdout);
    }

    // Gateway routes: read every fragment, pull out the address it forwards
    // to, and ask whether anything is listening there. A route that resolves
    // to nothing is only ever found by someone who needs it.
    let gw = inp.gateway_vmid.to_string();
    let script = format!(
        "for f in {}/*.yml; do echo \"### $(basename $f)\"; cat \"$f\"; done 2>/dev/null",
        inp.gateway_routes_dir
    );
    if let Ok(out) = exec
        .run(&Cmd::new(
            "pct",
            &["exec", &gw, "--", "sh", "-c", &script],
            60,
        ))
        .await
    {
        let mut current = String::new();
        for line in out.stdout.lines() {
            if let Some(name) = line.strip_prefix("### ") {
                current = name.to_string();
                continue;
            }
            let t = line.trim();
            let target = t
                .strip_prefix("- url:")
                .or_else(|| t.strip_prefix("- address:"))
                .map(|v| v.trim().trim_matches('"').to_string());
            if let Some(target) = target {
                let hostport = crate::ops::fleetcheck::probe_hostport(target.trim_matches('"'));
                // bash, not sh: /dev/tcp is a bash feature and the shell in
                // these containers is dash, which reports "Directory
                // nonexistent" for every address. The first run of this
                // check called every route in the house dead.
                let probe = format!(
                    "timeout 3 bash -c 'echo > /dev/tcp/{}' 2>/dev/null && echo up || echo down",
                    hostport
                );
                let answered = exec
                    .run(&Cmd::new(
                        "pct",
                        &["exec", &gw, "--", "sh", "-c", &probe],
                        15,
                    ))
                    .await
                    .map(|o| o.stdout.contains("up"))
                    .unwrap_or(false);
                facts.routes.push(RouteFact {
                    file: current.clone(),
                    target,
                    answered,
                });
            }
        }
    }

    // G3: what each managed container's resources look like right now — every
    // container on the hypervisor except the untouchable ones, not only the
    // stacks this orchestrator has adopted (the four it could not see were
    // the oldest and fullest on the machine).
    let managed: Vec<(u16, String)> = facts
        .containers
        .iter()
        .filter(|(v, _)| !inp.no_touch.contains(v))
        .map(|(v, h)| (*v, h.clone()))
        .collect();
    for (vmid, hostname) in &managed {
        let vs = vmid.to_string();
        let Ok(out) = exec
            .run(&Cmd::new(
                "pct",
                &["exec", &vs, "--", "sh", "-c", GROWTH_PROBE],
                45,
            ))
            .await
        else {
            continue;
        };
        if let Some(g) = parse_growth(*vmid, hostname, &out.stdout) {
            facts.growth.push(g);
        }
    }

    // W3: the configured shape of every managed container, from `pct config`
    // rather than from inside it — `pct exec` cannot ask a stopped guest
    // anything, and the stopped guest is exactly the one to find.
    for (vmid, hostname) in &managed {
        let vs = vmid.to_string();
        let Ok(out) = exec.run(&Cmd::new("pct", &["config", &vs], 30)).await else {
            continue;
        };
        if !out.success() {
            continue;
        }
        facts.boot.push(BootFact {
            vmid: *vmid,
            hostname: hostname.clone(),
            live: crate::ops::reconcile::parse(&out.stdout),
        });
    }

    // Is each stack's safety net actually attached? Both questions are
    // skipped when their address is not configured: an unasked question must
    // never become a finding.
    let prom = inp.prometheus_url.clone();
    let loki = inp.loki_url.clone();
    let window = &inp.logs_window;
    let provisioned: Option<Vec<String>> = match inp.grafana_dashboards_dir.as_deref() {
        Some(dir) => grafana_generated_uids(exec, inp.gateway_vmid, dir).await,
        None => None,
    };
    if prom.is_some() || loki.is_some() {
        if let Some(snapshot) = snapshot.as_ref() {
            for (name, st) in &snapshot.stacks {
                let mut c = CoverageFact {
                    stack: name.clone(),
                    ..Default::default()
                };
                if let Some(base) = prom.as_deref() {
                    let q = format!(
                        "{}/api/v1/query?query=max(up%7Bstack%3D%22{}%22%7D)",
                        base.trim_end_matches('/'),
                        name
                    );
                    c.scraped = Some(
                        exec.run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
                            .await
                            .map(|o| o.stdout.contains("\"1\""))
                            .unwrap_or(false),
                    );
                }
                // Only ask about logs where logs are expected. A native
                // service with no promtail ships none by design.
                let ships_logs = st
                    .manifest
                    .as_ref()
                    .map(|m| m.apps.iter().any(|a| a == "promtail"))
                    .unwrap_or(false);
                if let (Some(base), true) = (loki.as_deref(), ships_logs) {
                    // `container_name=~".+"` is not decoration (F79): lines
                    // kept arriving for months without the label the
                    // dashboards query by. Counting LABELLED lines is the
                    // question that was actually being got wrong.
                    let q = format!(
                        "{}/loki/api/v1/query?query=sum(count_over_time(%7Bstack%3D%22{}%22%2Ccontainer_name%3D~%22.%2B%22%7D%5B{}%5D))",
                        base.trim_end_matches('/'),
                        name,
                        window
                    );
                    c.logs_recent = Some(
                        exec.run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
                            .await
                            .map(|o| o.stdout.contains("\"value\""))
                            .unwrap_or(false),
                    );
                }
                if let Some(uids) = provisioned.as_ref() {
                    let uid = format!("homelab-{}", name);
                    c.dashboard_provisioned = Some(uids.iter().any(|u| u == &uid));
                }
                facts.coverage.push(c);
            }
        }
    }
    (facts, notes)
}
