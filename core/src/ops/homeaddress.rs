//! Keep the house's own public address in a CrowdSec whitelist on the
//! gateway.
//!
//! fix-94 (Kenny, triage 2026-09-27, crowdsec-home-ip: "Thuisadres
//! automatisch vrijstellen"). Until fix-45 CrowdSec saw every kp-soft.dev
//! request as 172.18.0.1, the docker bridge the tunnel delivers through, and
//! that range is whitelisted — so it could ban nobody, the house included.
//! Since fix-45 it sees the real visitor, and a request Kenny made from home
//! on 2026-09-27 was logged as 62.235.8.143: the router's WAN address. A
//! burst of the house's own traffic can now get the house banned, and the
//! Traefik bouncer then answers 403 to every name from home.
//!
//! The address is dynamic, so it cannot live in the static `whitelists.yaml`
//! in the repository. The router knows it; the orchestrator already asks the
//! router for its configuration every night (route A, `devicebackup`), and
//! this asks the same router, with the same credential and the same pin, for
//! its WAN address. **VM 100 stays untouched**: one read-only GET.
//!
//! The whitelist file is the memory. What it names is the last known
//! address, so an address that cannot be read changes nothing and removes
//! nothing — it becomes a noted finding instead.

use std::net::Ipv4Addr;

use crate::error::CoreError;
use crate::executor::{pct_sh, Cmd, Executor};
use crate::ops::devicebackup::{tls_args, DeviceBackup};
use crate::ops::fleetcheck::{Finding, Severity};
use crate::ops::util::{push_content, shq};
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;
use crate::state::{HostState, StateStore};

/// The device-backup entry that is the router. Reused rather than configured
/// a second time: its URL names the router's API, its credential file and pin
/// are what the house already trusts to talk to it, and a second entry for
/// the same router would be a second secret to keep in step (Kenny's rule:
/// never a new secret for this).
pub const ROUTER_DEVICE: &str = "opnsense";

/// Where the whitelist lives, as a path inside the gateway container.
///
/// `/appdata/gateway/crowdsec-config` is CrowdSec's `/etc/crowdsec`
/// (stacks/gateway/crowdsec/docker-compose.yml), and every file in
/// `parsers/s02-enrich/` is loaded as a parser — which is how the static
/// `whitelists.yaml` is loaded too, bind-mounted beside this one. A file of
/// its own rather than a line in that one: the repository's copy is laid over
/// the static file on every deploy, and the address would vanish until the
/// next check put it back.
pub const WHITELIST_FILE: &str =
    "/appdata/gateway/crowdsec-config/parsers/s02-enrich/homelab-home-address.yaml";

/// OPNsense's interface overview: every interface with its addresses. The
/// WAN row carries the public address when the router holds it itself, which
/// it does here (measured 2026-09-27: `wan` on vtnet1, 62.235.8.143/21).
const INTERFACES_PATH: &str = "/api/interfaces/overview/interfacesInfo";

/// The same pair CrowdSec's own systemd unit runs for `reload`: test the
/// configuration, then HUP the running process. The test matters because
/// CrowdSec exits on a reload it cannot load, and the bouncer fails closed —
/// a bad whitelist would turn into 403 for every name in the house.
const CONFIG_TEST: &str = "docker exec crowdsec crowdsec -c /etc/crowdsec/config.yaml -t -error";
const RELOAD: &str = "docker kill --signal=HUP crowdsec";

/// The router among the configured device backups, if there is one. None =
/// the feature is off: there is nothing to ask and nothing to report.
pub fn router(devices: &[DeviceBackup]) -> Option<&DeviceBackup> {
    devices.iter().find(|d| d.name == ROUTER_DEVICE)
}

/// `https://10.10.10.1/api/core/backup/download/this` → the interface list
/// on the same host.
fn interfaces_url(device_url: &str) -> Option<String> {
    let scheme_end = device_url.find("://")? + 3;
    let host_end = device_url[scheme_end..]
        .find('/')
        .map(|i| scheme_end + i)
        .unwrap_or(device_url.len());
    Some(format!("{}{}", &device_url[..host_end], INTERFACES_PATH))
}

/// The router's WAN address from its interface list, if it is one the
/// internet can see the house as.
///
/// A private or carrier-grade address on the WAN means a modem or the ISP
/// holds the public one in front of the router. Whitelisting what the router
/// has would then exempt nobody while the log said it had, so it is refused.
/// IPv4 only: the WAN has no IPv6 address (measured 2026-09-27).
pub fn wan_address(json: &str) -> Result<Ipv4Addr, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| {
        "the answer is not JSON (a login page or an error page is a plausible 200)".to_string()
    })?;
    let rows = v.get("rows").and_then(|r| r.as_array()).ok_or_else(|| {
        "the answer has no interface list (`rows`) — has the API changed?".to_string()
    })?;
    let wan = rows
        .iter()
        .find(|r| r.get("identifier").and_then(|i| i.as_str()) == Some("wan"))
        .ok_or_else(|| "the router reports no WAN interface".to_string())?;
    let with_prefix = wan
        .get("addr4")
        .and_then(|a| a.as_str())
        .filter(|a| !a.is_empty())
        .or_else(|| {
            wan.get("ipv4")
                .and_then(|l| l.as_array())
                .and_then(|l| l.first())
                .and_then(|a| a.get("ipaddr"))
                .and_then(|a| a.as_str())
        })
        .ok_or_else(|| "the WAN interface has no IPv4 address (link down?)".to_string())?;
    let bare = with_prefix.split('/').next().unwrap_or(with_prefix);
    let addr: Ipv4Addr = bare
        .parse()
        .map_err(|_| format!("the WAN address `{}` does not parse", with_prefix))?;
    let o = addr.octets();
    let carrier_grade = o[0] == 100 && (o[1] & 0xc0) == 64;
    if addr.is_private()
        || carrier_grade
        || addr.is_loopback()
        || addr.is_link_local()
        || addr.is_unspecified()
        || addr.is_broadcast()
        || addr.is_documentation()
        || addr.is_multicast()
    {
        return Err(format!(
            "the WAN address {} is not a public address — the house's public one sits on \
             a device in front of the router, which this cannot read",
            addr
        ));
    }
    Ok(addr)
}

/// The whitelist, as CrowdSec reads it. No timestamp in it on purpose: the
/// content changes only when the address does, so an unchanged address
/// rewrites and reloads nothing.
pub fn whitelist_yaml(addr: Ipv4Addr) -> String {
    format!(
        "# Written by the homelab orchestrator (fix-94); do not edit. Every gateway\n\
         # deploy and every nightly round rewrite it from the router's WAN address.\n\
         name: homelab/home-address\n\
         description: \"The house's own public address, read from the router\"\n\
         whitelist:\n  \
         reason: \"The house's own public address (router WAN)\"\n  \
         ip:\n    \
         - \"{}\"\n",
        addr
    )
}

/// The address a whitelist file names under `ip:`, if any.
pub fn whitelisted_address(yaml: &str) -> Option<Ipv4Addr> {
    let mut in_ip = false;
    for line in yaml.lines() {
        let t = line.trim();
        if t == "ip:" {
            in_ip = true;
        } else if t.ends_with(':') {
            in_ip = false;
        } else if in_ip {
            if let Some(item) = t.strip_prefix("- ") {
                if let Ok(a) = item.trim().trim_matches('"').parse() {
                    return Some(a);
                }
            }
        }
    }
    None
}

/// Ask the router for its WAN address, with the device backup's credential.
async fn read_wan(exec: &dyn Executor, router: &DeviceBackup) -> Result<Ipv4Addr, String> {
    let url = interfaces_url(&router.url)
        .ok_or_else(|| format!("no router address in `{}`", router.url))?;
    let mut args: Vec<String> = vec![
        "-sS".into(),
        "--fail-with-body".into(),
        "--max-time".into(),
        "20".into(),
    ];
    args.extend(tls_args(router));
    // `-K`: the credential reaches curl through its config file, never argv
    // (rule 10, the same reason as the device backup).
    args.extend(["-K".to_string(), router.cred_file.clone(), url]);
    let argv: Vec<&str> = args.iter().map(String::as_str).collect();
    // Quiet: the answer lists every interface, address and MAC in the house,
    // and the transcript needs only the one address this takes from it.
    let out = exec
        .run(&Cmd::new("curl", &argv, 30).quiet())
        .await
        .map_err(|e| e.to_string())?;
    if !out.success() {
        return Err(format!("curl exit {}: {}", out.code, out.stderr.trim()));
    }
    wan_address(&out.stdout)
}

/// Write the new whitelist, have CrowdSec test it, and reload. A whitelist
/// the test refuses is put back to what it was, with no reload sent.
async fn write_and_reload(
    exec: &dyn Executor,
    gw: u16,
    before: &str,
    want: &str,
) -> Result<StepOutcome, CoreError> {
    push_content(exec, gw, WHITELIST_FILE, want, "644").await?;
    let test = pct_sh(exec, gw, CONFIG_TEST, 120).await?;
    if !test.success() {
        let restored = if before.is_empty() {
            pct_sh(exec, gw, &format!("rm -f {}", shq(WHITELIST_FILE)), 30)
                .await
                .map(|o| o.success())
                .unwrap_or(false)
        } else {
            push_content(exec, gw, WHITELIST_FILE, before, "644")
                .await
                .is_ok()
        };
        return Err(CoreError::Other(format!(
            "CrowdSec's configuration test refused the new whitelist ({}); {} :: remedy: run \
             `{}` on the gateway and read what it says",
            format!("{} {}", test.stdout.trim(), test.stderr.trim()).trim(),
            if restored {
                "the previous file is back and no reload was sent"
            } else {
                "putting the previous file back FAILED too, and CrowdSec will not start with \
                 this one — remove it by hand"
            },
            CONFIG_TEST
        )));
    }
    let hup = pct_sh(exec, gw, RELOAD, 30).await?;
    if !hup.success() {
        return Err(CoreError::Other(format!(
            "the whitelist is written but CrowdSec could not be told to reload ({}) :: \
             remedy: check the crowdsec container on the gateway; it reads the file when it \
             next starts",
            hup.stderr.trim()
        )));
    }
    Ok(StepOutcome::Changed)
}

/// Record what the whitelist holds and whether the address could be read.
async fn record(ctx: &OpCtx<'_>, runner: &Runner<'_>, kept: Option<Ipv4Addr>, err: Option<String>) {
    let now = ctx.now_unix;
    let res = StateStore::new(ctx.exec, &ctx.state_dir)
        .update(|s| {
            s.home_address = kept.map(|a| a.to_string());
            s.home_address_checked = now;
            s.home_address_error = err;
        })
        .await;
    if let Err(e) = res {
        runner.log(
            Level::Warn,
            format!("[crowdsec] could not record the home-address check: {}", e),
        );
    }
}

/// Read the router's WAN address and keep the gateway's whitelist equal to
/// it: rewritten and CrowdSec reloaded only when it changed, and said so.
///
/// Runs after every gateway deploy and once in every nightly round (the
/// address is dynamic). An address that cannot be read is not a failed
/// operation: a failed operation notifies, and the whitelist still holds the
/// last known address. It is a noted finding instead.
pub async fn sync_home_address(ctx: &OpCtx<'_>, router: &DeviceBackup) -> OperationReport {
    let mut runner = Runner::new("crowdsec-home-address", ctx.sink, ctx.journal);
    let exec = ctx.exec;
    let gw = ctx.safety.gateway_vmid;

    let before = match pct_sh(
        exec,
        gw,
        &format!("cat {} 2>/dev/null || true", shq(WHITELIST_FILE)),
        30,
    )
    .await
    {
        Ok(out) if out.success() => out.stdout,
        Ok(out) => {
            let e = CoreError::Other(format!(
                "cannot read the whitelist on the gateway (vmid {}): {} :: remedy: is the \
                 gateway container running?",
                gw,
                out.stderr.trim()
            ));
            return runner.finish_err("read the whitelist", &e);
        }
        Err(e) => return runner.finish_err("read the whitelist", &e),
    };
    let kept = whitelisted_address(&before);

    let addr = match read_wan(exec, router).await {
        Ok(a) => a,
        Err(why) => {
            runner.log(
                Level::Warn,
                format!(
                    "[crowdsec] could not read the router's WAN address ({}) — the whitelist \
                     {}; nothing was removed",
                    why,
                    match kept {
                        Some(a) => format!("keeps {}", a),
                        None => "holds no home address".to_string(),
                    }
                ),
            );
            record(ctx, &runner, kept, Some(why)).await;
            return runner.finish_ok();
        }
    };

    let want = whitelist_yaml(addr);
    if before == want {
        runner.log(
            Level::Info,
            format!(
                "[crowdsec] home address {} unchanged; whitelist left alone",
                addr
            ),
        );
        record(ctx, &runner, Some(addr), None).await;
        return runner.finish_ok();
    }

    const STEP: &str = "whitelist the home address";
    match runner
        .step(STEP, || write_and_reload(exec, gw, &before, &want))
        .await
    {
        Ok(_) => {
            runner.log(
                Level::Info,
                match kept {
                    Some(old) if old != addr => format!(
                        "[crowdsec] home address changed from {} to {} — whitelist rewritten, \
                         CrowdSec reloaded",
                        old, addr
                    ),
                    Some(_) => format!(
                        "[crowdsec] home address {} rewritten in the current format — CrowdSec \
                         reloaded",
                        addr
                    ),
                    None => format!(
                        "[crowdsec] home address {} whitelisted (none was before) — CrowdSec \
                         reloaded",
                        addr
                    ),
                },
            );
            record(ctx, &runner, Some(addr), None).await;
            runner.finish_ok()
        }
        Err(e) => {
            record(
                ctx,
                &runner,
                kept,
                Some(format!("the whitelist for {} was not applied: {}", addr, e)),
            )
            .await;
            runner.finish_err(STEP, &e)
        }
    }
}

/// The fleet check's view: a whitelist running on a last known address is
/// worth seeing, not worth waking anyone for (Noted, Kenny's triage).
pub fn evaluate_home_address(state: &HostState) -> Vec<Finding> {
    let Some(why) = &state.home_address_error else {
        return Vec::new();
    };
    vec![Finding {
        severity: Severity::Noted,
        subject: "crowdsec home address".into(),
        what: format!(
            "the last check ({}) could not keep the house's address current ({}); the \
             whitelist {}",
            crate::state::ymd(state.home_address_checked),
            why,
            match &state.home_address {
                Some(a) => format!("keeps {}, the last known address", a),
                None => "holds no home address, so CrowdSec can ban the house".to_string(),
            }
        ),
        remedy: format!(
            "nothing is removed while this stands, and the next gateway deploy or nightly \
             round tries again. If the house's address changed meanwhile, check that the \
             `{}` device_backups credential may read `{}`",
            ROUTER_DEVICE, INTERFACES_PATH
        ),
    }]
}
