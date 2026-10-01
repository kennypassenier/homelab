//! Doctor / self-diagnosis (F6): each check is a pure function over injected
//! probe data, so the whole health matrix is unit-testable. The host gathers
//! the probes; core decides healthy/warn/fail and the remediation hint.
//!
//! ## First run, on a host with nothing deployed yet (fix-130)
//!
//! A host right after its daemon starts for the first time — `managed_stacks`
//! empty, no host-meta snapshot, no restore drill, no probes read yet —
//! reports [`Health::Warn`] at worst from that emptiness, never
//! [`Health::Fail`]: "never snapshotted" and "never proved a restore" are
//! facts about a clock that has not ticked yet, not about anything broken.
//! The one thing doctor still fails hard on before a single stack exists is
//! the restic password file: bootstrapping it is a one-time manual step that
//! has to happen before the first backup can be written at all, and a
//! daemon that cannot report that plainly would let someone deploy for days
//! before finding out backups were never possible. `managed_stacks` being
//! empty also means every per-stack line (`stack <name> backup`,
//! `stack <name> env`) is silent rather than printed as a Fail about a stack
//! that is not there yet — an unasked question is never a finding, the same
//! rule probes and manual checks hold. See
//! `fix_130_a_fresh_host_with_nothing_deployed_never_fails` for the case
//! this pins.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Ok,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub name: String,
    pub health: Health,
    pub detail: String,
    pub remedy: Option<String>,
}

/// Everything doctor needs, gathered by the host (I/O stays outside core).
#[derive(Debug, Clone, Default)]
pub struct Probes {
    pub host_disk_free_pct: Option<u64>,
    /// fix-113 ADDENDUM (owner + chassis-rs, 2026-10-01): free % on the
    /// chassis backup staging directory's filesystem — only asked, and only
    /// a finding, when `native_backup_staging_dir` is set; unset is not a
    /// finding, it just means chassis-paused backups skip staging.
    /// `Some(None)`: configured but unreadable. `None`: not configured.
    pub staging_disk_free_pct: Option<Option<u64>>,
    /// fix-62 (restore-drill-covers-almost-nothing, 2026-10-01): free % on
    /// the restore drill's scratch directory's filesystem — always asked,
    /// since (unlike staging) the drill always has somewhere configured to
    /// restore to (a default applies when host.toml names none).
    pub restore_scratch_disk_free_pct: Option<u64>,
    pub state_parses: bool,
    pub managed_stacks: Vec<StackProbe>,
    pub offsite_configured: bool,
    pub offsite_token_valid: bool,
    pub mirror_behind: Option<u32>,
    pub interrupted_ops: Vec<String>,
    /// Host units on pve that differ from what the binary carries
    /// (`crate::hostunits`); `None` when not asked.
    pub host_units_drift: Option<Vec<String>>,
    /// fix-120: connections the daemon refused for their token since it
    /// started; `None` when not asked.
    pub failed_auth: Option<FailedAuth>,
    // fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): doctor
    // answered backups and the Drive token only. Each probe below is `None`
    // when not asked, so an older caller keeps its old report.
    /// Where the daemon listens and whether remote exec is on.
    pub exposure: Option<Exposure>,
    /// Private files readable by group or others, as `path (mode)`.
    pub loose_files: Option<Vec<String>>,
    /// Privileged containers, and those among them the host policy
    /// (`privileged_vmids`, fix-120) does not name.
    pub privileged: Option<Privileged>,
    /// Age of the host-meta snapshot (vault, state, TLS, intent repo).
    pub host_meta: Option<Freshness>,
    /// The restore drill: when one last proved a restore, and what fails.
    pub restore_drill: Option<DrillProbe>,
    /// The restic password file exists and is not empty.
    pub password_file_ok: Option<bool>,
    /// Google Drive's space, from `rclone about`.
    pub drive: Option<DriveSpace>,
    /// fix-130 (second half, 2026-10-01): names from the gateway's routes
    /// directory that no stack's deploy recorded — the same judgement the
    /// fleet check carries (`fleetcheck::unowned_route_files`), read here so
    /// `homelab doctor`/`today` says it too without waiting for a nightly
    /// round. `None` when the gateway could not be listed; `Some(vec![])`
    /// when it could and every file is owned.
    pub unowned_route_files: Option<Vec<String>>,
}

/// fix-130: how the daemon faces the network.
#[derive(Debug, Clone, Default)]
pub struct Exposure {
    pub listen: String,
    pub exec_enabled: bool,
}

/// fix-130: privileged containers on the host.
#[derive(Debug, Clone, Default)]
pub struct Privileged {
    pub vmids: Vec<u16>,
    pub outside_policy: Vec<u16>,
}

/// fix-130: how old something is; `age_h: None` = it never happened.
#[derive(Debug, Clone, Default)]
pub struct Freshness {
    pub age_h: Option<u64>,
}

/// fix-130: the restore drill's standing.
#[derive(Debug, Clone, Default)]
pub struct DrillProbe {
    /// Hours since a drill last proved a restore; None = never.
    pub age_h: Option<u64>,
    /// How long a passed drill counts for, in hours.
    pub interval_h: u64,
    /// Repositories whose last drill proved nothing, with the reason.
    pub failing: Vec<String>,
}

/// fix-130: space on the offsite remote, in bytes.
#[derive(Debug, Clone, Default)]
pub struct DriveSpace {
    pub total: u64,
    pub free: u64,
    /// In the trash: pruned packs Drive still counts against the quota.
    pub trashed: u64,
}

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): what the 401
/// counter saw.
#[derive(Debug, Clone, Default)]
pub struct FailedAuth {
    pub count: u64,
    pub last_peer: Option<String>,
    /// Unix time of the last refusal; 0 when there was none.
    pub last_at: u64,
}

#[derive(Debug, Clone)]
pub struct StackProbe {
    pub name: String,
    /// Hours since the last successful backup, if any.
    pub backup_age_h: Option<u64>,
    /// Container exists at the expected vmid.
    pub container_present: bool,
    pub env_sealed: bool,
    /// fix-130: the stack declares nothing to back up (every mount
    /// `no_backup` or `no_data`, no native service), so its backup age says
    /// nothing either way.
    pub nothing_to_back_up: bool,
}

pub fn diagnose(p: &Probes) -> Vec<Check> {
    let mut checks = Vec::new();

    checks.push(match p.host_disk_free_pct {
        Some(free) if free < 10 => Check {
            name: "host disk".into(),
            health: Health::Fail,
            detail: format!("{}% free", free),
            remedy: Some(
                "free space on pve-root; runaway guards (B2) cap logs but check backups/images"
                    .into(),
            ),
        },
        Some(free) if free < 20 => Check {
            name: "host disk".into(),
            health: Health::Warn,
            detail: format!("{}% free", free),
            remedy: Some("getting tight — review disk usage soon".into()),
        },
        Some(free) => Check {
            name: "host disk".into(),
            health: Health::Ok,
            detail: format!("{}% free", free),
            remedy: None,
        },
        None => Check {
            name: "host disk".into(),
            health: Health::Warn,
            detail: "unknown".into(),
            remedy: Some("could not read host disk usage".into()),
        },
    });

    // fix-113 ADDENDUM: only a finding when staging is configured — an
    // unconfigured staging directory is the normal, default state and must
    // not nag every night.
    if let Some(staging) = p.staging_disk_free_pct {
        checks.push(match staging {
            Some(free) if free < 10 => Check {
                name: "backup staging disk".into(),
                health: Health::Fail,
                detail: format!("{}% free", free),
                remedy: Some(
                    "free space on the chassis backup staging pool — chassis-paused native \
                     backups will skip staging and fall back to a live, held-open pause"
                        .into(),
                ),
            },
            Some(free) if free < 20 => Check {
                name: "backup staging disk".into(),
                health: Health::Warn,
                detail: format!("{}% free", free),
                remedy: Some(
                    "getting tight — chassis-paused native backups may start skipping staging"
                        .into(),
                ),
            },
            Some(free) => Check {
                name: "backup staging disk".into(),
                health: Health::Ok,
                detail: format!("{}% free", free),
                remedy: None,
            },
            None => Check {
                name: "backup staging disk".into(),
                health: Health::Warn,
                detail: "unknown".into(),
                remedy: Some("could not read the staging directory's free space".into()),
            },
        });
    }

    checks.push(match p.restore_scratch_disk_free_pct {
        Some(free) if free < 10 => Check {
            name: "restore drill scratch disk".into(),
            health: Health::Fail,
            detail: format!("{}% free", free),
            remedy: Some(
                "free space on the restore drill's scratch pool (restore_drill_scratch_dir) \
                 — the nightly drill cannot restore a repository without room to put it"
                    .into(),
            ),
        },
        Some(free) if free < 20 => Check {
            name: "restore drill scratch disk".into(),
            health: Health::Warn,
            detail: format!("{}% free", free),
            remedy: Some("getting tight — the drill restores one repository a night".into()),
        },
        Some(free) => Check {
            name: "restore drill scratch disk".into(),
            health: Health::Ok,
            detail: format!("{}% free", free),
            remedy: None,
        },
        None => Check {
            name: "restore drill scratch disk".into(),
            health: Health::Warn,
            detail: "unknown".into(),
            remedy: Some("could not read the restore drill scratch directory's free space".into()),
        },
    });

    checks.push(Check {
        name: "state file".into(),
        health: if p.state_parses {
            Health::Ok
        } else {
            Health::Fail
        },
        detail: if p.state_parses {
            "parses".into()
        } else {
            "unreadable/corrupt".into()
        },
        remedy: (!p.state_parses).then(|| {
            "inspect /var/lib/homelab/state.json; restore from a backup if corrupt".into()
        }),
    });

    for s in &p.managed_stacks {
        if !s.container_present {
            checks.push(Check {
                name: format!("stack {}", s.name),
                health: Health::Fail,
                detail: "container missing".into(),
                remedy: Some(format!(
                    "redeploy {} — auto-restore (E3) refills config",
                    s.name
                )),
            });
            continue;
        }
        if !s.env_sealed {
            checks.push(Check {
                name: format!("stack {} env", s.name),
                health: Health::Fail,
                detail: "a secret file on the container has no copy in the host's vault".into(),
                remedy: Some(format!(
                    "redeploy {} (`homelab deploy stacks/{}`, or `homelab adopt stacks/{}` for an \
                     adopted service) so the vault takes a copy; without it a lost container \
                     cannot get its secrets back without latch (gap-27)",
                    s.name, s.name, s.name
                )),
            });
        }
        if s.nothing_to_back_up {
            // fix-130: `registry backup — last backup 12h ago` read as a
            // protected stack for a cache that is deliberately not kept.
            checks.push(Check {
                name: format!("stack {} backup", s.name),
                health: Health::Ok,
                detail: "nothing to back up (declared): every mount no_backup or no_data".into(),
                remedy: None,
            });
            continue;
        }
        match s.backup_age_h {
            Some(h) if h > 48 => checks.push(Check {
                name: format!("stack {} backup", s.name),
                health: Health::Warn,
                detail: format!("last backup {}h ago", h),
                remedy: Some("run a backup; the scheduler (E4) may be stalled".into()),
            }),
            None => checks.push(Check {
                name: format!("stack {} backup", s.name),
                health: Health::Warn,
                detail: "never backed up".into(),
                remedy: Some("run the first backup for this stack".into()),
            }),
            Some(h) => checks.push(Check {
                name: format!("stack {} backup", s.name),
                health: Health::Ok,
                detail: format!("last backup {}h ago", h),
                remedy: None,
            }),
        }
    }

    if p.offsite_configured {
        checks.push(Check {
            name: "offsite (Drive)".into(),
            health: if p.offsite_token_valid {
                Health::Ok
            } else {
                Health::Fail
            },
            detail: if p.offsite_token_valid {
                "token valid".into()
            } else {
                "token invalid/expired".into()
            },
            remedy: (!p.offsite_token_valid).then(|| {
                // gap-27: it added "local backups still run", but every
                // repository lives behind rclone on Google Drive: with the
                // token dead, no backup runs at all until it is refreshed.
                "refresh the rclone Google Drive token (E5); until then no backup can be written"
                    .into()
            }),
        });
    }

    if let Some(behind) = p.mirror_behind {
        checks.push(Check {
            name: "github mirror".into(),
            health: if behind == 0 {
                Health::Ok
            } else {
                Health::Warn
            },
            detail: if behind == 0 {
                "up to date".into()
            } else {
                format!("{} commit(s) behind", behind)
            },
            remedy: (behind > 0)
                .then(|| "mirror push is retrying in the background (non-blocking)".into()),
        });
    }

    if let Some(drift) = p.host_units_drift.as_ref() {
        checks.push(Check {
            name: "host units".into(),
            health: if drift.is_empty() {
                Health::Ok
            } else {
                Health::Warn
            },
            detail: if drift.is_empty() {
                "the daemon's units match the ones this binary carries".into()
            } else {
                format!("differ from this binary's copy: {}", drift.join(", "))
            },
            remedy: (!drift.is_empty()).then(|| {
                "the next self-update puts them back (`homelab release-update`); \
                 until then the watchdog or the rollback may be missing"
                    .into()
            }),
        });
    }

    if let Some(fa) = &p.failed_auth {
        checks.push(if fa.count == 0 {
            Check {
                name: "refused connections".into(),
                health: Health::Ok,
                detail: "no connection refused for its token since the daemon started".into(),
                remedy: None,
            }
        } else {
            Check {
                name: "refused connections".into(),
                health: Health::Warn,
                detail: format!(
                    "{} connection(s) refused for a missing or wrong token since the daemon \
                     started, the last from {} at {} {:02}:{:02} UTC",
                    fa.count,
                    fa.last_peer.as_deref().unwrap_or("?"),
                    crate::state::ymd(fa.last_at),
                    fa.last_at % 86_400 / 3600,
                    fa.last_at % 3600 / 60
                ),
                remedy: Some(
                    "a machine of yours with an old token, or something probing the daemon: \
                     `journalctl -u homelab-host | grep '401 on'` lists each one with its \
                     address"
                        .into(),
                ),
            }
        });
    }

    checks.extend(security_and_recovery(p));

    if !p.interrupted_ops.is_empty() {
        checks.push(Check {
            name: "interrupted operations".into(),
            health: Health::Warn,
            detail: p.interrupted_ops.join(", "),
            remedy: Some("re-run the listed operation(s); re-running is always safe (B1)".into()),
        });
    }

    checks
}

/// fix-130 (expert panel, doctor-checks-too-little, 2026-09-27): the lines
/// for the probes doctor did not have: how the daemon faces the network,
/// file modes, privileged containers, the host-meta snapshot, the restore
/// drill, the restic password file and Drive's space.
fn security_and_recovery(p: &Probes) -> Vec<Check> {
    let mut checks = Vec::new();
    let ok = |name: &str, detail: String| Check {
        name: name.into(),
        health: Health::Ok,
        detail,
        remedy: None,
    };
    let bad = |name: &str, health: Health, detail: String, remedy: &str| Check {
        name: name.into(),
        health,
        detail,
        remedy: Some(remedy.into()),
    };

    if let Some(e) = &p.exposure {
        // Informational: where it listens is a decision (the rescue leg on
        // VLAN 10), not a fault doctor can judge.
        checks.push(ok(
            "daemon exposure",
            format!(
                "listens on {}; remote exec {}",
                e.listen,
                if e.exec_enabled { "on" } else { "off" }
            ),
        ));
    }

    if let Some(loose) = &p.loose_files {
        checks.push(if loose.is_empty() {
            ok(
                "file modes",
                "config, secrets, TLS key and records are root-only".into(),
            )
        } else {
            bad(
                "file modes",
                Health::Warn,
                format!("readable beyond root: {}", loose.join(", ")),
                "chmod go-rwx on each (0600 files, 0700 directories); the daemon fixes its \
                 own records at start (fix-125), the rest were made by hand",
            )
        });
    }

    if let Some(pv) = &p.privileged {
        let list = |v: &[u16]| {
            v.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };
        checks.push(if pv.outside_policy.is_empty() {
            ok(
                "privileged containers",
                if pv.vmids.is_empty() {
                    "none".into()
                } else {
                    format!("{}, all named in privileged_vmids", list(&pv.vmids))
                },
            )
        } else {
            bad(
                "privileged containers",
                Health::Warn,
                format!(
                    "{} privileged but not in host.toml's privileged_vmids",
                    list(&pv.outside_policy)
                ),
                "a privileged container is root on the host: name it in privileged_vmids if \
                 it is meant, otherwise rebuild it unprivileged",
            )
        });
    }

    if let Some(hm) = &p.host_meta {
        checks.push(match hm.age_h {
            Some(h) if h <= 48 => ok("host-meta backup", format!("last snapshot {}h ago", h)),
            age => bad(
                "host-meta backup",
                Health::Warn,
                match age {
                    Some(h) => format!("last snapshot {}h ago", h),
                    None => "never snapshotted".into(),
                },
                "the vault (with the only restic password), state and TLS are in it: \
                 `homelab backup-host-meta`, then see why the nightly one did not run",
            ),
        });
    }

    if let Some(d) = &p.restore_drill {
        let overdue = d.age_h.is_none_or(|h| h > d.interval_h);
        let when = match d.age_h {
            Some(h) => format!("last proved a restore {} days ago", h / 24),
            None => "never proved a restore".into(),
        };
        checks.push(if !overdue && d.failing.is_empty() {
            ok("restore drill", when)
        } else {
            let mut detail = when;
            if !d.failing.is_empty() {
                detail.push_str(&format!("; failing: {}", d.failing.join("; ")));
            }
            bad(
                "restore drill",
                Health::Warn,
                detail,
                "a backup nobody restored is a hypothesis: read the failure, fix the \
                 repository, and let the nightly drill pass (it takes one repository a night)",
            )
        });
    }

    if let Some(usable) = p.password_file_ok {
        checks.push(if usable {
            ok("restic password file", "present".into())
        } else {
            bad(
                "restic password file",
                Health::Fail,
                "missing or empty".into(),
                "no backup can be written or read without it: restore it from Bitwarden \
                 into the configured restic_password_file, 0600",
            )
        });
    }

    if let Some(unowned) = &p.unowned_route_files {
        checks.push(if unowned.is_empty() {
            ok(
                "gateway route files",
                "every file in the routes directory is declared by a stack".into(),
            )
        } else {
            bad(
                "gateway route files",
                Health::Warn,
                format!("no stack declares: {}", unowned.join(", ")),
                "declare it in the stack it belongs to (gateway_route, or extra_routes to keep \
                 its name) and deploy that stack, or delete it by hand on the gateway; homelab \
                 never removes a route file no deploy wrote",
            )
        });
    }

    if let Some(d) = &p.drive {
        const GIB: u64 = 1 << 30;
        let pct = (d.free.saturating_mul(100))
            .checked_div(d.total)
            .unwrap_or(0);
        let detail = format!(
            "{} GiB free of {} GiB ({}%); {} GiB in the trash",
            d.free / GIB,
            d.total / GIB,
            pct,
            d.trashed / GIB
        );
        let remedy = "empty Drive's trash (pruned packs stay there and count against the \
                      quota: `rclone cleanup gdrive:`), then look at retention or the plan";
        checks.push(match pct {
            p if p < 5 => bad("Drive space", Health::Fail, detail, remedy),
            p if p < 10 => bad("Drive space", Health::Warn, detail, remedy),
            _ => ok("Drive space", detail),
        });
    }

    checks
}

pub fn overall(checks: &[Check]) -> Health {
    if checks.iter().any(|c| c.health == Health::Fail) {
        Health::Fail
    } else if checks.iter().any(|c| c.health == Health::Warn) {
        Health::Warn
    } else {
        Health::Ok
    }
}
