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

use crate::executor::{Cmd, Executor, shq};
use crate::ops::fleetcheck::{
    BootFact, CoverageFact, GrowthFact, LiveFacts, RouteFact, WatchedBackupFact,
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
    pub state_dir: String,
    pub gateway_vmid: u16,
    pub gateway_routes_dir: String,
    pub no_touch: Vec<u16>,
    pub prometheus_url: Option<String>,
    pub loki_url: Option<String>,
    /// fix-93: the container Loki runs in. Set, the log question is asked
    /// from inside it on [`crate::ops::logshipper::LOKI_QUERY_LOOPBACK`],
    /// because the LAN port takes pushes only; unset, it is asked at
    /// `loki_url` from the host, as before.
    pub loki_vmid: Option<u16>,
    pub logs_window: String,
    /// Now, in seconds since the epoch. Core never reads a clock.
    pub now_unix: u64,
    /// true = list every watched backup on its remote and record the answer
    /// (the nightly round); false = read the recorded answer and list only a
    /// watcher that has none yet (`homelab check`, `homelab today`). See
    /// [`crate::ops::watched`].
    pub watched_fresh: bool,
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
                // free gives total and swap; the containers' config files (the
                // part before the first [snapshot] section, what `pct config`
                // prints, without its 0.45 s start per container) and qm give
                // what is promised.
                "free -m | awk '/^Mem:/{print $2} /^Swap:/{print $3\" \"$2}'; \
                 awk 'FNR==1{skip=0} /^\\[/{skip=1} /^memory:/ && !skip{s+=$2} END{print s+0}' \
                   /etc/pve/lxc/*.conf 2>/dev/null; \
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
        if let (Some(vmid), Some(_status)) = (cols.next(), cols.next())
            && let (Ok(vmid), Some(name)) = (vmid.parse::<u16>(), cols.last())
        {
            out.push((vmid, name.to_string()));
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
    // a half-guarded container reads as guarded. The docker half only where
    // docker runs: a native container (inbox on CT 118) gets the journald
    // cap alone, by design (gap-33), and was reported unguarded forever.
    "if ls /etc/systemd/journald.conf.d/*.conf >/dev/null 2>&1 && ",
    "{ ! command -v docker >/dev/null 2>&1 || grep -q max-size /etc/docker/daemon.json 2>/dev/null; }; ",
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

/// List one watched backup on its remote: the fact, and the record to keep
/// when the listing worked (a failed listing is never recorded).
async fn list_watched(
    exec: &dyn Executor,
    inp: &FactsInputs,
    w: &WatchedBackupSpec,
) -> (
    WatchedBackupFact,
    Option<crate::ops::watched::WatchedRecord>,
) {
    let out = exec
        .run(&Cmd::new(
            "sh",
            &[
                "-c",
                &format!(
                    "rclone lsjson --files-only {} 2>&1 | \
                     sed -n 's/.*\"ModTime\":\"\\([^\"]*\\)\".*/\\1/p' | sort | tail -1",
                    shq(&w.rclone_path)
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
    let mut newest_unix = None;
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
                    && let Ok(t) = d.stdout.trim().parse::<u64>()
                {
                    newest_unix = Some(t);
                    fact.newest_age_s = Some(inp.now_unix.saturating_sub(t));
                }
                if newest_unix.is_none() {
                    // A file whose time could not be read: not recorded,
                    // so the next check asks again rather than keeping
                    // "no files" for a repository that has some.
                    return (fact, None);
                }
            }
        }
    }
    let learned = fact
        .error
        .is_none()
        .then(|| crate::ops::watched::WatchedRecord {
            rclone_path: w.rclone_path.clone(),
            newest_unix,
            learned_at: inp.now_unix,
            source: if inp.watched_fresh {
                "nightly"
            } else {
                "check"
            }
            .to_string(),
        });
    (fact, learned)
}

/// Read off the machine everything `fleetcheck` judges.
pub async fn gather_live_facts(
    exec: &dyn Executor,
    inp: &FactsInputs,
    stack_files: &[(String, u16)],
) -> (LiveFacts, Notes) {
    gather_live_facts_with(exec, inp, stack_files, &|_| {}).await
}

/// [`gather_live_facts`], saying what it is doing as it goes.
///
/// fix-104: `progress` is handed one line per phase and one per container
/// probed; the host sends them to whoever is watching, so a long wait is
/// never silent. Story: `docs/deployment/REGISTER.md`.
pub async fn gather_live_facts_with(
    exec: &dyn Executor,
    inp: &FactsInputs,
    stack_files: &[(String, u16)],
    progress: &(dyn Fn(&str) + Send + Sync),
) -> (LiveFacts, Notes) {
    let mut notes: Notes = Vec::new();
    progress("reading backups, the monitor seeder and the recorded stacks…");

    // O1: what the router (and anything else outside this suite) uploaded
    // last night. Read through the rclone remote that already exists for the
    // restic repositories — no new credential, no new timer.
    // Fleet check speed (2026-09-29): a check reads the answer the host
    // recorded (ops::watched) and lists only a watcher with no record; the
    // nightly round lists them all and records what it saw.
    let watched_fut = async {
        let records = if inp.watched_backups.is_empty() {
            Default::default()
        } else {
            crate::ops::watched::load(exec, &inp.state_dir).await
        };
        let answers: Vec<(
            WatchedBackupFact,
            Option<crate::ops::watched::WatchedRecord>,
        )> = crate::ops::pool::bounded(
            inp.watched_backups
                .iter()
                .map(|w| {
                    let known = (!inp.watched_fresh)
                        .then(|| crate::ops::watched::recorded(&records, &w.name, &w.rclone_path))
                        .flatten()
                        .cloned();
                    async move {
                        match known {
                            Some(r) => (
                                WatchedBackupFact {
                                    name: w.name.clone(),
                                    max_age_s: w.max_age_hours * 3600,
                                    newest_age_s: r
                                        .newest_unix
                                        .map(|t| inp.now_unix.saturating_sub(t)),
                                    ..Default::default()
                                },
                                None,
                            ),
                            None => list_watched(exec, inp, w).await,
                        }
                    }
                })
                .collect(),
            crate::ops::pool::READ_CONCURRENCY,
        )
        .await;
        let mut records = records;
        let mut changed = false;
        let mut facts = Vec::with_capacity(answers.len());
        for (w, (fact, learned)) in inp.watched_backups.iter().zip(answers) {
            if let Some(r) = learned {
                records.insert(w.name.clone(), r);
                changed = true;
            }
            facts.push(fact);
        }
        if changed {
            crate::ops::watched::save(exec, &inp.state_dir, &records).await;
        }
        facts
    };

    // The stacks the host has recorded — read once, used twice below.
    let snapshot_fut = async {
        crate::state::StateStore::new(exec, &inp.state_dir)
            .load()
            .await
            .ok()
    };
    // Three independent reads, overlapped; polled in the order they used to
    // run, so a scripted executor sees the same sequence.
    let (watched, host_memory, snapshot) = futures_util::join!(
        watched_fut,
        // F184: the host's own numbers, so a per-container remedy cannot
        // advise memory the machine does not have.
        read_host_memory(exec),
        snapshot_fut
    );

    let mut facts = LiveFacts {
        stack_files: stack_files.to_vec(),
        watched_backups: watched,
        host_memory,
        ..Default::default()
    };

    // R13: how full are the pools the libraries actually live on. The paths
    // come from the stacks' own `data_mounts`, so this watches what is
    // declared rather than a list somebody keeps in step by hand. One `df`
    // for all of them, keyed by filesystem: two stacks that name different
    // paths on one pool are one pool.
    let pools_fut = async {
        let mut big_logs = None;
        let mut pools = None;
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
                            "printf '%s ' {q}; df -Pk {q} 2>/dev/null | tail -n +2 | head -1; echo",
                            q = shq(p)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("; ");
                // fix-24: oversized logs on the same mounts. Two levels deep only:
                // logs sit at the top of a log mount, and the media pools hold
                // terabytes that a nightly walk has no business descending into.
                let rotated: Vec<String> = snapshot
                    .stacks
                    .values()
                    .filter_map(|st| st.manifest.as_ref())
                    .flat_map(|m| m.data_mounts.iter())
                    .filter(|dm| dm.rotate.is_some())
                    .map(|dm| dm.host_path.clone())
                    .collect();
                let find = format!(
                    "find {} -maxdepth 2 -xdev -type f -name '*.log' -size +{}k -printf '%s %p\\n' 2>/dev/null; true",
                    paths.iter().map(|p| shq(p)).collect::<Vec<_>>().join(" "),
                    crate::ops::fleetcheck::BIG_LOG_BYTES / 1024
                );
                let find_cmd = Cmd::new("sh", &["-c", &find], 120);
                let df_cmd = Cmd::new("sh", &["-c", &script], 60);
                let (found, df) = futures_util::join!(exec.run(&find_cmd), exec.run(&df_cmd));
                if let Ok(out) = found {
                    big_logs = Some(crate::ops::fleetcheck::big_log_facts(&out.stdout, &rotated));
                }
                if let Ok(out) = df {
                    pools = Some(crate::ops::fleetcheck::pool_facts_from_df(
                        &out.stdout,
                        &declared,
                    ));
                }
            }
        }
        (big_logs, pools)
    };

    // fix-26: every storage directory whose owner the stack declares, read
    // back off the disk. The deploy chowns to the declared uid, so a wrong
    // declaration is a service that works until its next restart.
    let owners_fut = async {
        let snapshot = snapshot.as_ref()?;
        let mut declared: Vec<(String, u32, String)> = Vec::new();
        for (name, st) in &snapshot.stacks {
            if let Some(m) = &st.manifest {
                for mount in &m.storage {
                    if let Some(uid) = mount.host_owner_uid {
                        declared.push((mount.host_path.clone(), uid, name.clone()));
                    }
                }
            }
        }
        if declared.is_empty() {
            return None;
        }
        let script = declared
            .iter()
            .map(|(p, _, _)| format!("stat -c '%u %n' {} 2>/dev/null", shq(p)))
            .collect::<Vec<_>>()
            .join("; ");
        let out = exec.run(&Cmd::new("sh", &["-c", &script], 60)).await.ok()?;
        Some(crate::ops::fleetcheck::owner_facts(&out.stdout, &declared))
    };

    // fix-88: each recorded stack's firewall file, read whole off pmxcfs so
    // the check compares bytes with the declaration's rendering. A file that
    // cannot be read counts as absent: the declaration says it should exist,
    // and the finding then says so.
    let fw_targets: Vec<(&String, u16)> = snapshot
        .as_ref()
        .map(|s| {
            s.stacks
                .iter()
                .filter(|(_, st)| !inp.no_touch.contains(&st.vmid))
                .map(|(name, st)| (name, st.vmid))
                .collect()
        })
        .unwrap_or_default();
    let firewalls_fut = crate::ops::pool::bounded(
        fw_targets
            .into_iter()
            .map(|(name, vmid)| async move {
                crate::ops::fleetcheck::FirewallFact {
                    stack: name.clone(),
                    vmid,
                    content: exec.read_file(&crate::firewall::fw_path(vmid)).await.ok(),
                }
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    );

    let pct_list_cmd = Cmd::new("pct", &["list"], 30);
    let ((big_logs, pools), owners, firewalls, listed) = futures_util::join!(
        pools_fut,
        owners_fut,
        firewalls_fut,
        exec.run(&pct_list_cmd)
    );
    if let Some(b) = big_logs {
        facts.big_logs = b;
    }
    if let Some(p) = pools {
        facts.pools = p;
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
    if let Some(o) = owners {
        facts.owners = o;
    }
    facts.firewalls = firewalls;
    if let Ok(out) = listed {
        facts.containers = parse_pct_list(&out.stdout);
    }
    progress("probing every gateway route…");

    // fix-92: every name in the routes directory, whatever its extension, so
    // the check can name a file no stack declares. `*.yml` below would miss
    // the `.bak` that was there on 2026-09-27. Only a listing that ran counts:
    // an unreadable directory is no fact, not an empty one.
    if let Ok(out) = exec
        .run(&crate::executor::attach_sh(
            inp.gateway_vmid,
            &format!("ls -1A '{}'", inp.gateway_routes_dir),
            30,
        ))
        .await
        && out.success()
    {
        facts.route_files = out
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
    }

    // Gateway routes: read every fragment, pull out the address it forwards
    // to, and ask whether anything is listening there. A route that resolves
    // to nothing is only ever found by someone who needs it.
    let gw = inp.gateway_vmid;
    let script = format!(
        "for f in {}/*.yml; do echo \"### $(basename $f)\"; cat \"$f\"; done 2>/dev/null",
        inp.gateway_routes_dir
    );
    if let Ok(out) = exec.run(&crate::executor::attach_sh(gw, &script, 60)).await {
        let mut current = String::new();
        let mut targets: Vec<(String, String)> = Vec::new();
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
                targets.push((current.clone(), target));
            }
        }
        // ops::pool: every route is knocked on at once (bounded), and the
        // facts keep the order the fragments listed them in.
        facts.routes = crate::ops::pool::bounded(
            targets
                .into_iter()
                .map(|(file, target)| async move {
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
                        .run(&crate::executor::attach_sh(gw, &probe, 15))
                        .await
                        .map(|o| o.stdout.contains("up"))
                        .unwrap_or(false);
                    RouteFact {
                        file,
                        target,
                        answered,
                    }
                })
                .collect(),
            crate::ops::pool::READ_CONCURRENCY,
        )
        .await;
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
    progress(&format!(
        "probing {} container(s): disk, memory, logs, guards…",
        managed.len()
    ));
    // ops::pool: the containers are asked a few at a time. The progress line
    // is said as each one ANSWERS, `n` counting answers, so the count climbs
    // steadily whatever order they finish in.
    let done = crate::ops::pool::Done::new(managed.len());
    let done = &done;
    facts.growth = crate::ops::pool::bounded(
        managed
            .iter()
            .map(|(vmid, hostname)| async move {
                let out = exec
                    .run(&crate::executor::attach_sh(*vmid, GROWTH_PROBE, 45))
                    .await;
                progress(&format!("  {} {}", done.tick(), hostname));
                parse_growth(*vmid, hostname, &out.ok()?.stdout)
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await
    .into_iter()
    .flatten()
    .collect();

    progress("reading each container's configuration…");
    // W3: the configured shape of every managed container, from its
    // configuration rather than from inside it — nothing can be asked inside
    // a stopped guest, and the stopped guest is exactly the one to find.
    // Read straight off pmxcfs rather than through `pct config` (0.45 s of
    // Perl start-up each, measured 2026-09-29); the file's main section is
    // what `pct config` prints, so the same values are parsed.
    facts.boot = crate::ops::pool::bounded(
        managed
            .iter()
            .map(|(vmid, hostname)| async move {
                let conf = exec
                    .read_file(&crate::executor::lxc_conf_path(*vmid))
                    .await
                    .ok()?;
                Some(BootFact {
                    vmid: *vmid,
                    hostname: hostname.clone(),
                    live: crate::ops::reconcile::parse(crate::executor::lxc_conf_current(&conf)),
                })
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await
    .into_iter()
    .flatten()
    .collect();

    // fix-150: the patch state, asked inside each running container. One
    // probe, three lines: the upgradable count, the reboot-required mtime or
    // `-`, the unattended-upgrades stamp mtime or `-`. A container that does
    // not answer yields no fact, and no finding.
    progress("asking each container about pending updates…");
    facts.patch = crate::ops::pool::bounded(
        managed
            .iter()
            .map(|(vmid, hostname)| async move {
                let out = exec
                    .run(&crate::executor::attach_sh(*vmid, PATCH_PROBE, 60))
                    .await
                    .ok()?;
                if !out.success() {
                    return None;
                }
                Some(parse_patch_probe(
                    *vmid,
                    hostname,
                    &out.stdout,
                    inp.now_unix,
                ))
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await
    .into_iter()
    .flatten()
    .collect();

    // checks-automate (2026-09-30): every probe the deploys registered,
    // read in its own container. A failed command is a reading too ("could
    // not be read"); the evaluation turns both into findings.
    progress("reading each app's probes…");
    let probe_records = crate::state::StateStore::new(exec, &inp.state_dir)
        .load()
        .await
        .map(|st| st.probes)
        .unwrap_or_default();
    facts.probe_readings = crate::ops::pool::bounded(
        probe_records
            .into_iter()
            .map(|(id, rec)| async move {
                let reading = match exec
                    .run(&crate::executor::attach_sh(
                        rec.vmid,
                        &rec.probe.command,
                        60,
                    ))
                    .await
                {
                    Ok(out) if out.success() => Ok(out.stdout.trim().to_string()),
                    Ok(out) => Err(format!(
                        "exit {}: {}",
                        out.code,
                        out.stderr.trim().chars().take(160).collect::<String>()
                    )),
                    Err(e) => Err(e.to_string().chars().take(160).collect()),
                };
                crate::ops::probes::ProbeReading { id, reading }
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await;

    // Is each stack's safety net actually attached? Both questions are
    // skipped when their address is not configured: an unasked question must
    // never become a finding.
    progress("asking Prometheus and Loki about each stack…");
    let prom = inp.prometheus_url.as_deref();
    let loki = inp.loki_url.as_deref();
    let window = &inp.logs_window;
    // ops::pool: one future per recorded stack, a few at a time, and within a
    // stack the Prometheus and the Loki question side by side.
    let asked: Vec<(&String, &crate::state::StackState)> =
        match (&snapshot, prom.is_some() || loki.is_some()) {
            (Some(snapshot), true) => snapshot.stacks.iter().collect(),
            _ => Vec::new(),
        };
    let coverage_fut = crate::ops::pool::bounded(
        asked
            .into_iter()
            .map(|(name, st)| async move {
                let mut c = CoverageFact {
                    stack: name.clone(),
                    ..Default::default()
                };
                // Phase 9 (inbox): a native service declared unmeasured is
                // not asked about; the check notes the decision instead.
                c.unmeasured_by_choice =
                    !st.natives.is_empty() && st.natives.iter().all(|n| n.metrics == Some(false));
                let scraped_fut = async {
                    let (Some(base), false) = (prom, c.unmeasured_by_choice) else {
                        return None;
                    };
                    let q = format!(
                        "{}/api/v1/query?query=max(up%7Bstack%3D%22{}%22%7D)",
                        base.trim_end_matches('/'),
                        name
                    );
                    Some(
                        exec.run(&Cmd::new("curl", &["-s", "-m", "10", &q], 20))
                            .await
                            .map(|o| o.stdout.contains("\"1\""))
                            .unwrap_or(false),
                    )
                };
                // Every stack ships logs through Alloy since 2026-09-02:
                // compose stacks as container lines labelled
                // `container_name`, native stacks as journal lines labelled
                // `unit`. This used to ask only stacks with an app named
                // `promtail`, which after the migration was none, so the
                // check stayed silent while no container line arrived for
                // 24 days (expert panel, container-logs-missing-in-loki).
                let label = if st.natives.is_empty() {
                    "container_name"
                } else {
                    "unit"
                };
                let logs_fut = async {
                    let base = loki?;
                    // fix-93: the LAN port takes pushes only, so with Loki's
                    // container named the question goes in there, to the
                    // loopback port that carries the full API.
                    let base = match inp.loki_vmid {
                        Some(_) => crate::ops::logshipper::LOKI_QUERY_LOOPBACK,
                        None => base,
                    };
                    // A label matcher is not decoration (F79): lines kept
                    // arriving for months without the label the dashboards
                    // query by. Counting LABELLED lines is the question that
                    // was actually being got wrong.
                    let q = format!(
                        "{}/loki/api/v1/query?query=sum(count_over_time(%7Bstack%3D%22{}%22%2C{}%3D~%22.%2B%22%7D%5B{}%5D))",
                        base.trim_end_matches('/'),
                        name,
                        label,
                        window
                    );
                    let cmd = match inp.loki_vmid {
                        Some(vmid) => {
                            crate::executor::attach_cmd(vmid, &["curl", "-s", "-m", "10", &q], 30)
                        }
                        None => Cmd::new("curl", &["-s", "-m", "10", &q], 20),
                    };
                    Some(
                        exec.run(&cmd)
                            .await
                            .map(|o| o.stdout.contains("\"value\""))
                            .unwrap_or(false),
                    )
                };
                let (scraped, logs_recent) = futures_util::join!(scraped_fut, logs_fut);
                c.scraped = scraped;
                c.logs_recent = logs_recent;
                c
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    );
    facts.coverage = coverage_fut.await;
    // fix-142: the intent copy of every stack the client named, for the
    // repository comparison. Nothing is read when no stack files came (the
    // nightly round).
    let names: Vec<String> = stack_files
        .iter()
        .map(|(d, _)| {
            let d = d.trim_end_matches('/');
            d.rsplit('/').next().unwrap_or(d).to_string()
        })
        .collect();
    facts.intent_files = intent_files(exec, &inp.state_dir, &names).await;
    (facts, notes)
}

/// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift): the host's
/// intent-history copy of each named stack (`<state_dir>/repo/stacks/<name>/`,
/// what the last deploy sent), as container-bound path → sha256.
///
/// A stack whose copy cannot be read is left out rather than entered empty:
/// "no files" would make every file of the stack look new. Names are the
/// client's directory names, so anything but a plain stack name is skipped
/// before it can become a path.
/// fix-150: what runs inside a container to report its patch state.
pub const PATCH_PROBE: &str = "apt-get -s -o Debug::NoLocking=1 upgrade 2>/dev/null | grep -c '^Inst'; \
     stat -c %Y /var/run/reboot-required 2>/dev/null || echo -; \
     stat -c %Y /var/lib/apt/periodic/upgrade-stamp 2>/dev/null || echo -; \
     stat -c %Y /etc/hostname 2>/dev/null || echo -";

/// fix-150: the probe's three lines into a fact. Anything that is not three
/// parseable lines is an unknown (`upgradable: None`), never a finding.
pub fn parse_patch_probe(
    vmid: u16,
    hostname: &str,
    stdout: &str,
    now_unix: u64,
) -> crate::ops::fleetcheck::PatchFact {
    let mut fact = crate::ops::fleetcheck::PatchFact {
        vmid,
        hostname: hostname.to_string(),
        ..Default::default()
    };
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    let (Some(count), Some(reboot), Some(stamp)) = (lines.first(), lines.get(1), lines.get(2))
    else {
        return fact;
    };
    let Ok(upgradable) = count.parse::<u32>() else {
        return fact;
    };
    let age = |s: &str| s.parse::<u64>().ok().map(|t| now_unix.saturating_sub(t));
    fact.upgradable = Some(upgradable);
    fact.reboot_required_age_s = age(reboot);
    fact.unattended_stamp_age_s = age(stamp);
    // The fourth line (container age) is optional: an older probe had three.
    fact.age_s = lines.get(3).and_then(|l| age(l));
    fact
}

pub async fn intent_files(
    exec: &dyn Executor,
    state_dir: &str,
    stacks: &[String],
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> {
    // ops::pool: one listing per stack, a few at a time.
    let read = crate::ops::pool::bounded(
        stacks
            .iter()
            .map(|name| async move {
                let plain = !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
                if !plain {
                    return None;
                }
                let dir = format!("{}/repo/stacks/{}", state_dir, name);
                let Ok(o) = exec
                    .run(&Cmd::new(
                        "sh",
                        &[
                            "-c",
                            &format!("cd '{}' && find . -type f -exec sha256sum {{}} +", dir),
                        ],
                        60,
                    ))
                    .await
                else {
                    return None;
                };
                if !o.success() {
                    return None;
                }
                let files = o
                    .stdout
                    .lines()
                    .filter_map(|l| {
                        let (hash, path) = l.split_once("  ")?;
                        Some((
                            path.trim_start_matches("./").to_string(),
                            hash.trim().to_string(),
                        ))
                    })
                    .collect();
                Some((name.clone(), files))
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await;
    read.into_iter().flatten().collect()
}

/// fix-142 (nightly hash comparison, Kenny's go 2026-10-01): `/opt/<stack>/`
/// hashed from inside each container, for comparison with what the last
/// deploy recorded as pushed (`StackState::pushed_file_hashes`,
/// `fleetcheck::evaluate_container_drift`).
///
/// `stacks` is `(name, vmid)`, same shape as the repository side's
/// `intent_files` takes stack names — the caller passes only the stacks that
/// have something recorded to compare against, so an adopted or
/// never-rebuilt stack is never asked. A container that does not answer, or
/// whose `/opt/<stack>` is not there, is left out rather than entered empty:
/// "no files" would make every pushed file look gone.
pub async fn container_file_hashes(
    exec: &dyn Executor,
    stacks: &[(String, u16)],
) -> std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>> {
    let read = crate::ops::pool::bounded(
        stacks
            .iter()
            .map(|(name, vmid)| async move {
                let script = format!(
                    "cd '/opt/{}' 2>/dev/null && find . -type f -exec sha256sum {{}} +",
                    name
                );
                let o = crate::executor::pct_sh(exec, *vmid, &script, 60)
                    .await
                    .ok()?;
                if !o.success() {
                    return None;
                }
                let files = o
                    .stdout
                    .lines()
                    .filter_map(|l| {
                        let (hash, path) = l.split_once("  ")?;
                        Some((
                            path.trim_start_matches("./").to_string(),
                            hash.trim().to_string(),
                        ))
                    })
                    .collect();
                Some((name.clone(), files))
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await;
    read.into_iter().flatten().collect()
}

/// gap-27: the secret files on a stack's container that have no copy in the
/// host's vault. A compose app's `/opt/<stack>/<app>/.env` is sealed at
/// `<state_dir>/secrets/<stack>/<app>.env`; a native unit's env file at the
/// path the deploy's own `vault_key` names. A file that is not on the
/// container is nothing to seal; a container that cannot be asked yields no
/// finding, because an unasked question must never become one.
pub async fn unsealed_secret_files(
    exec: &dyn Executor,
    state_dir: &str,
    stack: &str,
    st: &crate::state::StackState,
) -> Vec<String> {
    let mut wanted: Vec<(String, String)> = Vec::new();
    for app in &st.apps {
        if st.natives.iter().any(|n| &n.unit == app) {
            continue;
        }
        wanted.push((
            format!("/opt/{}/{}/.env", stack, app),
            format!("{}/secrets/{}/{}.env", state_dir, stack, app),
        ));
    }
    for n in &st.natives {
        if let Some(env) = &n.env_file {
            wanted.push((
                env.clone(),
                format!(
                    "{}/secrets/{}/{}",
                    state_dir,
                    stack,
                    crate::ops::deploy::vault_key(env)
                ),
            ));
        }
    }
    let mut missing = Vec::new();
    for (on_container, in_vault) in wanted {
        let there = exec
            .run(&crate::executor::attach_sh(
                st.vmid,
                &format!(
                    "test -s {} && echo yes || true",
                    crate::ops::util::shq(&on_container)
                ),
                30,
            ))
            .await;
        let Ok(out) = there else { continue };
        if out.stdout.trim() != "yes" {
            continue;
        }
        let sealed = exec
            .read_file(&in_vault)
            .await
            .map(|c| !c.trim().is_empty())
            .unwrap_or(false);
        if !sealed {
            missing.push(on_container);
        }
    }
    missing
}
