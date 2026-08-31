//! W1: does the host actually have what a stack asks for?
//!
//! v1 checked the machine before it deployed; this generation hands the
//! devices in and assumes they exist. The failure that produces is the one
//! that hides itself: a container with `gpu: true` on a host with no card
//! comes up perfectly, and the first sign is a film transcoding on the CPU
//! at nine in the evening (F54).
//!
//! The group ids are the second half of the same problem. They were written
//! as the literals 44 and 104 — correct on this host today, and silently
//! wrong on any host where `video` and `render` were numbered differently.
//! A gid that does not match means the device node is there and unreadable,
//! which looks exactly like the device being absent.

use crate::error::CoreError;
use crate::executor::{Cmd, Executor};

/// The device nodes a `gpu: true` stack is handed, and the groups that own
/// them on this host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuDevices {
    pub card: String,
    pub card_gid: u32,
    pub render: String,
    pub render_gid: u32,
}

const CARD: &str = "/dev/dri/card0";
const RENDER: &str = "/dev/dri/renderD128";
const TUN: &str = "/dev/net/tun";

/// One shell round trip that answers both questions for every path: is it
/// there, and who owns it. A missing path reports itself rather than making
/// the whole command fail, so the error can name which one is missing and
/// what the host does have instead.
fn probe_cmd(paths: &[&str]) -> Cmd {
    let script = format!(
        "for d in {}; do if [ -e \"$d\" ]; then echo \"$d $(stat -c %g \"$d\")\"; \
         else echo \"$d MISSING\"; fi; done; echo \"dri: $(ls /dev/dri 2>/dev/null | tr '\\n' ' ')\"",
        paths.join(" ")
    );
    Cmd::new("sh", &["-c", &script], 30)
}

fn field(stdout: &str, path: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{} ", path)))
        .map(|v| v.trim().to_string())
}

/// What the host has in `/dev/dri`, for the error message. Empty means the
/// directory itself is missing, which is the no-GPU-at-all case.
fn dri_contents(stdout: &str) -> String {
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("dri:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "nothing".into())
}

/// Refuse a GPU stack the host cannot serve, and read the group ids instead
/// of assuming them.
pub async fn check_gpu(exec: &dyn Executor, stack: &str) -> Result<GpuDevices, CoreError> {
    let out = exec.run(&probe_cmd(&[CARD, RENDER])).await?;
    let mut gids = Vec::new();
    for path in [CARD, RENDER] {
        match field(&out.stdout, path).as_deref() {
            Some("MISSING") | None => {
                return Err(CoreError::SafetyAbort(format!(
                    "stack '{}' asks for a GPU but this host has no {} (in /dev/dri: {}) :: \
                     deploying anyway gives a container that starts and transcodes on the CPU",
                    stack,
                    path,
                    dri_contents(&out.stdout)
                )));
            }
            Some(g) => match g.parse::<u32>() {
                Ok(gid) => gids.push(gid),
                Err(_) => {
                    return Err(CoreError::SafetyAbort(format!(
                        "stack '{}': could not read the group of {} (got '{}') :: \
                         a wrong gid hands over a device the container cannot open",
                        stack, path, g
                    )))
                }
            },
        }
    }
    Ok(GpuDevices {
        card: CARD.into(),
        card_gid: gids[0],
        render: RENDER.into(),
        render_gid: gids[1],
    })
}

/// Same for the VPN flag: without `/dev/net/tun` the container starts and
/// gluetun fails inside it, where nothing the orchestrator watches is
/// looking.
pub async fn check_tun(exec: &dyn Executor, stack: &str) -> Result<(), CoreError> {
    let out = exec.run(&probe_cmd(&[TUN])).await?;
    match field(&out.stdout, TUN).as_deref() {
        Some("MISSING") | None => Err(CoreError::SafetyAbort(format!(
            "stack '{}' asks for VPN passthrough but this host has no {} :: \
             the container would start and its tunnel would not",
            stack, TUN
        ))),
        Some(_) => Ok(()),
    }
}

/// `--dev0`/`--dev1` arguments built from what the host reported.
pub fn dev_args(g: &GpuDevices) -> (String, String) {
    (
        format!("{},gid={}", g.card, g.card_gid),
        format!("{},gid={}", g.render, g.render_gid),
    )
}

/// M1: a data mount must already exist on the host. The orchestrator never
/// creates one — that is the whole point of the separate list — so a missing
/// path is a fact about the machine, not something to fix by making a
/// directory.
///
/// It refuses rather than warns because of what the alternative looks like:
/// `pct set` would happily create an empty directory, the container would
/// start, Jellyfin would come up with an empty library and every *arr root
/// folder would report itself missing. A rebuild that silently loses the
/// media libraries is the exact failure this check exists to prevent.
pub async fn check_data_mounts(
    exec: &dyn Executor,
    stack: &str,
    mounts: &[crate::manifest::DataMount],
) -> Result<(), CoreError> {
    if mounts.is_empty() {
        return Ok(());
    }
    let paths: Vec<&str> = mounts.iter().map(|m| m.host_path.as_str()).collect();
    let script = paths
        .iter()
        .map(|p| {
            format!(
                "if [ -d \"{}\" ]; then echo \"{} OK\"; else echo \"{} MISSING\"; fi",
                p, p, p
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 30)).await?;
    let missing: Vec<&str> = paths
        .iter()
        .filter(|p| !out.stdout.contains(&format!("{} OK", p)))
        .copied()
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(CoreError::SafetyAbort(format!(
        "stack '{}' declares data_mounts this host does not have: {} :: these are \
         directories the orchestrator does not create on purpose, so a missing one \
         means the wrong host or a pool that is not imported — deploying anyway gives \
         a container whose libraries are empty",
        stack,
        missing.join(", ")
    )))
}
