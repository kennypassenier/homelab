//! Hot-apply resources (C4): raise RAM/cores/disk on a RUNNING container
//! straight from the manifest. Shrinking while running is refused (the
//! kernel can't take memory back safely; disk shrink is never safe) — the
//! remedy is stop + redeploy, or accept the change at the next recreate.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd, Executor, TracingExecutor};
use crate::manifest::StackManifest;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

use super::OpCtx;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

fn conf_u32(conf: &str, key: &str) -> Option<u32> {
    conf.lines()
        .find(|l| l.starts_with(&format!("{}:", key)))
        .and_then(|l| l.split(':').nth(1))
        .and_then(|v| v.trim().parse().ok())
}

/// Rootfs size in GB from a `rootfs: local-lvm:vm-108-disk-0,size=8G` line.
fn conf_disk_gb(conf: &str) -> Option<u32> {
    conf.lines()
        .find(|l| l.starts_with("rootfs:"))
        .and_then(|l| l.split("size=").nth(1))
        .map(|s| s.trim_end_matches(['G', '\n']))
        .and_then(|v| v.parse().ok())
}

pub async fn hot_apply(ctx: &OpCtx<'_>, m: &StackManifest) -> OperationReport {
    let op = format!("resize-{}", m.stack_name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let vm = m.vmid.to_string();

    let mut running = false;
    let mut cur_mem = 0u32;
    let mut cur_cores = 0u32;
    let mut cur_disk = 0u32;

    step!(runner, "read live config", {
        crate::manifest::validate_manifest(m)?;
        if ctx.safety.no_touch.contains(&m.vmid) {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} is on the no-touch list",
                m.vmid
            )));
        }
        let cfg = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if !cfg.success() {
            return Err(CoreError::Other(format!("vmid {} does not exist", m.vmid)));
        }
        // A2: same hostname guard as every other mutation.
        let live_host = cfg
            .stdout
            .lines()
            .find(|l| l.starts_with("hostname:"))
            .map(|l| l.trim_start_matches("hostname:").trim().to_string())
            .unwrap_or_default();
        if live_host != m.hostname {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} is '{}', expected '{}'",
                m.vmid, live_host, m.hostname
            )));
        }
        cur_mem = conf_u32(&cfg.stdout, "memory").unwrap_or(0);
        cur_cores = conf_u32(&cfg.stdout, "cores").unwrap_or(0);
        cur_disk = conf_disk_gb(&cfg.stdout).unwrap_or(0);
        let status = exec.run(&Cmd::new("pct", &["status", &vm], 30)).await?;
        running = status.stdout.contains("running");
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "apply ram + cores", {
        let want_mem = m.resources.memory_mb;
        let want_cores = m.resources.cores as u32;
        if running && (want_mem < cur_mem || want_cores < cur_cores) {
            return Err(CoreError::SafetyAbort(format!(
                "shrink refused while running ({}→{} MiB, {}→{} cores) — stop the container first or redeploy",
                cur_mem, want_mem, cur_cores, want_cores
            )));
        }
        if want_mem == cur_mem && want_cores == cur_cores {
            return Ok(StepOutcome::Unchanged);
        }
        let mem = want_mem.to_string();
        let cores = want_cores.to_string();
        let swap = m.resources.swap_mb.to_string();
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "set", &vm, "--memory", &mem, "--cores", &cores, "--swap", &swap,
                ],
                60,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "apply disk", {
        let want = m.resources.disk_gb;
        if want < cur_disk {
            return Err(CoreError::SafetyAbort(format!(
                "disk shrink refused ({}G → {}G) — never safe on a live filesystem",
                cur_disk, want
            )));
        }
        if want == cur_disk || cur_disk == 0 {
            return Ok(StepOutcome::Unchanged);
        }
        let size = format!("{}G", want);
        run_ok(
            exec,
            &Cmd::new("pct", &["resize", &vm, "rootfs", &size], 120),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!(
            "[resize] {} now {} MiB / {} cores / {}G (live, no restart)",
            m.stack_name, m.resources.memory_mb, m.resources.cores, m.resources.disk_gb
        ),
    );
    runner.finish_ok()
}
