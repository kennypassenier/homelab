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
                detail: "no sealed .env".into(),
                remedy: Some(format!(
                    "provide {}'s .env; deploy fails closed without it (A3)",
                    s.name
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
                "refresh the rclone Google Drive token (E5); local backups still run".into()
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
