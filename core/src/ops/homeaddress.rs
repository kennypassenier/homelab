//! Keep the house's own public address in a CrowdSec whitelist on the
//! gateway.
//!
//! fix-94: since fix-45 CrowdSec sees the real visitor address, so a burst
//! of the house's own traffic can get the house banned and the Traefik
//! bouncer then answers 403 to every name from home. Story:
//! `docs/deployment/REGISTER.md`.
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
use crate::executor::{Cmd, Executor, pct_sh};
use crate::ops::OpCtx;
use crate::ops::fleetcheck::{Finding, Severity};
use crate::ops::util::{push_content, shq};
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;
use crate::state::{HostState, StateStore};

/// The device-backup entry that is the router. Reused rather than configured
/// a second time: its URL names the router's API, its credential file and pin
/// are what the house already trusts to talk to it, and a second entry for
/// the same router would be a second secret to keep in step (Kenny's rule:
/// never a new secret for this).
pub const ROUTER_DEVICE: &str = "opnsense";

/// Where the whitelist lives and how it is tested and reloaded comes from
/// the stack that declares `home_address_whitelist:` (app-knowledge,
/// 2026-09-30); the gateway stack does, for CrowdSec. A file of its own rather
/// than a line in the app's static whitelist: the repository's copy is laid
/// over that one on every deploy, and the address would vanish until the next
/// check put it back.
use crate::manifest::HomeAddressWhitelist;

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
        } else if in_ip
            && let Some(item) = t.strip_prefix("- ")
            && let Ok(a) = item.trim().trim_matches('"').parse()
        {
            return Some(a);
        }
    }
    None
}

/// fix-156: the house's address is what a public service sees this host
/// come from, not what the router reports (fix-94's router read never
/// worked on pve). Story: `docs/deployment/REGISTER.md`.
pub const PUBLIC_SOURCES: &[(&str, &str)] = &[
    ("cloudflare", "https://cloudflare.com/cdn-cgi/trace"),
    ("ipify", "https://api.ipify.org"),
];

/// Cloudflare's trace (`key=value` lines, `ip=` among them) or a bare
/// address (ipify); a public IPv4 address, or why not.
pub fn parse_public(body: &str) -> Result<Ipv4Addr, String> {
    let raw = body
        .lines()
        .find_map(|l| l.trim().strip_prefix("ip="))
        .unwrap_or(body.trim());
    let addr: Ipv4Addr = raw.trim().parse().map_err(|_| {
        format!(
            "the answer is not an IPv4 address: {:?}",
            raw.chars().take(60).collect::<String>()
        )
    })?;
    if addr.is_private() || addr.is_loopback() || addr.is_link_local() || addr.is_unspecified() {
        return Err(format!("{} is not a public address", addr));
    }
    Ok(addr)
}

/// Ask the public sources in order; the first public address wins.
async fn read_public(exec: &dyn Executor) -> Result<Ipv4Addr, String> {
    let mut why = Vec::new();
    for (name, url) in PUBLIC_SOURCES {
        let out = exec
            .run(&Cmd::new(
                "curl",
                &["-sS", "--fail", "--max-time", "10", url],
                20,
            ))
            .await;
        match out {
            Ok(o) if o.success() => match parse_public(&o.stdout) {
                Ok(a) => return Ok(a),
                Err(e) => why.push(format!("{}: {}", name, e)),
            },
            Ok(o) => why.push(format!(
                "{}: curl exit {}: {}",
                name,
                o.code,
                o.stderr.trim()
            )),
            Err(e) => why.push(format!("{}: {}", name, e)),
        }
    }
    Err(why.join("; "))
}

/// Write the new whitelist, have CrowdSec test it, and reload. A whitelist
/// the test refuses is put back to what it was, with no reload sent.
async fn write_and_reload(
    exec: &dyn Executor,
    gw: u16,
    wl: &HomeAddressWhitelist,
    before: &str,
    want: &str,
) -> Result<StepOutcome, CoreError> {
    push_content(exec, gw, &wl.file, want, "644").await?;
    let test = pct_sh(exec, gw, &wl.test, 120).await?;
    if !test.success() {
        let restored = if before.is_empty() {
            pct_sh(exec, gw, &format!("rm -f {}", shq(&wl.file)), 30)
                .await
                .map(|o| o.success())
                .unwrap_or(false)
        } else {
            push_content(exec, gw, &wl.file, before, "644")
                .await
                .is_ok()
        };
        return Err(CoreError::Other(format!(
            "the whitelist's configuration test refused the new file ({}); {} :: remedy: run \
             `{}` in the container and read what it says",
            format!("{} {}", test.stdout.trim(), test.stderr.trim()).trim(),
            if restored {
                "the previous file is back and no reload was sent"
            } else {
                "putting the previous file back FAILED too, and the app may not start with \
                 this one — remove it by hand"
            },
            wl.test
        )));
    }
    let hup = pct_sh(exec, gw, &wl.reload, 30).await?;
    if !hup.success() {
        return Err(CoreError::Other(format!(
            "the whitelist is written but its app could not be told to reload ({}) :: \
             remedy: check the app that reads it; it reads the file when it next starts",
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
            format!(
                "[home-address] could not record the home-address check: {}",
                e
            ),
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
pub async fn sync_home_address(ctx: &OpCtx<'_>) -> OperationReport {
    let mut runner = Runner::new("home-address-whitelist", ctx.sink, ctx.journal);
    let exec = ctx.exec;
    // app-knowledge: the stack that declares the whitelist, and its container.
    let declared = StateStore::new(exec, &ctx.state_dir)
        .load()
        .await
        .ok()
        .and_then(|st| {
            st.stacks.values().find_map(|s| {
                let m = s.manifest.as_ref()?;
                Some((m.vmid, m.home_address_whitelist.clone()?))
            })
        });
    let Some((gw, wl)) = declared else {
        runner.log(
            Level::Info,
            "[home-address] no stack declares home_address_whitelist; nothing to keep".to_string(),
        );
        return runner.finish_ok();
    };

    let before = match pct_sh(
        exec,
        gw,
        &format!("cat {} 2>/dev/null || true", shq(&wl.file)),
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

    let addr = match read_public(exec).await {
        Ok(a) => a,
        Err(why) => {
            runner.log(
                Level::Warn,
                format!(
                    "[home-address] could not read the house's public address ({}) — the whitelist \
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
                "[home-address] home address {} unchanged; whitelist left alone",
                addr
            ),
        );
        record(ctx, &runner, Some(addr), None).await;
        return runner.finish_ok();
    }

    const STEP: &str = "whitelist the home address";
    match runner
        .step(STEP, || write_and_reload(exec, gw, &wl, &before, &want))
        .await
    {
        Ok(_) => {
            runner.log(
                Level::Info,
                match kept {
                    Some(old) if old != addr => format!(
                        "[home-address] home address changed from {} to {} — whitelist rewritten, \
                         the app reloaded",
                        old, addr
                    ),
                    Some(_) => format!(
                        "[home-address] home address {} rewritten in the current format — the app \
                         reloaded",
                        addr
                    ),
                    None => format!(
                        "[home-address] home address {} whitelisted (none was before) — the app \
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

/// The fleet check's view. A whitelist running on a last known address is
/// worth seeing (Noted); no address at all is Broken since fix-156: the
/// dashboard's second lock then refuses the house, and CrowdSec can ban it.
pub fn evaluate_home_address(state: &HostState) -> Vec<Finding> {
    let Some(why) = &state.home_address_error else {
        return Vec::new();
    };
    vec![Finding {
        severity: if state.home_address.is_some() {
            Severity::Noted
        } else {
            Severity::Broken
        },
        subject: "home address whitelist".into(),
        what: format!(
            "the last check ({}) could not keep the house's address current ({}); the \
             whitelist {}",
            crate::state::ymd(state.home_address_checked),
            why,
            match &state.home_address {
                Some(a) => format!("keeps {}, the last known address", a),
                None => "holds no home address, so the house can be blocked and the admin \
                         dashboard refuses it"
                    .to_string(),
            }
        ),
        remedy: "nothing is removed while this stands; the host asks again after the next \
                 gateway deploy, every night and at its own start. Check that pve reaches \
                 https://cloudflare.com/cdn-cgi/trace or https://api.ipify.org"
            .to_string(),
    }]
}
