//! Runaway guards (B2) + unattended security updates (A7): every managed
//! container gets hard caps on everything that grows unattended. Idempotent —
//! configs are only pushed (and services only restarted) on content change.

use crate::error::CoreError;
use crate::executor::{pct_sh, Executor};
use crate::ops::util::push_content;
use crate::sink::{Level, PipelineEvent, Sink};

/// Docker's log settings, applied to every managed container.
///
/// `max-size` and `max-file` are the cap that G1 rolled out fleet-wide.
///
/// `tag` was added 2026-08-31 and is the fix for a three-year-old kind of
/// silence. Every promtail config in this repo has always tried to read the
/// container's name out of the log line's `attrs.name`, and the three Loki
/// dashboards were written against the `container_name` label that pipeline
/// was meant to produce. Docker writes no `attrs` field at all unless it is
/// asked to, so the extraction found nothing, the empty label was dropped,
/// and the dashboards queried a label that had never once existed. Nothing
/// reported it — an empty dashboard and a working one look the same until
/// somebody needs it, which is how Kenny found it.
///
/// `{{.Name}}` makes docker write `"attrs":{"tag":"jellyfin"}` on every line.
/// The option is read when a container is CREATED, so an existing container
/// keeps logging untagged until it is recreated.
pub const DOCKER_DAEMON_JSON: &str = r#"{
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3",
    "tag": "{{.Name}}"
  }
}
"#;

pub const JOURNALD_LIMITS: &str =
    "[Journal]\nSystemMaxUse=100M\nRuntimeMaxUse=50M\nMaxRetentionSec=1month\n";

pub const LOGROTATE_POLICY: &str = r#"/var/log/syslog /var/log/messages /var/log/auth.log {
    daily
    rotate 7
    maxsize 50M
    missingok
    notifempty
    compress
    delaycompress
    sharedscripts
    postrotate
        /usr/lib/rsyslog/rsyslog-rotate 2>/dev/null || true
    endscript
}
"#;

pub const APT_AUTOCLEAN: &str =
    "APT::Periodic::AutocleanInterval \"7\";\nAPT::Periodic::CleanInterval \"7\";\n";

pub const UNATTENDED_UPGRADES: &str = r#"Unattended-Upgrade::Allowed-Origins {
    "${distro_id}:${distro_codename}-security";
};
Unattended-Upgrade::Automatic-Reboot "false";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
"#;

pub const PRUNE_SERVICE: &str = "[Unit]\nDescription=Prune stale Docker data (homelab runaway guard)\n\n[Service]\nType=oneshot\nExecStart=/usr/bin/docker system prune -f --filter until=168h\n";

pub const PRUNE_TIMER: &str = "[Unit]\nDescription=Weekly Docker prune (homelab runaway guard)\n\n[Timer]\nOnCalendar=weekly\nRandomizedDelaySec=1h\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n";

/// `docker` = false for a container that runs no docker at all: it gets the
/// journald cap and nothing else. Installing the docker guards there put a
/// weekly prune timer on CT 109 and CT 112 that has been failing ever since,
/// which is worse than useless — a guard that fails every week teaches you to
/// ignore failures.
pub async fn apply(
    exec: &dyn Executor,
    sink: &dyn Sink,
    vmid: u16,
    docker: bool,
) -> Result<(), CoreError> {
    let log = |msg: String| {
        sink.emit(PipelineEvent::Line {
            level: Level::Info,
            source: "HOST".into(),
            msg,
        })
    };

    // 1. Docker container logs — must land before app containers (re)start.
    if docker
        && push_content(
            exec,
            vmid,
            "/etc/docker/daemon.json",
            DOCKER_DAEMON_JSON,
            "644",
        )
        .await?
    {
        pct_sh(exec, vmid, "systemctl restart docker", 120).await?;
        log("[guard] docker log caps applied (10m x 3)".into());
    }

    // 2. systemd journal caps.
    if push_content(
        exec,
        vmid,
        "/etc/systemd/journald.conf.d/homelab-limits.conf",
        JOURNALD_LIMITS,
        "644",
    )
    .await?
    {
        pct_sh(exec, vmid, "systemctl restart systemd-journald", 60).await?;
        log("[guard] journald capped at 100M / 1 month".into());
    }

    // 3. Classic syslog rotation.
    pct_sh(
        exec,
        vmid,
        "command -v logrotate >/dev/null || (export DEBIAN_FRONTEND=noninteractive; apt-get install -y -qq logrotate)",
        300,
    )
    .await?;
    push_content(
        exec,
        vmid,
        "/etc/logrotate.d/homelab",
        LOGROTATE_POLICY,
        "644",
    )
    .await?;

    // 4. Weekly docker prune timer — only where there is docker to prune.
    if docker {
        push_content(
            exec,
            vmid,
            "/etc/systemd/system/docker-prune.service",
            PRUNE_SERVICE,
            "644",
        )
        .await?;
        if push_content(
            exec,
            vmid,
            "/etc/systemd/system/docker-prune.timer",
            PRUNE_TIMER,
            "644",
        )
        .await?
        {
            pct_sh(
                exec,
                vmid,
                "systemctl daemon-reload && systemctl enable --now docker-prune.timer",
                60,
            )
            .await?;
            log("[guard] weekly docker prune timer armed".into());
        }
    }

    // 5. apt cache hygiene.
    push_content(
        exec,
        vmid,
        "/etc/apt/apt.conf.d/60homelab-clean",
        APT_AUTOCLEAN,
        "644",
    )
    .await?;

    // 6. Security patches (A7): unattended-upgrades, security-only, no reboot.
    pct_sh(
        exec,
        vmid,
        "command -v unattended-upgrade >/dev/null || (export DEBIAN_FRONTEND=noninteractive; apt-get install -y -qq unattended-upgrades)",
        300,
    )
    .await?;
    push_content(
        exec,
        vmid,
        "/etc/apt/apt.conf.d/50unattended-upgrades",
        UNATTENDED_UPGRADES,
        "644",
    )
    .await?;
    pct_sh(
        exec,
        vmid,
        "systemctl enable --now unattended-upgrades 2>/dev/null || true",
        60,
    )
    .await?;
    pct_sh(exec, vmid, "apt-get clean", 60).await?;

    log("[guard] runaway guards + security patching in place".into());
    Ok(())
}
