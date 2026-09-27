//! Doctor / self-diagnosis (F6): each check is a pure function over injected
//! probe data, so the whole health matrix is unit-testable. The host gathers
//! the probes; core decides healthy/warn/fail and the remediation hint.

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

pub fn overall(checks: &[Check]) -> Health {
    if checks.iter().any(|c| c.health == Health::Fail) {
        Health::Fail
    } else if checks.iter().any(|c| c.health == Health::Warn) {
        Health::Warn
    } else {
        Health::Ok
    }
}
