//! State handling (AR4): typed JSON documents with a schema version, written
//! atomically through the Executor so tests capture them and power loss can
//! never leave a half-written file.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::executor::Executor;

pub const STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackState {
    pub vmid: u16,
    pub hostname: String,
    pub apps: Vec<String>,
    /// Set by the caller (host) — core never reads clocks.
    pub applied_at: u64,
    /// Unix time of the last successful backup (E4 scheduler input).
    #[serde(default)]
    pub last_backup: u64,
    /// B4: fingerprint of the intent that was last applied (see
    /// `manifest::intent_hash`); the client compares its local dir to this.
    #[serde(default)]
    pub applied_hash: String,
    /// Full manifest as last applied, so host-side operations (scheduled
    /// backup, fleet update) can run without the client being connected.
    #[serde(default)]
    pub manifest: Option<crate::manifest::StackManifest>,
    /// H8 (light variant): gates the nightly scheduler for this stack and is
    /// flipped off automatically when a nightly run fails (one loud message,
    /// then silence until the operator looks). Never touches the container's
    /// run state — manual `pct stop` and this flag are independent worlds.
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// fix-41: the gateway route file a deploy of this stack wrote. Only this
    /// file is ever retired; a route written by hand on the gateway (almanac,
    /// kyu, Home Assistant) is not the stack's to remove.
    #[serde(default)]
    pub route_file: Option<String>,
    /// fix-91: the `extra_routes` files a deploy of this stack wrote. Same
    /// rule as `route_file`: only these are ever retired or removed by a
    /// destroy, so a file on the gateway that no deploy recorded stays.
    #[serde(default)]
    pub extra_route_files: Vec<String>,
    /// T5: native services on this stack (bare binaries under systemd). A
    /// stack has either `manifest` (compose) or `natives`, never both. A list
    /// because the layout puts kyu, kyu-runner and http-switchboard on one
    /// container, and one hostname per container means they cannot be three
    /// separate stacks: `native.rs` forces `<vmid>-app-<stack>` and
    /// `guard_target` re-checks it against the live container.
    #[serde(default)]
    pub natives: Vec<crate::native::NativeServiceManifest>,
    /// S2: the step a deploy stopped at, or None when it ran to the end.
    ///
    /// State used to be written only by the last step, so a deploy that
    /// stopped earlier left no record at all. On 2026-09-01 the media stack
    /// failed at "start apps" and therefore did not exist as far as the
    /// orchestrator was concerned: no drift detection, no retention, and —
    /// the part that mattered — no nightly backup of 12 GB of application
    /// configuration, with nothing anywhere saying so. A container that is
    /// running and unknown is worse than one that is plainly broken.
    #[serde(default)]
    pub incomplete_step: Option<String>,
    /// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): the
    /// commit the last deploy was read from, with "+ N uncommitted file(s)"
    /// when the stack directory differed from it (`SourceRev::summary`).
    /// None for a deploy from a client that did not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_source: Option<String>,
}

/// A unix timestamp as `YYYY-MM-DD`, for messages that have to say when a
/// stored value was last refreshed.
///
/// Written by hand rather than pulled in with a date crate: this is the only
/// place the orchestrator formats a time, the input always comes from a value
/// someone else read off a clock (core never reads one itself), and a whole
/// dependency for twelve lines of arithmetic is a poor trade. The algorithm is
/// the standard civil-from-days one.
pub fn ymd(unix: u64) -> String {
    let days = (unix / 86_400) as i64;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{:04}-{:02}-{:02}", y, m, d)
}

/// Fold a stack's pre-T5 single-service `native` field into `natives`, in
/// the raw document, before it becomes a `StackState`.
///
/// legacy-native-field (expert panel, 2026-09-27): the field used to stay on
/// `StackState` beside `natives`, written as `null` on every save and carried
/// through the deploy, so one fact had two fields that could disagree after
/// a partial write. It is migrated here, once per read, and no longer exists
/// in the type, so it is never written. No schema bump: a binary of schema 1
/// reads the result unchanged, and a bump would make it refuse the file after
/// a self-update rollback.
fn migrate_legacy_native(stack: &mut serde_json::Value) {
    let Some(obj) = stack.as_object_mut() else {
        return;
    };
    let Some(one) = obj.remove("native") else {
        return;
    };
    if one.is_null() {
        return;
    }
    let list = obj
        .entry("natives")
        .or_insert_with(|| serde_json::Value::Array(Vec::new()));
    if let Some(list) = list.as_array_mut() {
        if !list.iter().any(|n| n.get("unit") == one.get("unit")) {
            list.push(one);
        }
    }
}

impl StackState {
    /// True for a stack the orchestrator supervises as systemd units.
    pub fn is_native(&self) -> bool {
        !self.natives.is_empty()
    }
}

fn enabled_default() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostState {
    #[serde(default)]
    pub schema_version: u32,
    #[serde(default)]
    pub stacks: BTreeMap<String, StackState>,
    /// H10: unix time of the last successful host-meta snapshot (vault,
    /// state, TLS, intent repo). 0 = never — the nightly run then takes one
    /// at the first opportunity.
    #[serde(default)]
    pub last_host_meta: u64,
    /// E8: unix time of the last successful ZFS snapshot+replication run.
    #[serde(default)]
    pub last_zfs: u64,
    /// G17: the checks only a person can answer, and whether anyone did.
    ///
    /// They were printed at the end of every deploy and stored nowhere, so
    /// "did anybody ever look at the front page after a deploy" had no answer
    /// — 94 of them across 28 files, measured 2026-09-02, and one of them is
    /// exactly the check that would have caught the empty homepage months
    /// earlier. The deploy that prints them now also registers them here, so
    /// the record is written by the thing that already knows.
    ///
    /// Keyed by `manualchecks::id_for`, which is stable across deploys as
    /// long as the wording does not change. If it does, the check is a
    /// different question and the old answer no longer applies.
    #[serde(default)]
    pub manual_checks: BTreeMap<String, ManualCheckRecord>,
    /// checks-automate (2026-09-30): each stack's nightly probes, as its last
    /// deploy declared them, keyed by `probes::id_for`. Measured by every
    /// fleet check (nightly and `homelab check`), never answered by a person.
    #[serde(default)]
    pub probes: BTreeMap<String, ProbeRecord>,
    /// O10 / app-knowledge (2026-09-30): each app's busy check as its
    /// stack's last deploy declared it, keyed `stack/app`.
    #[serde(default)]
    pub busy_checks: BTreeMap<String, String>,
    /// G16: unix time of the last notification that actually arrived.
    #[serde(default)]
    pub last_notify_ok: u64,
    /// G16: unix time of the last one that reached no route at all.
    #[serde(default)]
    pub last_notify_failed: u64,
    /// G16: what the last failure said. Kept so the finding can quote it
    /// rather than say "something went wrong".
    #[serde(default)]
    pub last_notify_error: Option<String>,
    /// G14: unix time of the last restore drill that PROVED something.
    #[serde(default)]
    pub last_restore_drill: u64,
    /// Which repository that was, so the finding can name it.
    #[serde(default)]
    pub last_restore_drill_repo: String,
    /// Why the last drill proved nothing. None = it did.
    #[serde(default)]
    pub last_restore_drill_error: Option<String>,
    /// Round-robin cursor, so a year of drills covers every repository
    /// instead of proving the same one twelve times.
    #[serde(default)]
    pub restore_drill_index: usize,
    /// fix-62: the drill's record per repository, so a failure is remembered
    /// until that repository passes instead of until any other one does.
    #[serde(default)]
    pub restore_drills: BTreeMap<String, DrillRecord>,
    /// fix-65: the alarming set the nightly fleet check last sent
    /// (`fleetcheck::report_fingerprint`), and when. Empty when the last
    /// night had nothing alarming, so a problem that returns is sent again.
    #[serde(default)]
    pub last_fleet_report_fp: String,
    #[serde(default)]
    pub last_fleet_report_at: u64,
    /// fix-59 (failed-update-parks-backups, 2026-09-27): stacks whose
    /// automatic updates a failed nightly update parked, with the unix time
    /// it happened. Only updates: a parked stack keeps its nightly backup.
    /// The old park set `enabled = false`, which stopped the backups too, so
    /// one bad upstream image meant no backup until somebody typed `homelab
    /// enable`. `homelab enable <stack>` clears the entry.
    #[serde(default)]
    pub updates_parked: BTreeMap<String, u64>,
    /// ask-8 / ask-9: what a stack, app or native unit left behind when it
    /// left the files, keyed by `<stack>` for a whole stack and
    /// `<stack>/<name>` for an app or unit that left a stack still running.
    ///
    /// Kenny, 2026-09-27: backups, /appdata and vault copies of anything
    /// retired are KEPT FOREVER by default. Nothing automatic ever deletes
    /// them; `homelab wipe <key>` does, after the name is typed, and the
    /// fleet check names every entry (Noted) so what is being kept never
    /// becomes something nobody knows is there.
    #[serde(default)]
    pub retired: BTreeMap<String, RetiredRecord>,
    /// fix-94 (crowdsec-home-ip, 2026-09-27): the house's own public address
    /// as the CrowdSec whitelist on the gateway holds it. None = no check has
    /// run, or none has ever found an address to keep.
    #[serde(default)]
    pub home_address: Option<String>,
    /// Unix time of the last check, whatever it found.
    #[serde(default)]
    pub home_address_checked: u64,
    /// Why the last check could not read the router's WAN address. None = it
    /// could. Kept so the fleet check can say the whitelist is running on a
    /// last known value, and since when.
    #[serde(default)]
    pub home_address_error: Option<String>,
    /// fix-143 (Cloudflare nightly comparison, owner decision 2026-10-01):
    /// unix time of the last nightly edge comparison the HOST ran (the one
    /// `homelab check` runs from the workstation is unaffected and keeps its
    /// own rhythm). 0 = never — not configured counts as never too, so
    /// setting `cloudflare_token` later runs it at the first opportunity.
    #[serde(default)]
    pub last_edge_check: u64,
    /// How many findings that comparison reported, 0 meaning it agreed.
    #[serde(default)]
    pub last_edge_findings: usize,
    /// Why the comparison could not run at all (no token, the API did not
    /// answer, the capture does not read). None when it ran, whatever it
    /// found — this is "not compared", never counted as a finding itself.
    #[serde(default)]
    pub last_edge_error: Option<String>,
    /// fix-83 (manual-images-latest-unpinned, 2026-09-27): per stack, per
    /// container, the image and digest each `manual` container ran when the
    /// nightly round last looked. The stack file says what SHOULD run; this
    /// says what did, read off the container.
    #[serde(default)]
    pub running_images: BTreeMap<String, BTreeMap<String, crate::ops::pins::RunningImage>>,
    /// fix-83: the last answer about each declared upstream, cached so GitHub
    /// is asked at most once a night.
    #[serde(default)]
    pub upstream_releases: BTreeMap<String, crate::ops::pins::UpstreamRelease>,
    /// fix-96 (single-offsite-copy-no-integrity-check, 2026-09-27): when the
    /// nightly copy of every repository into the second repository set last
    /// ran, and what each repository's copy did. 0 = never.
    #[serde(default)]
    pub last_second_copy: u64,
    #[serde(default)]
    pub second_copies: BTreeMap<String, CopyRecord>,
    /// fix-96: when the rotating `restic check` last ran, and its record per
    /// repository (both copies).
    #[serde(default)]
    pub last_integrity_check: u64,
    #[serde(default)]
    pub integrity: BTreeMap<String, IntegrityRecord>,
    /// fix-147 (restore-check-failure, 2026-09-27): data a deploy found
    /// empty and whose backup it could not check, keyed by `<stack>:<path>`
    /// or `<stack>:<unit>`. Kept until a later check of the same thing
    /// succeeds.
    #[serde(default)]
    pub restore_check_failures: BTreeMap<String, RestoreCheckFailure>,
}

/// fix-147: one empty data directory (or native unit) whose backup a deploy
/// could not check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreCheckFailure {
    pub stack: String,
    /// The directory, or the native unit, that was found empty.
    pub what: String,
    /// Unix time of the deploy that could not check it.
    pub at: u64,
    /// What the check said.
    pub why: String,
}

/// fix-96: what the nightly `restic copy` of one repository did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CopyRecord {
    /// Unix time of the last attempt, copied or not.
    #[serde(default)]
    pub last_attempt: u64,
    /// Unix time of the last copy that completed.
    #[serde(default)]
    pub last_ok: u64,
    /// Why the last attempt failed. None = it did not.
    #[serde(default)]
    pub last_error: Option<String>,
}

/// fix-96: what `restic check` last said about one repository, per copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IntegrityRecord {
    /// Unix time of the last check of this repository, passed or not.
    #[serde(default)]
    pub last_check: u64,
    /// Unix time of the last check that also read a subset of the data.
    #[serde(default)]
    pub last_data_read: u64,
    /// Which slice (`n` of `--read-data-subset=n/t`) the next data read takes,
    /// so successive reads walk the whole repository instead of sampling.
    #[serde(default)]
    pub next_subset: u32,
    /// Why the last check of the Google Drive copy failed. None = it passed.
    #[serde(default)]
    pub drive_error: Option<String>,
    /// Why the last check of the second copy failed. None = it passed or was
    /// not checked.
    #[serde(default)]
    pub local_error: Option<String>,
}

/// fix-62: what the restore drill knows about one repository.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DrillRecord {
    /// Unix time of the last drill of this repository, passed or not.
    #[serde(default)]
    pub last_attempt: u64,
    /// Unix time of the last drill of this repository that proved a restore.
    #[serde(default)]
    pub last_pass: u64,
    /// Why the last drill of this repository proved nothing. None = it did.
    #[serde(default)]
    pub last_error: Option<String>,
}

/// What kind of thing left the files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetiredKind {
    /// A whole stack: destroyed, or forgotten after its container was lost.
    Stack,
    /// A compose app that left a stack that still exists.
    App,
    /// A native unit dropped from a stack's `natives:`.
    Unit,
}

/// One retired stack, app or unit, and exactly what it left behind (ask-9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetiredRecord {
    pub kind: RetiredKind,
    /// The stack it belonged to (the stack itself for `Stack`).
    pub stack: String,
    /// The stack, app or unit name.
    pub name: String,
    pub vmid: u16,
    /// Unix time it left — the destroy, forget or deploy that saw it go.
    pub retired_at: u64,
    /// Restic repository names under `restic_base` (`<owner>-config`).
    #[serde(default)]
    pub repos: Vec<String>,
    /// Host directories under /appdata it kept its configuration in.
    #[serde(default)]
    pub appdata: Vec<String>,
    /// Vault paths on the host (a stack's whole `secrets/<stack>` directory,
    /// or one app's or unit's copies).
    #[serde(default)]
    pub vault: Vec<String>,
}

/// One thing only a person can confirm, and the last word on it.
/// checks-automate: one probe as a deploy registered it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeRecord {
    pub stack: String,
    pub app: String,
    pub vmid: u16,
    pub probe: crate::checks::Probe,
    /// Where the application is opened (checks-link).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualCheckRecord {
    pub stack: String,
    pub app: String,
    /// The question, verbatim from `checks.yml`.
    pub text: String,
    /// First time a deploy printed it.
    pub registered_at: u64,
    /// Unix time of the last answer. None = nobody has ever answered.
    #[serde(default)]
    pub answered_at: Option<u64>,
    /// What the answer was. None while unanswered.
    #[serde(default)]
    pub ok: Option<bool>,
    /// Whatever the person wanted to add. Empty is normal.
    #[serde(default)]
    pub note: String,
    /// fix-65: the stack's `applied_hash` when this was answered. Only a
    /// deploy that changed the stack's files reopens the question; `None`
    /// is an answer from before the field, judged by the old rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answered_hash: Option<String>,
    /// fix-65: a deliberate "not ok" that is accepted until this unix time,
    /// with the reason in `note`. Noted until then, Broken after.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accepted_until: Option<u64>,
    /// checks-onetime (2026-09-30): answered `ok` once is answered for good;
    /// a changed stack does not reopen it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub once: bool,
    /// checks-link (2026-09-30): where the application is opened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// password-chain-bus-factor (owner decision "Alleen de
    /// 90-dagen-controle", 2026-10-01): for a STANDING check only (one that
    /// is not registered by any stack's deploy, `manualchecks::ensure_standing`)
    /// — how many days after it was last answered it is due again,
    /// whatever the answer was. `checks-interval` (2026-09-30) deliberately
    /// removed this for ordinary per-app checks; a standing check is a
    /// different thing by design, asked on a clock because nothing a deploy
    /// does ever reopens it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recur_days: Option<u64>,
}

/// fix-51 (expert panel, state-writes-race, 2026-09-27): the lock that
/// serialises every write of one state file in this process.
///
/// Per path rather than per `StateStore`, because every caller builds its own
/// store: the nightly batch runs three backups at once, each recording its
/// notification outcome through a fresh store, and a lock inside the store
/// would have been three locks.
fn path_lock(path: &str) -> Arc<tokio::sync::Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(path.to_string())
        .or_default()
        .clone()
}

pub struct StateStore<'a> {
    exec: &'a dyn Executor,
    path: String,
}

impl<'a> StateStore<'a> {
    pub fn new(exec: &'a dyn Executor, state_dir: &str) -> Self {
        Self {
            exec,
            path: format!("{}/state.json", state_dir),
        }
    }

    /// Load state. A MISSING file is a fresh install (empty state); an
    /// UNPARSEABLE file is an error — silently continuing with an empty
    /// fleet would stop all scheduled work and the next save would erase
    /// every other stack permanently (hardening H7). The corrupt content is
    /// preserved next to the original before failing.
    ///
    /// fix-50 (expert panel, state-load-error-empty-fleet, 2026-09-27): the
    /// same holds for a file that exists and cannot be READ. Every read error
    /// used to count as "missing", so an EACCES, EIO or EMFILE loaded the
    /// empty fleet this comment warns about. Only `NotFound` is fresh now.
    pub async fn load(&self) -> Result<HostState, CoreError> {
        let raw = match self.exec.read_file(&self.path).await {
            Ok(raw) => raw,
            Err(CoreError::NotFound(_)) => return Ok(HostState::default()),
            Err(e) => {
                return Err(CoreError::State(format!(
                    "state.json exists but cannot be read ({}) — refusing to continue with an empty fleet; fix the file's access before running mutating operations",
                    e
                )))
            }
        };
        // T5: state written before native services became a list.
        let parsed = serde_json::from_str::<serde_json::Value>(&raw).and_then(|mut doc| {
            if let Some(stacks) = doc.get_mut("stacks").and_then(|s| s.as_object_mut()) {
                stacks.values_mut().for_each(migrate_legacy_native);
            }
            serde_json::from_value::<HostState>(doc)
        });
        match parsed {
            Ok(state) if state.schema_version <= STATE_SCHEMA_VERSION => Ok(state),
            Ok(state) => Err(CoreError::State(format!(
                "state.json schema v{} is newer than this binary understands (v{}) — refusing to touch it; update the host binary. File: {}",
                state.schema_version, STATE_SCHEMA_VERSION, self.path
            ))),
            Err(e) => {
                let quarantine = format!("{}.corrupt", self.path);
                let _ = self.exec.write_file(&quarantine, &raw, 0o600).await;
                Err(CoreError::State(format!(
                    "state.json does not parse ({}) — copy preserved at {}; fix or remove the original before running mutating operations",
                    e, quarantine
                )))
            }
        }
    }

    /// Load, change and save state as one step, under the file's lock.
    ///
    /// fix-51: a bare `load` then `save` from two tasks at once lets the
    /// later save carry the earlier load, and whatever the other task wrote
    /// in between is gone: a `last_backup`, a notification outcome, a manual
    /// check answer. Every short read-modify-write goes through here.
    pub async fn update<R>(
        &self,
        change: impl FnOnce(&mut HostState) -> R,
    ) -> Result<R, CoreError> {
        let lock = path_lock(&self.path);
        let _held = lock.lock().await;
        let mut state = self.load().await?;
        let out = change(&mut state);
        self.save_unlocked(state).await?;
        Ok(out)
    }

    /// Save under the same lock `update` takes, so a plain save can never
    /// land between an update's load and its save.
    pub async fn save(&self, state: HostState) -> Result<(), CoreError> {
        let lock = path_lock(&self.path);
        let _held = lock.lock().await;
        self.save_unlocked(state).await
    }

    async fn save_unlocked(&self, mut state: HostState) -> Result<(), CoreError> {
        state.schema_version = STATE_SCHEMA_VERSION;
        let raw =
            serde_json::to_string_pretty(&state).map_err(|e| CoreError::State(e.to_string()))?;
        self.exec.write_file(&self.path, &raw, 0o644).await
    }
}
