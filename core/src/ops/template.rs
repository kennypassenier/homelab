//! Golden template build (B8): bake docker + the runaway guards +
//! unattended-upgrades into a reusable Proxmox *template container*
//! (`debian-12-homelab-vN`), so new stacks provision in seconds via
//! `pct clone` instead of a full apt bootstrap. The deploy bootstrap remains
//! the source of truth and still runs over clones — it just skips everything.
//!
//! Safety: the builder owns exactly ONE temp vmid; its stop/destroy path
//! refuses to touch anything else, and the no-touch list applies as always.

use crate::error::CoreError;
use crate::executor::{pct_sh, run_ok, Cmd, Executor, TracingExecutor};
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

pub struct TemplateCfg {
    /// The vmid the builder may create AND destroy. Nothing else, ever.
    pub temp_vmid: u16,
    /// Base OS template to build from (a vztmpl path).
    pub base_template: String,
    /// Version tag: the result is named `debian-12-homelab-v{version}`.
    pub version: u32,
    pub storage: String,
    /// Build-time network (needs internet for apt/get.docker.com).
    pub bridge: String,
    pub ip: String,
    pub gateway: String,
    pub vlan: Option<u16>,
    pub features: String,
    /// O5/O2: `pct clone` cannot change a privilege level, so a container that
    /// must be privileged has to be cloned from a privileged template. Two
    /// templates therefore exist, and the name says which is which — nothing
    /// else about a template tells you.
    pub unprivileged: bool,
}

impl Default for TemplateCfg {
    fn default() -> Self {
        Self {
            temp_vmid: 999,
            base_template: "local:vztmpl/debian-12-standard_12.12-1_amd64.tar.zst".into(),
            version: 1,
            storage: "local-lvm".into(),
            bridge: "vmbr0".into(),
            ip: "10.10.10.99/24".into(),
            gateway: "10.10.10.1".into(),
            vlan: Some(10),
            features: "nesting=1,keyctl=1".into(),
            unprivileged: true,
        }
    }
}

pub async fn build_template(ctx: &OpCtx<'_>, cfg: &TemplateCfg) -> OperationReport {
    // The suffix is not decoration: a clone silently inherits its template's
    // privilege level, so telling the two apart at a glance is the difference
    // between a deploy that works and one that fails on permissions later.
    let name = format!(
        "debian-12-homelab-v{}{}",
        cfg.version,
        if cfg.unprivileged { "" } else { "-priv" }
    );
    let mut runner = Runner::new("template-build", ctx.sink, ctx.journal);
    let texec = TracingExecutor::new(ctx.exec, ctx.sink);
    let exec: &dyn Executor = &texec;
    let vm = cfg.temp_vmid.to_string();

    runner.log(
        Level::Info,
        format!(
            "[template] building {} on temp vmid {}",
            name, cfg.temp_vmid
        ),
    );

    // The temp vmid must be safe to own: never on the no-touch list, and not
    // an existing guest (we will destroy it at the end).
    step!(runner, "claim temp vmid", {
        if ctx.safety.no_touch.contains(&cfg.temp_vmid) {
            return Err(CoreError::SafetyAbort(format!(
                "temp vmid {} is on the no-touch list",
                cfg.temp_vmid
            )));
        }
        let existing = exec.run(&Cmd::new("pct", &["config", &vm], 30)).await?;
        if existing.success() {
            return Err(CoreError::SafetyAbort(format!(
                "vmid {} already exists — refusing to build over it",
                cfg.temp_vmid
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    step!(runner, "create build container", {
        let mut net = format!(
            "name=eth0,bridge={},firewall=0,ip={},gw={}",
            cfg.bridge, cfg.ip, cfg.gateway
        );
        if let Some(tag) = cfg.vlan {
            net.push_str(&format!(",tag={}", tag));
        }
        let rootfs = format!("{}:4", cfg.storage);
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "create",
                    &vm,
                    &cfg.base_template,
                    "--hostname",
                    &name,
                    "--rootfs",
                    &rootfs,
                    "--net0",
                    &net,
                    "--memory",
                    "1024",
                    "--cores",
                    "2",
                    "--unprivileged",
                    if cfg.unprivileged { "1" } else { "0" },
                    "--features",
                    &cfg.features,
                    "--description",
                    "homelab golden template build (B8) — temporary",
                ],
                300,
            ),
        )
        .await?;
        run_ok(exec, &Cmd::new("pct", &["start", &vm], 120)).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "wait for systemd", {
        for _ in 0..30 {
            let out = pct_sh(
                exec,
                cfg.temp_vmid,
                "systemctl is-system-running 2>/dev/null || true",
                20,
            )
            .await?;
            let s = out.stdout.trim();
            if s.contains("running") || s.contains("degraded") {
                return Ok(StepOutcome::Unchanged);
            }
            exec.sleep_ms(4000).await;
        }
        Err(CoreError::Other("build container never came up".into()))
    });

    step!(runner, "bake docker", {
        let install = pct_sh(
            exec,
            cfg.temp_vmid,
            "export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq curl ca-certificates && curl -fsSL https://get.docker.com | sh",
            900,
        )
        .await?;
        if !install.success() {
            return Err(CoreError::Command {
                rendered: "bake docker".into(),
                detail: install.stderr,
            });
        }
        Ok(StepOutcome::Changed)
    });

    // O2: node_exporter, cadvisor and promtail on every container, from the
    // template rather than per stack. They were installed by hand on six hosts
    // on 2026-08-29, which is precisely the work this removes — and the reason
    // a container added after that date measured nothing until someone noticed.
    step!(runner, "bake observability agents", {
        let install = pct_sh(
            exec,
            cfg.temp_vmid,
            "export DEBIAN_FRONTEND=noninteractive; apt-get install -y -qq prometheus-node-exporter && systemctl enable prometheus-node-exporter",
            600,
        )
        .await?;
        if !install.success() {
            return Err(CoreError::Command {
                rendered: "bake node_exporter".into(),
                detail: install.stderr,
            });
        }
        // cadvisor and promtail are containers, so the template only needs
        // their compose files and images pulled; the deploy brings them up.
        // Port 8081, not cadvisor's own 8080: gluetun already publishes 8080
        // on the downloader stack, and one uniform port keeps the scrape
        // config to a single pattern.
        let stage = pct_sh(
            exec,
            cfg.temp_vmid,
            "mkdir -p /opt/cadvisor && docker pull gcr.io/cadvisor/cadvisor:latest && docker pull grafana/promtail:3.0.0",
            900,
        )
        .await?;
        if !stage.success() {
            return Err(CoreError::Command {
                rendered: "pre-pull agent images".into(),
                detail: stage.stderr,
            });
        }
        Ok(StepOutcome::Changed)
    });

    step!(runner, "bake guards", {
        crate::ops::guards::apply(exec, ctx.sink, cfg.temp_vmid).await?;
        Ok(StepOutcome::Changed)
    });

    // Measured on a clone of the first v2 template (2026-08-31) rather than
    // assumed: two of these are real defects and none of them are about RAM.
    step!(runner, "trim what cannot work in a container", {
        let script = concat!(
            // A fresh clone booted `degraded` because of two hardware services
            // the Debian template pulls in and an LXC can never satisfy. That
            // matters beyond tidiness: the deploy's wait-for-systemd loop
            // accepts "degraded", so a template that is degraded by default
            // makes the word useless as a signal.
            "systemctl mask nvmf-autoconnect.service openipmi.service >/dev/null 2>&1; ",
            // Postfix listens on 127.0.0.1:25 in every container and nothing
            // sends mail from one. Worth ~8 MB, which is not the reason —
            // an unused network daemon on ten containers is.
            "export DEBIAN_FRONTEND=noninteractive; ",
            "apt-get purge -y -qq postfix >/dev/null 2>&1; ",
            // Docker plugins nobody uses: ~120 MB of disk per container.
            "apt-get purge -y -qq docker-buildx-plugin docker-model-plugin docker-ce-rootless-extras >/dev/null 2>&1; ",
            "apt-get autoremove -y -qq >/dev/null 2>&1; true"
        );
        let _ = pct_sh(exec, cfg.temp_vmid, script, 600).await?;
        Ok(StepOutcome::Changed)
    });

    // A clone must not inherit machine identity or apt debris. The ssh host
    // keys go with it — every clone must generate its own — but deleting them
    // without arranging for that left sshd FAILED on every container built
    // from the first v2 template. Found by cloning it and looking, not by
    // reading the step.
    step!(runner, "generalize", {
        let unit = "[Unit]\n             Description=Generate ssh host keys on first boot\n             ConditionPathExists=!/etc/ssh/ssh_host_ed25519_key\n             Before=ssh.service\n             [Service]\n             Type=oneshot\n             ExecStart=/usr/bin/ssh-keygen -A\n             RemainAfterExit=yes\n             [Install]\n             WantedBy=multi-user.target\n";
        let script = format!(
            "cat > /etc/systemd/system/ssh-host-keys.service <<'UNIT'\n{}UNIT\n             systemctl enable ssh-host-keys.service >/dev/null 2>&1; \
             apt-get clean && rm -f /etc/machine-id /var/lib/dbus/machine-id && \
             touch /etc/machine-id && rm -f /etc/ssh/ssh_host_*",
            unit
        );
        let _ = pct_sh(exec, cfg.temp_vmid, &script, 120).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "convert to template", {
        run_ok(exec, &Cmd::new("pct", &["stop", &vm], 120)).await?;
        run_ok(exec, &Cmd::new("pct", &["template", &vm], 300)).await?;
        run_ok(
            exec,
            &Cmd::new(
                "pct",
                &[
                    "set",
                    &vm,
                    "--description",
                    &format!(
                        "{} — golden template (B8), clone with template: \"clone:{}\"",
                        name, cfg.temp_vmid
                    ),
                ],
                30,
            ),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    runner.log(
        Level::Info,
        format!(
            "[template] {} ready — set template: \"clone:{}\" in StackDefaults/manifests for fast provisioning",
            name, cfg.temp_vmid
        ),
    );
    runner.finish_ok()
}
