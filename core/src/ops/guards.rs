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
///
/// `live-restore` (expert panel 2026-09-27, docker-no-live-restore): a change
/// to this file restarts docker, and without it that stopped every container
/// on the machine. The first deploy that adds it still restarts them once,
/// because the daemon being stopped is the one that did not have it.
pub const DOCKER_DAEMON_JSON: &str = r#"{
  "live-restore": true,
  "log-driver": "json-file",
  "log-opts": {
    "max-size": "10m",
    "max-file": "3",
    "tag": "{{.Name}}"
  }
}
"#;

/// cAdvisor, on every managed docker host.
///
/// H4. It was a per-stack app directory in seven of thirteen stacks, while
/// `deploy.rs` wrote a Prometheus scrape target for EVERY stack with apps —
/// so metrics and syncthing were scraped and answered nothing, permanently
/// down and permanently silent: the HostDown rule watches the node job, not
/// this one, so an empty container panel and a working one look the same.
///
/// It belongs here rather than in the golden template, even though the
/// template is where "every container gets it" naturally lives. Baking it in
/// only reaches containers cloned afterwards, and the two blind spots are
/// containers that already exist. The guards run on every managed container
/// on every deploy, which is exactly the reach this needs.
/// fix-82 (manual-images-latest-unpinned, 2026-09-27): the version and digest
/// every docker container ran that day (12 of 12, measured). `:latest` on a
/// `manual` service meant the next fresh container got whatever gcr.io called
/// latest, and nothing recorded which. The template pre-pulls this same
/// reference; moving it is an edit of this line and of the compose below.
pub const CADVISOR_IMAGE: &str =
    "gcr.io/cadvisor/cadvisor:v0.55.1@sha256:3de2bd5203120b866d74a9b283b2ffb8ec382fbf9dc321814700c6ea6f44ec57";

pub const CADVISOR_COMPOSE: &str = r#"services:
  cadvisor:
    image: gcr.io/cadvisor/cadvisor:v0.55.1@sha256:3de2bd5203120b866d74a9b283b2ffb8ec382fbf9dc321814700c6ea6f44ec57
    container_name: cadvisor
    restart: unless-stopped
    command:
      # Docker containers only. Without this cadvisor also emits a series per
      # cgroup, which in an LXC means thousands of metrics nobody reads.
      - --docker_only=true
      # Container labels become Prometheus labels; the *arr stacks carry
      # enough of them to blow up cardinality for no analytical gain.
      - --store_container_labels=false
      # Matches the 30s scrape interval: sampling faster only burns CPU.
      - --housekeeping_interval=30s
    volumes:
      - /:/rootfs:ro
      - /var/run:/var/run:ro
      - /sys:/sys:ro
      - /var/lib/docker/:/var/lib/docker:ro
    ports:
      # 8081 rather than cadvisor's own 8080: on the downloader stack that
      # port is already published by gluetun, and one uniform port everywhere
      # keeps the scrape config to a single pattern.
      - "8081:8080"
    labels:
      - com.homelab.update.policy=manual
      # fix-83: where the nightly round asks whether this pin is behind.
      # cAdvisor moved its images from gcr.io to ghcr.io/google/cadvisor after
      # v0.55; the notice says a newer release exists, not that gcr.io has it.
      - com.homelab.update.upstream=github.com/google/cadvisor
# No custom network on purpose: cadvisor has to run on EVERY docker host to
# see that host's containers, and a stack network exists only on its own
# stack. Prometheus scrapes the published port over the LAN, identically
# everywhere.
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

/// F307: matched on the CODENAME, not on the archive alias.
///
/// This used to be `Allowed-Origins` with `"${distro_id}:${distro_codename}-security"`,
/// which matches the archive field. Debian renames that field as a release
/// ages: bookworm-security reports `a=oldstable-security` once trixie ships,
/// and trixie-security reports `a=stable-security` today. The pattern then
/// matches nothing and every security package is refused with a -32768 pin,
/// while the daily run reports "No packages found that can be upgraded
/// unattended" — true, and the exact opposite of what is happening.
///
/// Measured 2026-09-09 across the whole fleet: twelve Debian 12 containers
/// each carried 19-20 unapplied security updates. The two Debian 13
/// containers carried none, which looked like the newer release being
/// immune — it is not. A dry-run there refuses `a=stable-security` in the
/// same words; they simply had no backlog yet. **The version was never the
/// variable.**
///
/// `codename=` matches `n=bookworm-security` / `n=trixie-security`, which
/// Debian does not rename. Both spellings are listed because the security
/// suite is published under the `-security` codename while point releases
/// arrive under the bare one, and a pattern that covers one and not the
/// other is how this class of silence starts.
///
/// `site=apt.grafana.com` (Kenny's triage answer 2026-09-27): Alloy, the log
/// shipper on every container, comes from Grafana's signed repository and
/// was never updated (1.19.2 installed, 1.20.0 available on CT 106). Only
/// Alloy is installed from that repository. Its release fields carry no
/// usable origin (`o=. stable`), so the pattern matches on the site.
pub const UNATTENDED_UPGRADES: &str = r#"Unattended-Upgrade::Origins-Pattern {
    "origin=Debian,codename=${distro_codename},label=Debian-Security";
    "origin=Debian,codename=${distro_codename}-security,label=Debian-Security";
    "site=apt.grafana.com,a=stable";
};
Unattended-Upgrade::Automatic-Reboot "false";
Unattended-Upgrade::Remove-Unused-Dependencies "true";
"#;

pub const PRUNE_SERVICE: &str = "[Unit]\nDescription=Prune stale Docker data (homelab runaway guard)\n\n[Service]\nType=oneshot\nExecStart=/usr/bin/docker system prune -f --filter until=168h\n";

pub const PRUNE_TIMER: &str = "[Unit]\nDescription=Weekly Docker prune (homelab runaway guard)\n\n[Timer]\nOnCalendar=weekly\nRandomizedDelaySec=1h\nPersistent=true\n\n[Install]\nWantedBy=timers.target\n";

/// A stack the host manages, as `apply_for_managed` needs it.
pub struct ManagedTarget {
    pub name: String,
    pub vmid: u16,
    /// false for a native-only stack, which runs no docker.
    pub docker: bool,
}

/// Guards applied on request (`homelab guards`, the TUI's `g`).
///
/// gap-33: that path skipped the A2 hostname guard every other mutating
/// operation uses, and always installed the docker guards, which on a native
/// container add a weekly prune timer that fails every week (see `apply`).
/// Only a stack the host manages is touched, after the hostname guard, with
/// docker guards only where it runs docker.
pub async fn apply_for_managed(
    exec: &dyn Executor,
    sink: &dyn Sink,
    safety: &crate::safety::SafetyConfig,
    managed: &[ManagedTarget],
    vmid: u16,
    cache: Option<&crate::ops::registry_cache::CacheCfg>,
) -> Result<(), CoreError> {
    let Some(target) = managed.iter().find(|t| t.vmid == vmid) else {
        return Err(CoreError::SafetyAbort(format!(
            "vmid {} is not a stack this host manages :: guards are applied to managed stacks; \
             `homelab deploy` one first, which applies them itself",
            vmid
        )));
    };
    crate::ops::guard_target(exec, safety, vmid, &format!("{}-app-{}", vmid, target.name)).await?;
    apply(exec, sink, vmid, target.docker, cache).await
}

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
    cache: Option<&crate::ops::registry_cache::CacheCfg>,
) -> Result<(), CoreError> {
    let log = |msg: String| {
        sink.emit(PipelineEvent::Line {
            level: Level::Info,
            source: "HOST".into(),
            msg,
        })
    };

    // 1. Docker container logs — must land before app containers (re)start.
    // D60: the cache speaks plain HTTP on the LAN, so the daemon has to be
    // told those addresses are expected. Without it every cached pull fails
    // with "server gave HTTP response to HTTPS client" — which reads like the
    // cache is broken rather than like a setting is missing.
    let daemon_json = match cache {
        None => DOCKER_DAEMON_JSON.to_string(),
        Some(c) => {
            let hosts: Vec<String> = c
                .upstreams
                .iter()
                .map(|u| format!("\"{}:{}\"", c.host, u.port))
                .collect();
            DOCKER_DAEMON_JSON
                .trim_end()
                .trim_end_matches('}')
                .trim_end()
                .to_string()
                + &format!(",\n  \"insecure-registries\": [{}]\n}}\n", hosts.join(", "))
        }
    };
    if docker && push_content(exec, vmid, "/etc/docker/daemon.json", &daemon_json, "644").await? {
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
    ensure_package(exec, vmid, "logrotate", "logrotate").await?;

    // sqlite3, because a service's own health checks (J1) run at this level
    // and several of them ask the application's database what it holds. The
    // alternative is a check that depends on a tool somebody installed by
    // hand once, which is the shape of a check that works until it does not.
    ensure_package(exec, vmid, "sqlite3", "sqlite3").await?;
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

    // 4b. cAdvisor on every docker host (H4). Same reasoning as the log caps:
    // something every container needs, that nothing per-stack should have to
    // remember to declare.
    if docker {
        push_content(
            exec,
            vmid,
            "/opt/cadvisor/docker-compose.yml",
            CADVISOR_COMPOSE,
            "644",
        )
        .await?;
        // Unconditionally, NOT only when the file changed.
        //
        // It used to read `if push_content(...).await?`, and that returns true
        // only when the content DIFFERS. So the first run wrote the file and
        // started cadvisor, and every run after found the file identical and
        // skipped the start. A cadvisor that never came up, or came up once
        // and later stopped, could not be repaired by any number of deploys:
        // the guard was permanently satisfied because the FILE was in place,
        // while its purpose is that the SERVICE runs.
        //
        // Measured 2026-09-01 across the fleet — the file on 10 of 10
        // containers, cadvisor running on 1. Starting it by hand on one of the
        // nine worked first time and took a second (F164).
        //
        // `docker compose up -d` is idempotent, so running it every time costs
        // a no-op and buys self-repair.
        pct_sh(exec, vmid, "cd /opt/cadvisor && docker compose up -d", 300).await?;
        log("[guard] cadvisor up — this host reports its containers".into());
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
    ensure_package(exec, vmid, "unattended-upgrade", "unattended-upgrades").await?;
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

/// fix-161: install `pkg` unless `tool` is already on the PATH. The package
/// lists are refreshed first (a container cloned from an old template carries
/// lists whose files are gone from the mirror), and a failed install fails the
/// step: on CT 118 the ignored exit code let a deploy report "security
/// patching in place" with neither unattended-upgrades nor sqlite3 installed.
async fn ensure_package(
    exec: &dyn Executor,
    vmid: u16,
    tool: &str,
    pkg: &str,
) -> Result<(), CoreError> {
    let out = pct_sh(
        exec,
        vmid,
        &format!(
            "command -v {tool} >/dev/null || (export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq {pkg})"
        ),
        300,
    )
    .await?;
    if out.success() {
        return Ok(());
    }
    Err(CoreError::Command {
        rendered: format!("apt-get install {pkg}"),
        detail: format!(
            "{pkg} could not be installed (exit {}): {}",
            out.code,
            out.stderr.trim()
        ),
    })
}

/// fix-24: where the per-stack rotation rule lives inside the container.
pub fn rotation_path(stack: &str) -> String {
    format!("/etc/logrotate.d/homelab-{}", stack)
}

/// fix-24: the logrotate rule for every data mount that declares `rotate:`,
/// or None when the stack declares none. Pure, so the exact bytes are tested.
pub fn rotation_policy(data_mounts: &[crate::manifest::DataMount]) -> Option<String> {
    let mut out = String::new();
    for dm in data_mounts {
        let Some(r) = &dm.rotate else { continue };
        let path = format!("{}/{}", dm.mount_point.trim_end_matches('/'), r.files);
        out.push_str(&format!(
            "# written by the homelab deploy from data_mounts[{}].rotate — edits here are overwritten\n",
            dm.mount_point
        ));
        out.push_str(&format!(
            "{} {{\n    daily\n    rotate {}\n    missingok\n    notifempty\n    compress\n    delaycompress\n",
            path, r.keep
        ));
        match &r.reopen {
            Some(o) => out.push_str(&format!(
                "    sharedscripts\n    postrotate\n        docker kill --signal={} {} >/dev/null 2>&1 || true\n    endscript\n",
                o.signal, o.container
            )),
            None => out.push_str("    copytruncate\n"),
        }
        out.push_str("}\n");
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// fix-24: install (or remove) the stack's rotation rule. Returns true when
/// the container changed.
pub async fn apply_rotation(
    exec: &dyn Executor,
    vmid: u16,
    stack: &str,
    data_mounts: &[crate::manifest::DataMount],
) -> Result<bool, CoreError> {
    let path = rotation_path(stack);
    match rotation_policy(data_mounts) {
        Some(policy) => push_content(exec, vmid, &path, &policy, "644").await,
        None => {
            // A rule whose declaration was removed must go with it, or the
            // stack file stops being the whole truth about this container.
            let out = pct_sh(
                exec,
                vmid,
                &format!("test -e {p} && rm -f {p} && echo removed || true", p = path),
                30,
            )
            .await?;
            Ok(out.stdout.contains("removed"))
        }
    }
}
