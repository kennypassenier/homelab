//! W3: boot policy and resources on a container that already exists.
//!
//! Both are set when a container is created and never looked at again, so
//! after a power cut the fleet boots in whatever order somebody set by hand
//! years ago rather than what the repo says. Kenny's rule that everything
//! behind the edge waits for Traefik and cloudflared lives in the stack
//! files; until now it lived nowhere the machine could read.
//!
//! Deliberately narrow on the writing side. A deploy corrects the boot
//! policy, which is safe at any moment and invisible until the next reboot.
//! It does NOT quietly change memory, cores or disk: raising them has its own
//! deliberate operation (`homelab resize`, raise-only because the kernel
//! cannot take memory back safely), and lowering them is a rebuild. Those
//! divergences are reported instead — the fleet check names them and says
//! which of the two remedies applies.

use crate::manifest::StackManifest;

/// What `pct config` says about the things W3 watches. Every field is
/// optional because a line that is absent means the setting is absent, and
/// that is a different statement from a value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveConfig {
    pub onboot: Option<bool>,
    pub order: Option<u16>,
    pub memory_mb: Option<u32>,
    pub cores: Option<u16>,
}

/// Parse the subset of `pct config <vmid>` this cares about.
/// `startup: order=80,up=30` → order 80; anything unparseable stays None,
/// which is treated as "unknown", never as "differs".
pub fn parse(conf: &str) -> LiveConfig {
    let val = |key: &str| -> Option<String> {
        conf.lines()
            .find_map(|l| l.strip_prefix(&format!("{}:", key)))
            .map(|v| v.trim().to_string())
    };
    LiveConfig {
        onboot: val("onboot").and_then(|v| match v.as_str() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        }),
        order: val("startup").and_then(|v| {
            v.split(',')
                .find_map(|p| p.trim().strip_prefix("order="))
                .and_then(|n| n.parse().ok())
        }),
        memory_mb: val("memory").and_then(|v| v.parse().ok()),
        cores: val("cores").and_then(|v| v.parse().ok()),
    }
}

/// The `pct set` arguments that would bring the boot policy back in line, or
/// an empty vector when it already is.
///
/// A live value that could not be read produces no argument: correcting a
/// setting on the strength of a line we failed to parse is how a check turns
/// into damage.
pub fn boot_set_args(m: &StackManifest, live: &LiveConfig) -> Vec<String> {
    let mut args = Vec::new();
    if live.onboot.is_some() && live.onboot != Some(m.boot.onboot) {
        args.push("--onboot".into());
        args.push(if m.boot.onboot {
            "1".into()
        } else {
            "0".into()
        });
    }
    if let Some(want) = m.boot.order {
        if live.order.is_some() && live.order != Some(want) {
            args.push("--startup".into());
            args.push(format!("order={}", want));
        }
    }
    args
}

/// Human-readable divergences, for a report rather than a repair. Boot
/// policy is included so the fleet check can name it before a deploy fixes
/// it; resources are here because nothing else will say them out loud.
pub fn divergences(m: &StackManifest, live: &LiveConfig) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(live_onboot) = live.onboot {
        if live_onboot != m.boot.onboot {
            out.push(format!(
                "starts on boot: {} on the machine, {} in the stack file",
                yes_no(live_onboot),
                yes_no(m.boot.onboot)
            ));
        }
    }
    if let (Some(live_order), Some(want)) = (live.order, m.boot.order) {
        if live_order != want {
            out.push(format!(
                "boot order: {} on the machine, {} in the stack file",
                live_order, want
            ));
        }
    }
    if let Some(mem) = live.memory_mb {
        if mem != m.resources.memory_mb {
            out.push(format!(
                "memory: {} MB on the machine, {} MB in the stack file",
                mem, m.resources.memory_mb
            ));
        }
    }
    if let Some(cores) = live.cores {
        if cores != m.resources.cores {
            out.push(format!(
                "cores: {} on the machine, {} in the stack file",
                cores, m.resources.cores
            ));
        }
    }
    out
}

fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}
