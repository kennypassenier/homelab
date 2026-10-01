//! F3 notification building + repeat-damping (hardening H13). The payload
//! is built here so its shape is golden-tested — the HA automation parses
//! these fields, and a silent rename would break the events log.

use std::collections::HashMap;

/// One event, exactly as POSTed to the webhook — and the ONLY place a
/// payload is built.
///
/// F86: every caller builds its payload through here, never by hand, so a
/// `source`/`label` filter never silently drops one of them. Story:
/// `docs/deployment/REGISTER.md`.
pub fn op_payload(op: &str, label: &str, ok: bool, error: Option<&str>, version: &str) -> String {
    op_payload_from("homelab-host", op, label, ok, error, version)
}

/// milestone act (homelab-admin, 2026-09-28): the same payload from another
/// sender. The dashboard's pushes (feat-ops-8) say `homelab-admin`, so a
/// consumer can tell them from the host's own and nothing else changes.
pub fn op_payload_from(
    source: &str,
    op: &str,
    label: &str,
    ok: bool,
    error: Option<&str>,
    version: &str,
) -> String {
    let error = error.map(notify_error_text);
    serde_json::json!({
        "source": source,
        "op": op,
        "label": label,
        "ok": ok,
        "error": error,
        "version": version,
    })
    .to_string()
}

/// fix-57: the most error text one notification carries.
pub const NOTIFY_ERROR_MAX: usize = 1024;

/// fix-57: what a notification says about a failure. The reason can be
/// kilobytes of app output, so it is masked here whatever the caller did
/// and cut to [`NOTIFY_ERROR_MAX`] bytes with a pointer to where the whole
/// text is. Story: `docs/deployment/REGISTER.md`.
pub fn notify_error_text(error: &str) -> String {
    let masked = error
        .lines()
        .map(crate::executor::mask_secrets)
        .collect::<Vec<_>>()
        .join("\n");
    if masked.len() <= NOTIFY_ERROR_MAX {
        return masked;
    }
    let mut cut = NOTIFY_ERROR_MAX;
    while !masked.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}… ({} bytes in all; the full text is in the incident bundle)",
        &masked[..cut],
        masked.len()
    )
}

/// Failure-repeat damping: an identical failing event inside the window is
/// suppressed (a stack that fails every night must not page every night);
/// successes are never suppressed, and a CHANGED error text passes through
/// (it is new information).
pub struct NotifyDamper {
    window_s: u64,
    last_sent: HashMap<String, u64>,
}

impl NotifyDamper {
    pub fn new(window_s: u64) -> Self {
        Self {
            window_s,
            last_sent: HashMap::new(),
        }
    }

    /// Decide-and-record. `now` injected — core reads no clocks.
    pub fn should_send(&mut self, op: &str, ok: bool, error: Option<&str>, now: u64) -> bool {
        if ok {
            return true;
        }
        let key = format!("{}|{}", op, error.unwrap_or(""));
        match self.last_sent.get(&key) {
            Some(&t) if now.saturating_sub(t) < self.window_s => false,
            _ => {
                self.last_sent.insert(key, now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_payload_shape() {
        // The HA automation reads exactly these fields — frozen here.
        assert_eq!(
            op_payload("deploy-synctest", "deploy", true, None, "3.35.0"),
            r#"{"error":null,"label":"deploy","ok":true,"op":"deploy-synctest","source":"homelab-host","version":"3.35.0"}"#
        );
        assert_eq!(
            op_payload(
                "backup-media",
                "scheduled-backup",
                false,
                Some("rclone: timeout"),
                "3.35.0"
            ),
            r#"{"error":"rclone: timeout","label":"scheduled-backup","ok":false,"op":"backup-media","source":"homelab-host","version":"3.35.0"}"#
        );
    }

    #[test]
    fn act_the_dashboard_payload_differs_only_in_source() {
        let host = op_payload("deploy-media", "deploy", false, Some("x"), "3.62.2");
        let admin = op_payload_from(
            "homelab-admin",
            "deploy-media",
            "deploy",
            false,
            Some("x"),
            "3.62.2",
        );
        assert_eq!(
            admin,
            host.replace("\"homelab-host\"", "\"homelab-admin\""),
            "one shape for every sender"
        );
    }

    #[test]
    fn damper_suppresses_repeats_within_window_only() {
        let mut d = NotifyDamper::new(20 * 3600);
        let t0 = 1_800_000_000;
        assert!(d.should_send("backup-media", false, Some("timeout"), t0));
        // Same failure an hour later: suppressed.
        assert!(!d.should_send("backup-media", false, Some("timeout"), t0 + 3600));
        // Different error text: new information, passes.
        assert!(d.should_send("backup-media", false, Some("repo locked"), t0 + 3600));
        // After the window: resends (still broken → remind once a day).
        assert!(d.should_send("backup-media", false, Some("timeout"), t0 + 21 * 3600));
        // Successes are never suppressed.
        assert!(d.should_send("backup-media", true, None, t0));
        assert!(d.should_send("backup-media", true, None, t0 + 1));
    }

    /// F86: the nightly fleet check is the most important report the system
    /// sends, and it was the only one that did not go through this function
    /// — no `source`, no `label`. A consumer filtering on `source` would
    /// have dropped exactly that one. Frozen here so a third shape cannot
    /// quietly appear again.
    #[test]
    fn the_fleet_check_uses_the_same_shape_as_every_other_event() {
        let p = op_payload(
            "fleet-check",
            "nightly",
            false,
            Some("4 finding(s)"),
            "3.35.0",
        );
        let v: serde_json::Value = serde_json::from_str(&p).unwrap();
        for field in ["source", "op", "label", "ok", "error", "version"] {
            assert!(
                v.get(field).is_some(),
                "fleet-check payload lost '{}'",
                field
            );
        }
        assert_eq!(v["source"], "homelab-host");
        assert_eq!(v["label"], "nightly");
    }

    /// fix-126: `--cacert` is added only for an `https://` url with a path
    /// given, never for `http://` (plain routes stay untouched during the
    /// stepwise migration) and never silently dropped when one is given.
    #[test]
    fn curl_args_pinned_adds_cacert_only_for_https_with_a_path() {
        let plain = curl_args_pinned("{}", "http://kyu.lan/x", None, Some("/tls/hub.pem"));
        assert!(
            !plain.contains(&"--cacert".to_string()),
            "http:// must not carry --cacert even if a path is given: {:?}",
            plain
        );
        let https_unpinned = curl_args_pinned("{}", "https://kyu.lan/x", None, None);
        assert!(
            !https_unpinned.contains(&"--cacert".to_string()),
            "{:?}",
            https_unpinned
        );
        let https_pinned = curl_args_pinned("{}", "https://kyu.lan/x", None, Some("/tls/hub.pem"));
        let i = https_pinned
            .iter()
            .position(|a| a == "--cacert")
            .expect("--cacert missing");
        assert_eq!(https_pinned[i + 1], "/tls/hub.pem");
    }

    /// fix-126: `cert_fingerprint` is the SHA-256 of the PEM's DECODED body
    /// (the DER bytes), not of the PEM text itself — frozen with a body
    /// that is not a real certificate (the function never parses X.509, it
    /// only base64-decodes and hashes) so the test needs no real key pair.
    #[test]
    fn cert_fingerprint_hashes_the_decoded_der_not_the_pem_text() {
        use sha2::{Digest, Sha256};
        // base64 of "hello"
        let pem = "-----BEGIN CERTIFICATE-----\naGVsbG8=\n-----END CERTIFICATE-----\n";
        let got = cert_fingerprint(pem).unwrap();
        let want: String = Sha256::digest(b"hello")
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect();
        assert_eq!(got, want);
    }

    /// fix-126: a PEM whose body is not valid base64 is refused with a
    /// readable reason, not a panic — this runs on whatever file
    /// `notify_tls_cert` happens to name, which a typo can point anywhere.
    #[test]
    fn cert_fingerprint_refuses_invalid_base64() {
        assert!(
            cert_fingerprint(
                "-----BEGIN CERTIFICATE-----\nnot-base64!!!\n-----END CERTIFICATE-----\n"
            )
            .is_err()
        );
    }
}

/// G16 · did the POST actually arrive?
///
/// It used to be `let _ = exec.run(...)`: a fire-and-forget curl with the
/// body thrown away. Every notification this project sends — including
/// "rollback also failed, this needs hands now" — travelled a path that could
/// be broken without anybody being able to tell. The counter-argument in
/// TARGET_LAYOUT names the window that makes it concrete: the orchestrator
/// restarting kyu, which is the container the notification travels through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    /// The far end answered 2xx.
    Delivered,
    /// It did not, and this is what curl said about it.
    Failed(String),
}

/// Read curl's verdict off its exit status and the code it printed.
///
/// Deliberately strict: only 2xx counts. A 401 from a rotated bearer token
/// and a 404 from a renamed topic are both "the message did not arrive", and
/// treating them as success is how a notification path rots unnoticed — kyu
/// answers 200 on a topic that exists and 404 on one that does not, so this
/// is the difference between routing and dropping.
pub fn verdict(ran: bool, http_code: &str) -> Delivery {
    if !ran {
        return Delivery::Failed("curl did not run or timed out".into());
    }
    match http_code.trim().parse::<u16>() {
        Ok(c) if (200..300).contains(&c) => Delivery::Delivered,
        Ok(0) => Delivery::Failed("no answer (connection refused, DNS, or timeout)".into()),
        Ok(c) => Delivery::Failed(format!("HTTP {}", c)),
        Err(_) => Delivery::Failed(format!("unreadable status {:?}", http_code.trim())),
    }
}

/// Where a notification goes, in order, and why there is a second entry.
///
/// The primary is kyu (Y2): a durable hub, so an HA outage no longer loses
/// the message. The fallback is Home Assistant directly, for the one case Y2
/// carved out — kyu itself being down, which is exactly what happens while
/// the orchestrator is updating it.
///
/// This replaces the static "operations targeting the messaging stack use the
/// direct path" rule with something strictly stronger: a list to keep up to
/// date cannot know that kyu is down for a reason nobody wrote down, and
/// trying the second path when the first one fails covers that case and every
/// other one. Kenny's standing rule about hand-maintained lists, applied to a
/// list that had not been written yet.
pub fn route<'a>(primary: Option<&'a str>, fallback: Option<&'a str>) -> Vec<&'a str> {
    let mut v = Vec::new();
    if let Some(p) = primary {
        v.push(p);
    }
    if let Some(f) = fallback
        && Some(f) != primary
    {
        v.push(f);
    }
    v
}

/// fix-123: a route as a log line may show it: scheme, host and port,
/// never the path, query or credentials (fix-25 treats a webhook id as a
/// secret). Story: `docs/deployment/REGISTER.md`.
pub fn route_for_log(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return "<not a URL>".into();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or("");
    let withheld = rest.len() > authority.len();
    format!(
        "{}://{}{}",
        scheme,
        host,
        if withheld { "/<path withheld>" } else { "" }
    )
}

/// Where route `route`'s bearer header is written for curl to read (fix-35).
pub fn header_file_path(state_dir: &str, route: usize) -> String {
    format!("{}/secrets/notify-route-{}.header", state_dir, route)
}

/// The header file's content: one header line, read by `curl -H @file`.
pub fn header_file_content(token: &str) -> String {
    format!("authorization: Bearer {}\n", token)
}

/// The curl arguments for one notification POST.
///
/// fix-35: the bearer token goes through a 0600 header file that curl reads
/// with `-H @<path>`; argv only ever names the path, never the token.
/// Story: `docs/deployment/REGISTER.md`.
pub fn curl_args(payload: &str, url: &str, header_file: Option<&str>) -> Vec<String> {
    curl_args_pinned(payload, url, header_file, None)
}

/// [`curl_args`], plus `--cacert cacert_path` when one is given.
///
/// fix-126 (TLS to the message hub, owner decision 2026-10-01): a route
/// pinned to a LAN self-signed certificate (no public CA, no Cloudflare)
/// trusts exactly that certificate and nothing curl's system CA bundle
/// would otherwise accept — the same "trust only what was pinned" shape as
/// the client's own TLS pin to the host (`client/src/tls.rs`), done with
/// curl's own mechanism rather than a second TLS stack. `cacert_path` is
/// refused as a route at startup (`startup_problems`) unless its on-disk
/// fingerprint matches the configured one, so this function itself does not
/// re-check it — by the time a POST is sent, the file is already the
/// pinned certificate.
pub fn curl_args_pinned(
    payload: &str,
    url: &str,
    header_file: Option<&str>,
    cacert_path: Option<&str>,
) -> Vec<String> {
    let mut args: Vec<String> = [
        "-m",
        "5",
        "-s",
        "-o",
        "/dev/null",
        // The status is the whole point: -o /dev/null throws the body away,
        // and without this the exit code alone cannot tell a 200 from a 404
        // on a topic that no longer exists.
        "-w",
        "%{http_code}",
        "-X",
        "POST",
        "-H",
        "Content-Type: application/json",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(path) = header_file {
        args.push("-H".into());
        args.push(format!("@{}", path));
    }
    if url.starts_with("https://")
        && let Some(cacert) = cacert_path
    {
        // --cacert alone, not --cacert plus the system bundle: the
        // whole point is trusting only the pinned certificate, not
        // widening what is trusted.
        args.push("--cacert".into());
        args.push(cacert.into());
    }
    args.push("-d".into());
    args.push(payload.into());
    args.push(url.into());
    args
}

/// fix-126: the SHA-256 of a PEM certificate file's DER bytes, lowercase
/// hex — what a configured `notify_tls_fingerprint` is checked against
/// before the host ever trusts the file at `notify_tls_cert`. Reusing the
/// same shape as `homelab_host::tls::fingerprint_of`, but over bytes
/// already read rather than a path, so core stays free of file I/O.
pub fn cert_fingerprint(pem: &str) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let body: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<String>();
    use base64::Engine as _;
    let der = base64::engine::general_purpose::STANDARD
        .decode(body.trim())
        .map_err(|e| format!("invalid certificate PEM: {e}"))?;
    Ok(Sha256::digest(&der)
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect())
}

// ── Decision notify-routing (2026-09-30) ───────────────────────────────
//
// Kenny, form "Meldingen": everything lands in the dashboard's notification
// centre with its history; only the urgent reaches the phone, desktop and
// lights at once, and homelab decides that at the source. Every notice says
// what is wrong, since when, the consequence and what to do, with a link to
// the dashboard page that acts on it.

/// app-knowledge (2026-09-30): no public address is known in code. The
/// dashboard's comes from `dashboard_url` in host.toml and
/// `HOMELAB_ADMIN_PUBLIC_URL` in its unit; unset, a push carries no link.
pub const DEFAULT_DASHBOARD_URL: &str = "";

/// The dashboard pages a notice links to (admin/web/js/router.js).
///
/// nav-decisions (chassis-rs 3.1.0, 2026-10-01): every page lives at the
/// root now (the web app mounts at `/`; `/app/…` from before is a 308
/// chassis answers, never a path this crate should mint).
pub mod page {
    pub const NOTIFICATIONS: &str = "/notifications";
    pub const HOST: &str = "/host";
    pub const CHECKS: &str = "/health?block=checks";
    pub const TODAY: &str = "/health?block=today";
    pub const JOBS: &str = "/jobs";

    /// One stack's page.
    pub fn stack(name: &str) -> String {
        format!("/stacks/{}", name)
    }
}

/// A page's absolute address, for a push's `click_url` (newsflash opens
/// only an absolute http(s) URL).
pub fn click_url(base: &str, page: &str) -> String {
    if base.trim().is_empty() {
        return String::new();
    }
    format!("{}{}", base.trim_end_matches('/'), page)
}

/// What the routing decision looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event<'a> {
    /// A host operation ended. `label` is its kind as the host runs it
    /// ("deploy", "scheduled-backup", "self-update", …).
    Op {
        label: &'a str,
        ok: bool,
        /// It stood aside on purpose (nothing ran, nothing broke).
        deferred: bool,
    },
    /// The nightly fleet check, with how many findings are broken.
    FleetCheck { broken: usize },
    /// A Prometheus alert through Alertmanager (firing or resolved: a
    /// resolved alert goes where its firing one went).
    Alert { alertname: &'a str },
    /// A stack the host parked (disabled) after a failed nightly run.
    Parked,
    /// The host came back; `interrupted` when operations did not finish.
    Boot { interrupted: bool },
}

/// The decision and its reason in words, for the notice's push column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Urgency {
    pub urgent: bool,
    pub why: &'static str,
}

/// A service that has not answered for more than 5 minutes.
pub const SERVICE_DOWN_ALERTS: &[&str] = &["HostDown", "TargetDown"];

/// A disk almost full, or a drive failing.
pub const DISK_ALERTS: &[&str] = &[
    "FilesystemAlmostFull",
    "PveStorageAlmostFull",
    "HypervisorRootFillingUp",
    "DiskPendingSectors",
    "DiskSmartFailed",
    "ZpoolNotOnline",
    "DriveMissing",
];

/// Something stopped working, or the notification path itself broke
/// (push-edge, Kenny, 2026-09-30: "Ook meteen"): a crashed systemd unit,
/// Alertmanager unable to deliver, almanac unable to read its journal.
pub const PIPELINE_ALERTS: &[&str] = &[
    "AlertDeliveryFailing",
    "AlmanacJournalUnreadable",
    "SystemdUnitFailed",
];

/// Operations whose failure is a failed backup (every copy of data).
pub const BACKUP_OPS: &[&str] = &[
    "backup",
    "scheduled-backup",
    "backup-native",
    "scheduled-backup-native",
    "host-meta-backup",
    "device-backup",
    "second-copy",
    "restic-check",
    "zfs-replicate",
];

/// Operations whose failure is a failed update (the host's own included).
pub const UPDATE_OPS: &[&str] = &[
    "update",
    "scheduled-update",
    "update-native",
    "scheduled-update-native",
    "release-update-native",
    "scheduled-release-update",
    "self-update",
    "patch",
    "rollback-native",
];

/// Operations whose failure is a failed deploy.
pub const DEPLOY_OPS: &[&str] = &["deploy", "install-native"];

/// The one decision: does this reach the phone now, or only the centre?
///
/// Urgent (Kenny, 2026-09-30): a service not answering for more than 5
/// minutes, a failed backup, a disk almost full or a failing drive, a
/// failed update or deploy, and a nightly check that found something broken.
/// push-edge (Kenny, 2026-09-30) added a crashed unit, a broken delivery
/// path, a parked stack and a restart that interrupted work.
/// Everything else (successes, standing aside, drift, warnings not in the
/// lists above) waits in the centre and in the 09:00 digest.
pub fn urgency(e: &Event) -> Urgency {
    let yes = |why| Urgency { urgent: true, why };
    let no = |why| Urgency { urgent: false, why };
    match *e {
        Event::Op { ok: true, .. } => no("not urgent: it succeeded"),
        Event::Op { deferred: true, .. } => no("not urgent: it stood aside, nothing broke"),
        Event::Op { label, .. } => match op_kind(label) {
            OpKind::Backup => yes("urgent: a backup failed"),
            OpKind::Update => yes("urgent: an update failed"),
            OpKind::Deploy => yes("urgent: a deploy failed"),
            OpKind::Other => no("not urgent: not a backup, update or deploy"),
        },
        Event::FleetCheck { broken } if broken > 0 => {
            yes("urgent: the nightly check found something broken")
        }
        Event::FleetCheck { .. } => no("not urgent: drift only, nothing broken"),
        Event::Alert { alertname } if SERVICE_DOWN_ALERTS.contains(&alertname) => {
            yes("urgent: a service does not answer")
        }
        Event::Alert { alertname } if DISK_ALERTS.contains(&alertname) => {
            yes("urgent: a disk is almost full or failing")
        }
        Event::Alert { alertname } if PIPELINE_ALERTS.contains(&alertname) => {
            yes("urgent: something stopped, or notifications may not arrive")
        }
        Event::Alert { .. } => no("not urgent: not in the urgent alert list"),
        Event::Parked => yes("urgent: a stack was parked and lost its nightly updates"),
        Event::Boot { interrupted: true } => {
            yes("urgent: the host restarted with work interrupted")
        }
        Event::Boot { interrupted: false } => {
            no("not urgent: the host is back, nothing interrupted")
        }
    }
}

/// Owner decision 2026-09-30 (item 3): the notifications table's push
/// column, short — "not sent: not urgent: it succeeded" read every
/// [`urgency`] reason in full and was far too long for a column. This
/// collapses the same judgement to a handful of fixed words; the long
/// reason stays available for the row a reader expands (`HostNotice::routed`,
/// an alert's own `Urgency::why`), unabridged.
///
/// `sent`/`failed` are the two outcomes a push attempt itself can have;
/// `why` is the routing reason (`Urgency::why`, or the damper's own line)
/// when neither is true.
pub fn push_status_short(sent: bool, failed: bool, why: &str) -> &'static str {
    if sent {
        return "Pushed";
    }
    if failed {
        return "Push failed";
    }
    if why.contains("not pushed again") {
        return "No push · repeat";
    }
    if why.contains("succeeded") {
        return "No push · succeeded";
    }
    if why.contains("stood aside") {
        return "No push · stood aside";
    }
    if why.contains("drift only") {
        return "No push · drift only";
    }
    if why.contains("nothing interrupted") {
        return "No push · resolved";
    }
    "No push · not urgent"
}

/// What kind of work an operation label names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Deploy,
    Backup,
    Update,
    Other,
}

pub fn op_kind(label: &str) -> OpKind {
    if BACKUP_OPS.contains(&label) {
        OpKind::Backup
    } else if UPDATE_OPS.contains(&label) {
        OpKind::Update
    } else if DEPLOY_OPS.contains(&label) {
        OpKind::Deploy
    } else {
        OpKind::Other
    }
}

/// The stack an operation is about, from its op name (`deploy-media`).
/// None for the host's own work and for a native's unit, which is not a
/// stack name.
pub fn op_stack(label: &str, op: &str) -> Option<String> {
    let prefixes: &[&str] = match label {
        "deploy" => &["deploy-"],
        "backup" | "scheduled-backup" | "backup-native" | "scheduled-backup-native" => &["backup-"],
        "update" | "scheduled-update" | "update-native" | "scheduled-update-native" => &["update-"],
        "restore" => &["restore-"],
        "destroy" => &["destroy-"],
        "forget" => &["forget-"],
        "resize" => &["resize-"],
        "set-enabled" => &["enable-", "disable-"],
        "adopt" => &["adopt-"],
        _ => &[],
    };
    prefixes
        .iter()
        .find_map(|p| op.strip_prefix(p))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// A notice's words: what is wrong, the consequence, what to do, and the
/// page that acts on it (a path under the dashboard, see [`page`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explained {
    pub title: String,
    pub stack: Option<String>,
    pub what: String,
    pub consequence: String,
    pub remedy: String,
    pub page: String,
}

/// The facts of one finished operation.
#[derive(Debug, Clone, Copy)]
pub struct OpFacts<'a> {
    pub op: &'a str,
    pub label: &'a str,
    pub ok: bool,
    pub deferred: Option<&'a str>,
    pub error: Option<&'a crate::error::OperatorError>,
    /// The incident bundle the failure left, by name.
    pub incident: Option<&'a str>,
}

/// The most a notice in the centre carries of one error text.
pub const NOTICE_TEXT_MAX: usize = 8 * 1024;

/// Masked like a push (fix-57), cut at [`NOTICE_TEXT_MAX`]: the centre
/// shows more than the phone, never a secret.
pub fn notice_text(s: &str) -> String {
    let masked = s
        .lines()
        .map(crate::executor::mask_secrets)
        .collect::<Vec<_>>()
        .join("\n");
    if masked.len() <= NOTICE_TEXT_MAX {
        return masked;
    }
    let mut cut = NOTICE_TEXT_MAX;
    while !masked.is_char_boundary(cut) {
        cut -= 1;
    }
    format!("{}… ({} bytes in all)", &masked[..cut], masked.len())
}

/// The retry for one failed operation: the dashboard button and the exact
/// command, or None when there is no manual verb for it.
fn retry(label: &str, stack: Option<&str>) -> Option<String> {
    let on_stack = |button: &str, verb: &str| {
        stack.map(|s| {
            format!(
                "the {} button on the {} stack page, or `homelab {} {}`",
                button, s, verb, s
            )
        })
    };
    match label {
        "deploy" => on_stack("Deploy", "deploy"),
        "backup" | "scheduled-backup" => on_stack("Back up", "backup"),
        "backup-native" | "scheduled-backup-native" => {
            on_stack("Back up (native)", "backup-native")
        }
        "update" | "scheduled-update" => on_stack("Update", "update"),
        "update-native" | "scheduled-update-native" => {
            on_stack("Update (native, own policy)", "update-native")
        }
        "release-update-native" | "scheduled-release-update" => Some(
            "Install newest release on the stack page of the stack that runs it, or \
             `homelab release-update-native <stack>`"
                .into(),
        ),
        "install-native" => Some(
            "Install a release on the stack page of the stack that runs it, or \
             `homelab release-update-native <stack>`"
                .into(),
        ),
        "rollback-native" => {
            Some("Roll back binary on the stack page, or `homelab rollback-native <stack>`".into())
        }
        "self-update" => {
            Some("Update the host on the host page, or `homelab release-update`".into())
        }
        "patch" => Some("Patch the fleet on the host page, or `homelab patch`".into()),
        "host-meta-backup" => Some(
            "Back up the host's own state on the host page, or `homelab backup-host-meta`".into(),
        ),
        "device-backup" => {
            Some("Back up devices on the host page, or `homelab backup-devices`".into())
        }
        "zfs-replicate" => {
            Some("ZFS replicate on the host page, or `homelab zfs-replicate`".into())
        }
        "restore" => on_stack("Restore", "restore").map(|s| format!("{s} <snapshot>")),
        _ => None,
    }
}

fn consequence_of(label: &str) -> &'static str {
    match (op_kind(label), label) {
        (_, "self-update") => {
            "The host keeps running the version it had; the swap rolls back on failure."
        }
        (_, "second-copy") => "The second copy of the backups is older than planned.",
        (_, "restic-check") => {
            "A backup repository was not verified; it may not restore until it is."
        }
        (OpKind::Backup, _) => {
            "There is no new restore point: the newest good backup is older than planned."
        }
        (OpKind::Update, _) => {
            "It stays on the version it had (an update rolls back on failure); check that it answers."
        }
        (OpKind::Deploy, _) => {
            "The stack may still run its previous version, or only part of the change is in place."
        }
        (OpKind::Other, _) => "The operation's change did not happen, or only in part.",
    }
}

/// What the words of a host operation's notice are.
pub fn explain_op(f: &OpFacts) -> Explained {
    let stack = op_stack(f.label, f.op);
    let page_path = match (&stack, f.label) {
        (Some(s), _) => page::stack(s),
        (None, "fleet-check") => page::CHECKS.into(),
        (None, _) => page::HOST.into(),
    };
    let subject = match &stack {
        Some(s) => format!("{} {}", f.label, s),
        None if f.op == f.label => f.label.to_string(),
        None => format!("{} ({})", f.label, f.op),
    };
    if f.ok {
        return Explained {
            title: format!("{}: done", subject),
            stack,
            what: String::new(),
            consequence: "Nothing to do.".into(),
            remedy: "Nothing to do.".into(),
            page: page_path,
        };
    }
    if let Some(why) = f.deferred {
        return Explained {
            title: format!("{}: stood aside", subject),
            stack,
            what: notice_text(why),
            consequence: "Nothing changed; it runs again at its next turn.".into(),
            remedy:
                "Nothing to do, unless it keeps standing aside: the reason says what it waits for."
                    .into(),
            page: page_path,
        };
    }
    let what = match f.error {
        Some(e) => notice_text(&format!("{}: {}", e.what, e.why)),
        None => "the host gave no reason".into(),
    };
    let mut remedy = String::new();
    if let Some(e) = f.error {
        let r = e.remedy.trim().trim_end_matches('.');
        if !r.is_empty() && r != "see transcript" {
            remedy.push_str(&format!("{}. ", r));
        }
    }
    match retry(f.label, stack.as_deref()) {
        Some(r) => remedy.push_str(&format!(
            "{} again: {}.",
            if remedy.is_empty() {
                "Run it"
            } else {
                "Then run it"
            },
            r
        )),
        None => remedy.push_str("The Jobs page and `homelab today` show what waits."),
    }
    if let Some(i) = f.incident {
        remedy.push_str(&format!(
            " `homelab incidents show {}` prints what happened.",
            i
        ));
    }
    Explained {
        title: format!("{} failed", subject),
        stack,
        what,
        consequence: consequence_of(f.label).into(),
        remedy,
        page: page_path,
    }
}

/// The words of the host's other events: the boot notice (`host-online`),
/// an automatically parked stack (`stack-disabled-<stack>`), anything else.
pub fn explain_event(op: &str, label: &str, ok: bool, error: Option<&str>) -> Explained {
    let what = error.map(notice_text).unwrap_or_default();
    if op == "host-online" {
        return if ok {
            Explained {
                title: "The host is back online".into(),
                stack: None,
                what,
                consequence: "Nothing to do.".into(),
                remedy: "Nothing to do.".into(),
                page: page::HOST.into(),
            }
        } else {
            Explained {
                title: "The host restarted with work interrupted".into(),
                stack: None,
                what,
                consequence:
                    "The interrupted operations did not finish; their stacks may be half-changed."
                        .into(),
                remedy: "Run each interrupted operation again from its stack page; \
                         `homelab today` lists what waits."
                    .into(),
                page: page::HOST.into(),
            }
        };
    }
    if let Some(stack) = op.strip_prefix("stack-disabled-") {
        return Explained {
            title: format!("{} was parked (disabled)", stack),
            stack: Some(stack.into()),
            what,
            consequence: "It gets no automatic update any more; its nightly backup goes on.".into(),
            remedy: format!(
                "Fix the cause above, then Enable on the {} stack page, or `homelab enable {}`.",
                stack, stack
            ),
            page: page::stack(stack),
        };
    }
    Explained {
        title: format!("{}: {}", op, if ok { "done" } else { "failed" }),
        stack: None,
        what,
        consequence: if ok {
            "Nothing to do.".into()
        } else {
            "Its change did not happen, or only in part.".into()
        },
        remedy: if ok {
            "Nothing to do.".into()
        } else {
            format!(
                "The Jobs page and `homelab today` show what waits ({}).",
                label
            )
        },
        page: page::HOST.into(),
    }
}

/// The nightly fleet check's notice: the whole report in the centre.
pub fn explain_fleet_check(findings: &[crate::ops::fleetcheck::Finding]) -> Explained {
    use crate::ops::fleetcheck::Severity;
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let (broken, drift) = (count(Severity::Broken), count(Severity::Drift));
    let mut parts = Vec::new();
    if broken > 0 {
        parts.push(format!("{} broken", broken));
    }
    if drift > 0 {
        parts.push(format!("{} drift", drift));
    }
    Explained {
        title: format!(
            "Nightly check: {}",
            if parts.is_empty() {
                "nothing alarming".into()
            } else {
                parts.join(", ")
            }
        ),
        stack: None,
        what: notice_text(&crate::ops::fleetcheck::render(findings)),
        consequence: if broken > 0 {
            "Something is not doing its job now (the broken items).".into()
        } else {
            "Nothing is down; drift bites on the next deploy or outage.".into()
        },
        remedy: "Each item carries its remedy above. The Checks page runs the check again; \
                 `homelab today` lists what waits."
            .into(),
        page: page::CHECKS.into(),
    }
}

/// The phone's version: the title and the first words of what to do, at
/// most [`PUSH_SHORT_MAX`] characters.
pub const PUSH_SHORT_MAX: usize = 300;

pub fn push_short(title: &str, remedy: &str) -> String {
    let full = if remedy.trim().is_empty() {
        title.to_string()
    } else {
        format!("{} — {}", title, remedy.trim())
    };
    let full = crate::executor::mask_secrets(&full);
    if full.chars().count() <= PUSH_SHORT_MAX {
        return full;
    }
    let cut: String = full.chars().take(PUSH_SHORT_MAX - 1).collect();
    format!("{}…", cut)
}

/// A push: the host's payload shape ([`op_payload_from`]) plus the page to
/// open, when there is one. Without a link it is byte for byte the old one.
pub fn push_payload(
    source: &str,
    op: &str,
    label: &str,
    ok: bool,
    short: Option<&str>,
    version: &str,
    click_url: Option<&str>,
) -> String {
    let base = op_payload_from(source, op, label, ok, short, version);
    // app-knowledge: an empty address (none configured) is no link.
    let Some(url) = click_url.filter(|u| !u.is_empty()) else {
        return base;
    };
    let mut v: serde_json::Value = serde_json::from_str(&base).unwrap_or_default();
    if let Some(o) = v.as_object_mut() {
        o.insert("click_url".into(), url.into());
    }
    v.to_string()
}

/// One notice as the host keeps it in `<state_dir>/notices.jsonl` and hands
/// it to the dashboard (`Command::Notices`). The host writes every event
/// here, urgent or not; the push is decided beside it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HostNotice {
    /// Grows with every notice, across restarts ([`next_seq`]); the
    /// dashboard's cursor.
    pub seq: u64,
    /// When it was written, unix seconds.
    pub at: u64,
    /// Since when it is so: the operation's start, or the event's moment.
    pub since: u64,
    pub op: String,
    pub label: String,
    pub ok: bool,
    #[serde(default)]
    pub deferred: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    pub title: String,
    pub what: String,
    pub consequence: String,
    pub remedy: String,
    /// A dashboard path ([`page`]).
    pub page: String,
    pub urgent: bool,
    /// [`Urgency::why`].
    pub routed: String,
    /// What became of the push: "sent", "failed: …", "centre only", …
    pub push: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incident: Option<String>,
    /// The request that asked for it, and the token that asked; None for
    /// the host's own work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub req: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    /// The nightly check's findings, each with its remedy, so the dashboard
    /// can offer a Fix button per finding it can run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub findings: Vec<crate::ops::fleetcheck::Finding>,
}

/// Every notice that parses; a torn line (power cut) is skipped.
pub fn parse_notices(text: &str) -> Vec<HostNotice> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<HostNotice>(l).ok())
        .collect()
}

/// The notices after `after`, oldest first, at most `limit`: the rest wait
/// for the next read, so none is skipped.
pub fn notices_after(all: Vec<HostNotice>, after: u64, limit: usize) -> Vec<HostNotice> {
    let mut v: Vec<HostNotice> = all.into_iter().filter(|n| n.seq > after).collect();
    v.sort_by_key(|n| n.seq);
    v.truncate(limit);
    v
}

/// The file after pruning: nothing older than `max_age_s`, then the oldest
/// dropped until it fits `max_bytes`. None when nothing changes.
pub fn prune_notices(text: &str, now: u64, max_age_s: u64, max_bytes: usize) -> Option<String> {
    let cutoff = now.saturating_sub(max_age_s);
    let mut lines: Vec<String> = parse_notices(text)
        .into_iter()
        .filter(|n| n.at >= cutoff)
        .filter_map(|n| serde_json::to_string(&n).ok().map(|s| s + "\n"))
        .collect();
    let mut total: usize = lines.iter().map(|l| l.len()).sum();
    let mut first = 0;
    while total > max_bytes && first < lines.len() {
        total -= lines[first].len();
        first += 1;
    }
    lines.drain(..first);
    let out = lines.concat();
    (out != text).then_some(out)
}

/// The next sequence number: the clock in milliseconds, but never less than
/// one past the last, so a clock that steps back cannot hide a notice
/// behind the dashboard's cursor.
pub fn next_seq(last: u64, now_ms: u64) -> u64 {
    now_ms.max(last + 1)
}
