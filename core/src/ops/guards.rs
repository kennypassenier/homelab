//! Runaway guards (B2) + unattended security updates (A7): every managed
//! container gets hard caps on everything that grows unattended. Idempotent —
//! configs are only pushed (and services only restarted) on content change.

use crate::error::CoreError;
use crate::executor::{pct_sh, run_ok, shq, Cmd, Executor};
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
      # 8081 rather than cadvisor's own 8080: an app may already publish
      # 8080 on the container, and one uniform port everywhere
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

/// double-ingestion-logrotate (panel finding, 2026-10-01): the `rsyslog`
/// package ships its own `/etc/logrotate.d/rsyslog`, which already rotates
/// `/var/log/syslog`, `/var/log/auth.log` and `/var/log/messages` with the
/// identical `rsyslog-rotate` postrotate hook `LOGROTATE_POLICY` calls.
/// logrotate refuses two fragments that name the same path outright
/// ("duplicate log entry for ...") and exits non-zero every day it runs —
/// the second half of this finding, independent of what Alloy ingests.
///
/// Measuring this once per guards run (`test -f /etc/logrotate.d/rsyslog`)
/// and writing nothing when it answers is simpler and safer than trying to
/// subtract paths from one stanza: it leaves `/etc/logrotate.d/homelab`
/// doing exactly what it always did on a container that has no rsyslog
/// (nothing else rotates the file there), and doing nothing at all on one
/// that does, where it was never anything but a duplicate.
pub fn logrotate_policy(rsyslog_logrotate_present: bool) -> Option<&'static str> {
    if rsyslog_logrotate_present {
        None
    } else {
        Some(LOGROTATE_POLICY)
    }
}

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
    // double-ingestion-logrotate: when the rsyslog package is present it
    // already shipped /etc/logrotate.d/rsyslog, rotating the same paths
    // this policy would — two fragments naming one path is a logrotate
    // error, not a harmless overlap.
    let rsyslog_logrotate_present = pct_sh(exec, vmid, "test -f /etc/logrotate.d/rsyslog", 10)
        .await
        .map(|o| o.success())
        .unwrap_or(false);

    // sqlite3, because a service's own health checks (J1) run at this level
    // and several of them ask the application's database what it holds. The
    // alternative is a check that depends on a tool somebody installed by
    // hand once, which is the shape of a check that works until it does not.
    ensure_package(exec, vmid, "sqlite3", "sqlite3").await?;

    // latch (dashboard-latch): verified from its signed release when it is
    // missing, left alone when it is there.
    ensure_latch(exec, sink, vmid).await?;
    match logrotate_policy(rsyslog_logrotate_present) {
        Some(policy) => {
            push_content(exec, vmid, "/etc/logrotate.d/homelab", policy, "644").await?;
        }
        None => {
            // A container that had this rule from before rsyslog arrived
            // (or before this guard existed) must not keep rotating a path
            // rsyslog's own fragment now also claims.
            pct_sh(exec, vmid, "rm -f /etc/logrotate.d/homelab", 10).await?;
            log(
                "[guard] /etc/logrotate.d/rsyslog already rotates syslog/auth.log/messages — \
                 the homelab rule is not written (double-ingestion-logrotate)"
                    .into(),
            );
        }
    }

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

/// dashboard-latch: where latch comes from and where it goes.
pub const LATCH_REPO: &str = "kennypassenier/latch-rs";
pub const LATCH_ASSET: &str = "latch-x86_64-unknown-linux-gnu";
pub const LATCH_BIN: &str = "/usr/local/bin/latch";
/// Beside the real path, so a copy that cannot run never replaces one that can.
const LATCH_NEW: &str = "/usr/local/bin/latch.homelab-new";
/// The presence probe's answer for "not installed"; any other failure is the
/// probe itself failing (container down), which is not a reason to download.
const LATCH_MISSING: i32 = 3;

/// dashboard-latch (Kenny, 2026-09-29, form "Latch": "latch in ct120, latch
/// moet als default geinstalleerd worden in de golden images"): latch on
/// every managed container, and so in the golden templates, which run these
/// guards in their "bake guards" step. The dashboard on CT 120 needs it to
/// deploy the stacks whose secrets live in latch; everywhere else it is the
/// tool at hand when a secret has to be read on the container itself.
///
/// Idempotent and cheap when present: one `test -x`, no network. When it is
/// missing the host fetches the newest SIGNED release of latch-rs (the
/// orchestrator's native releases follow `latest` the same way; a pin would
/// be one more hand-kept version, and the signature is what proves the
/// author), checks the minisign signature over `SHA256SUMS` (fix-29, the
/// same check as every native release), checks the download against that
/// list, pushes it beside the real path, checks the container's glibc can
/// run it (latch is a glibc build, F304), and only then moves it into place.
/// Any failed check fails the step, like a failed apt install (fix-161).
///
/// git comes with it: latch drives the git CLI for its secrets clone.
/// Returns true when latch was installed.
pub async fn ensure_latch(
    exec: &dyn Executor,
    sink: &dyn Sink,
    vmid: u16,
) -> Result<bool, CoreError> {
    ensure_package(exec, vmid, "git", "git").await?;
    let probe = pct_sh(
        exec,
        vmid,
        &format!("test -x {LATCH_BIN} || exit {LATCH_MISSING}"),
        30,
    )
    .await?;
    if probe.success() {
        return Ok(false);
    }
    if probe.code != LATCH_MISSING {
        return Err(CoreError::Command {
            rendered: format!("test -x {LATCH_BIN}"),
            detail: format!(
                "could not tell whether latch is installed (exit {}): {}",
                probe.code,
                probe.stderr.trim()
            ),
        });
    }
    let tag = install_latch(exec, vmid).await?;
    sink.emit(PipelineEvent::Line {
        level: Level::Info,
        source: "HOST".into(),
        msg: format!(
            "[guard] latch {tag} installed at {LATCH_BIN} (signature and checksum verified)"
        ),
    });
    Ok(true)
}

async fn fetch(exec: &dyn Executor, url: &str, what: &str) -> Result<String, CoreError> {
    let out = exec
        .run(&Cmd::new(
            "curl",
            &[
                "-sSL",
                "-m",
                "60",
                "-H",
                "Accept: application/vnd.github+json",
                url,
            ],
            90,
        ))
        .await?;
    if !out.success() {
        return Err(CoreError::Other(format!(
            "could not fetch {what} of latch: {}",
            out.stderr.trim()
        )));
    }
    Ok(out.stdout)
}

async fn install_latch(exec: &dyn Executor, vmid: u16) -> Result<String, CoreError> {
    use crate::ops::native::{
        glibc_probe_script, glibc_verdict, listed_sha, newest_signed_release,
    };
    let list = fetch(
        exec,
        &format!("https://api.github.com/repos/{LATCH_REPO}/releases?per_page=10"),
        "the release list",
    )
    .await?;
    let refs = newest_signed_release(&list, LATCH_ASSET).map_err(CoreError::Other)?;
    let sig_url = refs.sig_url.clone().unwrap_or_default();
    let sig = fetch(exec, &sig_url, "the signature").await?;
    let sums = fetch(exec, &refs.sums_url, "SHA256SUMS").await?;
    crate::release_sig::verify_sums(&sums, &sig)
        .map_err(|e| CoreError::SafetyAbort(format!("latch {}: {}", refs.tag, e)))?;
    let wanted = listed_sha(&sums, LATCH_ASSET).ok_or_else(|| {
        CoreError::SafetyAbort(format!(
            "the signed SHA256SUMS of latch {} lists no '{LATCH_ASSET}' — not installing it",
            refs.tag
        ))
    })?;

    // The download lands under the root-only state dir on the host (H21),
    // one file per container so two guards never share it (T74).
    let staged = format!("/var/lib/homelab/staged/latch/latch-{vmid}");
    let script = format!(
        "mkdir -p /var/lib/homelab/staged/latch && curl -sSL -m 300 -o {f} {u} && \
         sha256sum {f} | cut -d' ' -f1",
        f = shq(&staged),
        u = shq(&refs.asset_url)
    );
    let out = exec.run(&Cmd::new("sh", &["-c", &script], 400)).await?;
    let rm_staged = Cmd::new("rm", &["-f", &staged], 30);
    let drop_staged = || exec.run(&rm_staged);
    if !out.success() {
        let _ = drop_staged().await;
        return Err(CoreError::Other(format!(
            "download of latch {} failed: {}",
            refs.tag,
            out.stderr.trim()
        )));
    }
    if !out.stdout.trim().eq_ignore_ascii_case(&wanted) {
        let _ = drop_staged().await;
        return Err(CoreError::SafetyAbort(format!(
            "CHECKSUM MISMATCH for {LATCH_ASSET} in latch {}: the signed SHA256SUMS does not \
             list this download — corrupted or tampered; nothing installed",
            refs.tag
        )));
    }
    let pushed = run_ok(
        exec,
        &Cmd::new(
            "pct",
            &[
                "push",
                &vmid.to_string(),
                &staged,
                LATCH_NEW,
                "--perms",
                "0755",
            ],
            120,
        ),
    )
    .await;
    let _ = drop_staged().await;
    pushed?;

    let probe = pct_sh(exec, vmid, &glibc_probe_script(LATCH_NEW), 60).await?;
    if let Err(why) = glibc_verdict(&probe.stdout) {
        let _ = pct_sh(exec, vmid, &format!("rm -f {LATCH_NEW}"), 30).await;
        return Err(CoreError::SafetyAbort(format!(
            "latch {}: {} :: nothing installed",
            refs.tag, why
        )));
    }
    let swap = pct_sh(
        exec,
        vmid,
        &format!("{LATCH_NEW} --version >/dev/null && mv -f {LATCH_NEW} {LATCH_BIN}"),
        60,
    )
    .await?;
    if !swap.success() {
        let _ = pct_sh(exec, vmid, &format!("rm -f {LATCH_NEW}"), 30).await;
        return Err(CoreError::Command {
            rendered: format!("mv -f {LATCH_NEW} {LATCH_BIN}"),
            detail: format!(
                "latch {} does not run on this container (exit {}): {}",
                refs.tag,
                swap.code,
                swap.stderr.trim()
            ),
        });
    }
    Ok(refs.tag)
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

/// rule-20 (disk-audit, 2026-10-01): the fleet default applied to a data
/// mount that declares no `rotate:` of its own and does not opt out
/// (`DataMount::no_default_rotate`). Kenny's numbers: 50M, keep 5, `*.log` —
/// generous enough not to lose a day's log, small enough that an app that
/// never stops writing cannot grow it past a weekend. A host.toml key
/// (`default_log_rotation`), editable from the dashboard like every other
/// fleet default.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct FleetLogRotationDefault {
    /// File name or glob under the mount, e.g. `*.log`.
    #[serde(default = "default_fleet_rotate_files")]
    pub files: String,
    #[serde(default = "default_fleet_rotate_keep")]
    pub keep: u32,
    #[serde(default = "default_fleet_rotate_max_size")]
    pub max_size: String,
}

fn default_fleet_rotate_files() -> String {
    "*.log".into()
}
fn default_fleet_rotate_keep() -> u32 {
    5
}
fn default_fleet_rotate_max_size() -> String {
    "50M".into()
}

impl Default for FleetLogRotationDefault {
    fn default() -> Self {
        Self {
            files: default_fleet_rotate_files(),
            keep: default_fleet_rotate_keep(),
            max_size: default_fleet_rotate_max_size(),
        }
    }
}

/// fix-24 / rule-20: the logrotate rule for every data mount — its own
/// `rotate:` when it declares one, else the fleet default (`fleet_default`)
/// unless the mount opts out (`no_default_rotate`) — or None when nothing
/// applies to any mount. Pure, so the exact bytes are tested.
pub fn rotation_policy(
    data_mounts: &[crate::manifest::DataMount],
    fleet_default: Option<&FleetLogRotationDefault>,
) -> Option<String> {
    let mut out = String::new();
    for dm in data_mounts {
        let owned;
        let (r, from_default) = match &dm.rotate {
            Some(r) => (r, false),
            None if dm.no_default_rotate => continue,
            None => match fleet_default {
                Some(d) => {
                    owned = crate::manifest::LogRotation {
                        files: d.files.clone(),
                        keep: d.keep,
                        reopen: None,
                        max_size: Some(d.max_size.clone()),
                    };
                    (&owned, true)
                }
                None => continue,
            },
        };
        let path = format!("{}/{}", dm.mount_point.trim_end_matches('/'), r.files);
        out.push_str(&format!(
            "# written by the homelab deploy from data_mounts[{}].rotate{} — edits here are overwritten\n",
            dm.mount_point,
            if from_default {
                " (fleet default, default_log_rotation)"
            } else {
                ""
            }
        ));
        out.push_str(&format!("{} {{\n    daily\n    rotate {}\n", path, r.keep));
        if let Some(size) = &r.max_size {
            out.push_str(&format!("    maxsize {}\n", size));
        }
        out.push_str("    missingok\n    notifempty\n    compress\n    delaycompress\n");
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
    fleet_default: Option<&FleetLogRotationDefault>,
) -> Result<bool, CoreError> {
    let path = rotation_path(stack);
    match rotation_policy(data_mounts, fleet_default) {
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

#[cfg(test)]
mod double_ingestion_logrotate_tests {
    //! double-ingestion-logrotate (panel finding, 2026-10-01): `rsyslog`
    //! ships `/etc/logrotate.d/rsyslog`, rotating `/var/log/syslog`,
    //! `/var/log/auth.log` and `/var/log/messages` — the same paths
    //! `LOGROTATE_POLICY` names. logrotate refuses two fragments naming one
    //! path ("duplicate log entry"), so this project's own rule must step
    //! aside on a container that has rsyslog's.
    use super::*;

    #[test]
    fn no_rsyslog_fragment_present_writes_the_homelab_policy() {
        assert_eq!(logrotate_policy(false), Some(LOGROTATE_POLICY));
    }

    #[test]
    fn an_rsyslog_fragment_present_means_the_homelab_policy_is_not_written() {
        assert_eq!(logrotate_policy(true), None);
    }
}
