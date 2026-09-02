//! Back up a device this suite may not touch, by asking it for its own
//! configuration and putting the answer in restic.
//!
//! Kenny's route A (form J1 in the OPNsense session, 2026-09-02), after the
//! obvious road turned out to be closed: OPNsense's `os-gdrive-backup`
//! plugin cannot write to a consumer Google Drive any more — since
//! 2025-04-15 a new service account cannot own Drive items there, and the
//! plugin's own maintainer says it "may be useless for new installs". Four
//! attempts that afternoon all died on the first real Drive request.
//!
//! So the router keeps backing itself up the way it always could — by
//! answering an HTTP request — and this suite does the storing, using the
//! same restic repository layout, password and retention as every stack.
//!
//! **VM 100 stays untouched.** This is one GET against an API. Nothing here
//! runs on the device, changes it, or restarts it; the no-touch list is not
//! bent for it and does not need to be.

use crate::error::CoreError;
use crate::executor::{run_ok, Cmd};
use crate::ops::backup::{BackupCfg, RESTIC_CACHE_DIR};
use crate::ops::OpCtx;
use crate::runner::{OperationReport, Runner, StepOutcome};
use crate::sink::Level;

macro_rules! step {
    ($runner:expr, $name:expr, $body:expr) => {
        match $runner.step($name, || async { $body }).await {
            Ok(o) => o,
            Err(e) => return $runner.finish_err($name, &e),
        }
    };
}

/// One device that can hand over its own configuration.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct DeviceBackup {
    /// Names the restic repository: `<base>/<name>-config`, the same shape
    /// every stack uses, so retention and the existing "last backup was N
    /// hours ago" check treat it like any other.
    pub name: String,
    /// Full URL of the endpoint that returns the configuration.
    pub url: String,
    /// Host-local, root-only, and in **curl's own config format** rather
    /// than a bare `key:secret`:
    ///
    /// ```text
    /// user = "<key>:<secret>"
    /// ```
    ///
    /// Deliberate: `-u "$(cat file)"` would put the credential in curl's
    /// argv, and `/proc/<pid>/cmdline` is world-readable while
    /// `/proc/<pid>/environ` is not. `-K` keeps it off the command line
    /// entirely, with no temporary file to clean up (rule 10: secrets never
    /// in argv).
    pub cred_file: String,
    /// What the downloaded file is called inside the snapshot.
    pub filename: String,
    /// An SPKI pin — `--pinnedpubkey sha256//<base64>` — which is what
    /// actually verifies this connection.
    ///
    /// Preferred over a CA bundle here for a measured reason: OPNsense's web
    /// certificate is self-signed with `CN=OPNsense.internal` and a SAN that
    /// lists only that name, no IP. `--cacert` against `https://10.10.10.1`
    /// therefore fails on a name mismatch and needs a `--resolve` alias to
    /// work at all; a public-key pin does not care what the host is called.
    /// Measured 2026-09-02: the right pin returns 200, a wrong one returns
    /// curl exit 90 and no connection at all.
    ///
    /// **A pin outlives nothing.** That certificate expires 2027-04-12, and
    /// renewing it changes this value. The failure then is `curl (90)`,
    /// which is a different and much clearer error than a 403 or a timeout.
    pub pin: Option<String>,
    /// A CA bundle, for a device whose certificate can be verified by name.
    /// Needs the URL to use a hostname the certificate actually carries.
    pub ca_file: Option<String>,
}

/// A plausible answer that is not a configuration is the failure this guards
/// against: an OPNsense login page is a 200 with HTML in it, and restic
/// would store it without complaint.
const MIN_PLAUSIBLE_BYTES: usize = 4096;

pub async fn backup_device(
    ctx: &OpCtx<'_>,
    dev: &DeviceBackup,
    cfg: &BackupCfg,
) -> OperationReport {
    let op = format!("device-backup-{}", dev.name);
    let mut runner = Runner::new(&op, ctx.sink, ctx.journal);
    let exec = ctx.exec;

    runner.log(
        Level::Info,
        format!("[device] asking {} for its configuration", dev.name),
    );

    let verify = match (&dev.pin, &dev.ca_file) {
        // `-k` beside a pin is not a contradiction: it turns off the NAME
        // check, which cannot succeed against an IP, while the pin does the
        // verifying. curl enforces --pinnedpubkey regardless of -k.
        (Some(pin), _) => format!("-k --pinnedpubkey {}", crate::ops::util::shq(pin)),
        (None, Some(ca)) => format!("--cacert {}", crate::ops::util::shq(ca)),
        (None, None) => {
            runner.log(
                Level::Warn,
                format!(
                    "[device] {}: the certificate is NOT verified — a credential \
                     that can read the whole configuration crosses the wire on \
                     trust alone. Set ca_file to end that.",
                    dev.name
                ),
            );
            "-k".to_string()
        }
    };

    // The repository has to exist before anything is piped into it.
    //
    // Missing until 2026-09-03, and it stayed missing because this whole path
    // had never once run: the first real execution died with "repository does
    // not exist" AFTER curl had already read 16 KB off the router (F259).
    // Every other backup in this project inits first; this one was written
    // without that step and nothing noticed, because nothing ran it.
    step!(runner, "init repo", {
        let script = format!(
            "env RESTIC_REPOSITORY={base}/{name}-config \
             RESTIC_PASSWORD_FILE={pw} RESTIC_CACHE_DIR={cache} \
             restic cat config >/dev/null 2>&1 \
             || env RESTIC_REPOSITORY={base}/{name}-config \
                RESTIC_PASSWORD_FILE={pw} RESTIC_CACHE_DIR={cache} restic init",
            base = cfg.restic_base,
            name = dev.name,
            pw = cfg.password_file,
            cache = RESTIC_CACHE_DIR,
        );
        // Idempotent: `restic cat config` succeeds on a repository that is
        // already there, so init only runs the first time.
        run_ok(exec, &Cmd::new("sh", &["-c", &script], 300)).await?;
        Ok(StepOutcome::Changed)
    });

    step!(runner, "fetch and store", {
        // pipefail is load-bearing, the same lesson as the native backup: a
        // dead curl otherwise yields a "successful" empty snapshot, and a
        // backup that lies is worse than no backup. `--fail-with-body` so a
        // 403 is an error rather than a stored error page.
        let script = format!(
            "set -o pipefail; \
             curl -sS {} --fail-with-body -K {} {} \
             | env RESTIC_REPOSITORY={}/{}-config RESTIC_PASSWORD_FILE={} \
               RESTIC_CACHE_DIR={} \
               restic backup --stdin --stdin-filename {}",
            verify,
            crate::ops::util::shq(&dev.cred_file),
            crate::ops::util::shq(&dev.url),
            cfg.restic_base,
            dev.name,
            cfg.password_file,
            RESTIC_CACHE_DIR,
            crate::ops::util::shq(&dev.filename),
        );
        run_ok(
            exec,
            &Cmd::new("sh", &["-c", &script], cfg.snapshot_timeout_s),
        )
        .await?;
        Ok(StepOutcome::Changed)
    });

    // Asking the repository rather than trusting the exit code: restic is
    // perfectly willing to store four bytes of error page.
    step!(runner, "verify the snapshot has substance", {
        let out = exec
            .run(&Cmd::new(
                "sh",
                &[
                    "-c",
                    &format!(
                        "env RESTIC_REPOSITORY={}/{}-config \
                         RESTIC_PASSWORD_FILE={} RESTIC_CACHE_DIR={} \
                         restic stats latest --mode raw-data --json",
                        cfg.restic_base, dev.name, cfg.password_file, RESTIC_CACHE_DIR
                    ),
                ],
                300,
            ))
            .await?;
        let size = out
            .stdout
            .split("\"total_size\":")
            .nth(1)
            .and_then(|s| {
                s.split(|c: char| !c.is_ascii_digit())
                    .find(|x| !x.is_empty())
            })
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);
        if size < MIN_PLAUSIBLE_BYTES {
            return Err(CoreError::Other(format!(
                "{} stored only {} bytes — a login page and an error page are \
                 both a plausible 200. Check the credential's privileges before \
                 trusting this repository :: remedy: run the request by hand \
                 and look at what comes back",
                dev.name, size
            )));
        }
        Ok(StepOutcome::Unchanged)
    });

    runner.finish_ok()
}
