//! Wire protocol for the single CLIENT ↔ HOST line (AR5).
//!
//! Domain types (manifests, deploy specs) live in homelab-core and are
//! re-exported here so both sides compile against the same definitions.
//! Frames are bare JSON: the host opens with `ServerMsg::Hello` (its version
//! and `PROTO_VERSION`), the client sends `RpcRequest`s. Version skew is
//! handled by the client, which refuses a mutating command to a host older
//! than itself. (AR5 amended 2026-09-27: the `{v, topic, id, payload}`
//! envelope it named was defined here and never used; it is gone.)

use serde::{Deserialize, Serialize};

pub use homelab_core::manifest::{
    BootSpec, DeploySpec, FileBlob, GatewayRoute, LxcSpec, MountSpec, NetworkSpec, ResourceSpec,
    SourceRev, StackManifest,
};
pub use homelab_core::native::{BackupPause, NativeServiceManifest};
pub use homelab_core::retention::RetentionTier;

/// Sent in `Hello`. Bumped when a change would make an older peer misread a
/// frame (a field or message removed, renamed or given a new meaning); an
/// added field or command is covered by the client's version gate and does
/// not bump it (AR5, amended 2026-09-27). No such change has been made, so
/// it is still 1.
pub const PROTO_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    Ping,
    Status,
    DeployStack(Box<DeploySpec>),
    /// T85: one native binary, sent BEFORE the deploy that installs it.
    ///
    /// `DeployStack` used to carry every service binary of a stack in one
    /// message; three of them came to 94.7 MiB against a 64 MiB frame limit
    /// and the host reset the connection without a log line (F303). Raising
    /// the ceiling was a postponement — a stack with five services would
    /// have met it again, and the guard would then have said "split" while
    /// nothing split. The host keeps the staged bytes under its state
    /// directory until the deploy that follows consumes them.
    StageNativeBinary {
        stack: String,
        unit: String,
        binary_b64: String,
    },
    /// F6: self-diagnosis checks.
    Doctor {
        /// feat-platform-1 (homelab-admin, 2026-09-28): answer JSON instead
        /// of text. Never sent as false, so an older host reads the CLI's
        /// request unchanged.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        json: bool,
    },
    /// AR14: list captured incident bundles.
    Incidents {
        /// feat-platform-1 (homelab-admin, 2026-09-28): answer JSON instead
        /// of text. Never sent as false, so an older host reads the CLI's
        /// request unchanged.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        json: bool,
    },
    /// fix-131: one bundle, readable from the workstation: the error, the
    /// versions and the end of the transcript. `name` is a directory name
    /// from `Incidents`.
    IncidentShow {
        name: String,
    },
    /// Structured fleet snapshot for the TUI dashboard.
    GetState,
    /// C2: gated destroy. `confirm` must equal the stack name.
    DestroyStack {
        manifest: Box<StackManifest>,
        confirm: String,
        /// Kenny's B2 (2026-08-31): a destroy backs the stack up first and
        /// refuses when that fails, because a backup you may skip is not a
        /// backup. This is the deliberate way past it — `--no-backup` on the
        /// command line — so skipping is a decision somebody takes rather
        /// than something that happens.
        #[serde(default)]
        skip_backup: bool,
    },
    /// E1: back up a stack's /appdata.
    BackupStack(Box<StackManifest>),
    /// E2: restore a stack from a snapshot (default "latest").
    RestoreStack {
        manifest: Box<StackManifest>,
        snapshot: String,
        /// fix-64: the stack name as the operator typed it. The host refuses
        /// a restore without it; absent from clients before fix-64.
        #[serde(default)]
        confirm: Option<String>,
        /// fix-64: `--no-safety-copy`, the deliberate way past the copy of
        /// the current data a restore takes first.
        #[serde(default)]
        skip_safety_copy: bool,
        /// fix-112: restore one app of the stack; None (and every client
        /// before fix-112) restores the whole stack.
        #[serde(default)]
        app: Option<String>,
    },
    /// D9/B6: managed update with rollback. `app: None` = whole stack.
    UpdateStack {
        manifest: Box<StackManifest>,
        app: Option<String>,
    },
    /// H5: replace the HOST binary. `binary_b64` is the new executable,
    /// base64-encoded; the host stages, selfchecks, installs with an armed
    /// rollback, and restarts itself.
    SelfUpdateHost {
        binary_b64: String,
    },
    /// Owner decision 2026-09-30 (item 2): restart `homelab-host.service`
    /// itself, so a `host.toml` change marked `Apply::Restart` takes
    /// effect without a second host update. Nothing is replaced and no
    /// rollback is armed; the host schedules the restart through systemd
    /// (`systemctl restart --no-block`) so this reply reaches the client
    /// first. Refused while an operation runs.
    RestartHost,
    /// H6: apt dist-upgrade every managed stack (from host state),
    /// sequentially.
    PatchFleet,
    /// A6: run a shell command inside a managed LXC. Deny-by-default —
    /// requires exec_enabled in host config; no-touch vmids always refused;
    /// every invocation is audit-logged.
    ExecIn {
        vmid: u16,
        command: String,
    },
    /// B8: build the golden template container (docker + guards baked in)
    /// on a dedicated temp vmid, then convert it to a Proxmox template.
    BuildTemplate {
        temp_vmid: u16,
        version: u32,
        /// O2: two templates exist, because `pct clone` cannot change a
        /// privilege level and CT 105/106 must stay privileged. Defaults to
        /// unprivileged, which is what every stack but those two wants.
        #[serde(default = "yes")]
        unprivileged: bool,
        /// The base OS to bake, as a vztmpl path. Absent keeps the host's
        /// own default, so an older client still builds what it always did.
        ///
        /// Carried on the command rather than pinned in the host because the
        /// fleet is moving from Debian 12 to 13 one container at a time
        /// (Kenny, 2026-09-09), and during that move BOTH have to be
        /// buildable — a single pinned default would make the other one
        /// unreachable exactly while it is needed.
        #[serde(default)]
        base_template: Option<String>,
    },
    /// C4: hot-apply manifest resources to the running container (grow only).
    ApplyResources(Box<StackManifest>),
    /// C5: list available OS templates + clonable golden templates.
    ListTemplates,
    /// D6: fetch the applied (non-secret) files of a stack from the host's
    /// intent repo, so the client can render a real change plan.
    GetApplied {
        stack: String,
    },
    /// G8: read the host's runtime settings.
    GetConfig,
    /// G8: replace the host's runtime settings (persisted to host.toml).
    SetConfig(Box<HostConfigView>),
    /// H10: snapshot the host's own crown jewels (secrets vault, state.json,
    /// TLS material, intent repo) into the dedicated host-meta repo. Runs
    /// nightly; this is the on-demand trigger.
    BackupHostMeta,
    /// C7: adopt an existing hand-built native-service container — verify
    /// it is what the manifest claims, record it in state, never restart it.
    AdoptService(Box<NativeServiceManifest>),
    /// T11: install a native service's binary and unit file into a container
    /// the deploy has already created. The CLIENT downloads the release and
    /// verifies its checksum before sending, so the host needs no GitHub
    /// credential and never sees an unverified binary.
    InstallNative {
        manifest: Box<NativeServiceManifest>,
        /// The verified binary, base64. Decoded on the container.
        binary_b64: String,
        /// The systemd unit file, straight from the repository.
        unit_file: String,
    },
    /// TUI parity round (dash-install-native, Kenny 2026-09-28): install
    /// the release `tag` of a native service, downloaded and verified by the
    /// HOST (the dashboard on CT 120 has no `gh`): the minisign signature
    /// over SHA256SUMS, then the binary against it; an unsigned release is
    /// refused. The manifest and the unit file come from the repository, as
    /// with `InstallNative`. Sent by the dashboard only, to a host of 3.63.0
    /// or later; the CLI keeps `InstallNative` (it downloads with `gh`).
    InstallNativeRelease {
        manifest: Box<NativeServiceManifest>,
        unit_file: String,
        tag: String,
    },
    /// C7: on-demand backup of a native stack (pct-exec tar into restic).
    /// The stack must be adopted; the host reads the manifest from state.
    BackupNative {
        stack: String,
    },
    /// C7: run the app's own self-update under homelab supervision
    /// (preserve binary, restart-if-changed, health check, armed rollback).
    UpdateNative {
        stack: String,
    },
    /// B1: the orchestrator's own release update of a native stack, on
    /// demand — the same thing the nightly round does for services whose
    /// `update_policy` is `auto`, for every service of the stack that
    /// declares a release_repo.
    ReleaseUpdateNative {
        stack: String,
    },
    /// fix-114: go back to a native unit's kept previous binary, and park the
    /// stack's automatic updates. `unit: None` = the stack's only unit.
    RollbackNative {
        stack: String,
        #[serde(default)]
        unit: Option<String>,
    },
    /// T69: the operator's answer to a suspended step. `allow` false means
    /// stop; a question that is never answered times out on the host into
    /// `Unattended`, which is not the same thing and says so.
    Answer {
        /// fix-66: on the wire as `question`. A command is flattened into
        /// `RpcRequest`, whose own `id` is the request's; a field called `id`
        /// here wrote a second `id` key into the same object, the host could
        /// not parse the frame and dropped it, and no answer ever arrived
        /// (found 2026-09-27 by the first round-trip test).
        #[serde(rename = "question")]
        id: u64,
        allow: bool,
        /// arch-host-link: the `boot` of the question being answered. None
        /// from the CLI and TUI, which answer on the session that asked.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        boot: Option<String>,
    },
    /// E8: run the configured ZFS snapshot + replication jobs now.
    ZfsReplicate,
    /// G1: apply the runaway guards to a container. They exist and are
    /// applied at bootstrap of a container the orchestrator built — which on
    /// 2026-08-31 meant two of eight, while CT 104 carried 907 MB of docker
    /// logs and a 397 MB journal on a 30 GB disk. This makes them reachable
    /// for a guest the orchestrator did not create, which is where the
    /// unbounded growth actually is.
    ApplyGuards {
        vmid: u16,
    },
    /// Drop a stack's record from host state without touching the container.
    ///
    /// A stack that is renamed, or moved out of this orchestrator's care,
    /// leaves a record behind that no longer matches anything. The fleet
    /// check then reports it as broken forever, and a report that carries a
    /// permanent false finding is a report people stop reading.
    ///
    /// Refuses while a container still answers to the recorded hostname:
    /// that record is not stale, it is live, and forgetting it would leave a
    /// managed stack unbacked-up and unwatched with nothing to say so.
    ForgetStack {
        stack: String,
    },
    /// ask-8: gated destroy of a stack whose directory is gone, from the
    /// manifest the host recorded when it last applied it. Same gates as
    /// `DestroyStack`: `confirm` must equal the stack name, and the no-touch
    /// list and hostname guard run on the host. Sent by `homelab apply` (and
    /// `homelab destroy` when the directory is missing); never by the nightly
    /// round.
    DestroyRecorded {
        stack: String,
        confirm: String,
        #[serde(default)]
        skip_backup: bool,
    },
    /// ask-9: delete what a retired stack, app or unit kept — its restic
    /// repositories, /appdata directories and vault copies — and its record.
    /// `confirm: None` only returns the list of what would be deleted;
    /// `Some(name)` deletes, and must equal `name`. Never automatic.
    WipeRetired {
        name: String,
        #[serde(default)]
        confirm: Option<String>,
    },
    /// H2b: remove files under `/opt/<stack>/` that the repository no longer
    /// has. `confirm` must equal the stack name.
    ///
    /// Kenny's form H2b made this the only remover; since ask-8
    /// (2026-09-27, `Automatisch bij deploy`) the deploy removes what the
    /// files no longer declare itself, so this is mostly a no-op kept for a
    /// container that has not been deployed since.
    PruneOrphans {
        manifest: Box<StackManifest>,
        spec: Box<DeploySpec>,
        confirm: String,
    },
    /// Y4: hold the repository against reality and report every difference.
    /// The client sends what only it can see — the vmid each stack directory
    /// claims — and the host adds what only it can see: which containers
    /// exist, how they are named, when each stack was last backed up, and
    /// whether every gateway route reaches something that answers.
    FleetCheck {
        stack_files: Vec<(String, u16)>,
        /// fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift):
        /// what each stack's files say, so the check can compare them with
        /// what the host last applied. Empty from an older client.
        #[serde(default)]
        digests: Vec<homelab_core::ops::fleetcheck::StackDigest>,
        /// feat-platform-1 (homelab-admin, 2026-09-28): answer JSON instead
        /// of text. Never sent as false, so an older host reads the CLI's
        /// request unchanged.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        json: bool,
        /// fix-110: `config/host.toml` as the client's working copy reads
        /// it (non-secret keys only), so the check can compare it with the
        /// host's own running settings. `None` from a client with no such
        /// file (an older client, the nightly round, or a repository not
        /// yet carrying it) skips the comparison — backwards compatible.
        #[serde(default)]
        host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    },
    /// fix-68: doctor, the fleet check (with its manual checks) and the open
    /// incident bundles as one list and one verdict. The reply's message is
    /// a JSON `homelab_core::ops::today::Today`, rendered by the caller.
    Today {
        stack_files: Vec<(String, u16)>,
        /// fix-142: as in `FleetCheck`.
        #[serde(default)]
        digests: Vec<homelab_core::ops::fleetcheck::StackDigest>,
        /// fix-110: as in `FleetCheck`.
        #[serde(default)]
        host_config: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    },
    /// H8 (light): flip a stack's enabled flag. Disabled = nightly scheduler
    /// skips it + onboot cleared; enabled = back in rotation + onboot per
    /// manifest. Never starts or stops containers.
    SetStackEnabled {
        stack: String,
        enabled: bool,
    },
    /// Route A: fetch every configured device's own configuration now.
    ///
    /// It only ever ran inside the nightly round, which meant the only way to
    /// find out whether it worked was to wait until 04:00 and look afterwards
    /// — and on 2026-09-03 it turned out it had never worked at all (F205,
    /// F258). A backup you cannot run is a backup nobody verifies.
    BackupDevices,
    /// replace-homepage (2026-09-30): the start page's tiles as the stacks
    /// declare them, each with its reading taken now unless `bare` (the
    /// dashboard's minute watch needs only where each tile opens).
    /// Read-only; JSON.
    Tiles {
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        bare: bool,
    },
    /// feat-overview-10 (homelab-admin, 2026-10-01): every restic snapshot
    /// time for the named stacks (every managed docker stack when empty),
    /// for the dashboard's own backup calendar — its own query, separate
    /// from the backup page's reads, so the two pages cannot collide.
    /// Read-only; JSON `{"stacks": {"<name>": [unix, …]}, "skipped":
    /// ["<name>: why"]}`. Reaches the repositories over the network
    /// (rclone), so it can take a while on a slow link — the caller treats
    /// it like `FleetCheck`.
    BackupCalendar {
        #[serde(default)]
        stacks: Vec<String>,
    },
    /// G17: the questions only a person can answer, as the host has them on
    /// record. Read-only; the deploy is what puts them there.
    ListManualChecks {
        /// feat-platform-1 (homelab-admin, 2026-09-28): answer JSON instead
        /// of text. Never sent as false, so an older host reads the CLI's
        /// request unchanged.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        json: bool,
    },
    /// G17: one person's answer to one of those questions.
    ///
    /// `check_id`, not `id`: `RpcRequest` flattens this enum into the same
    /// JSON object as its own `id`, so a field of that name collides with the
    /// envelope and the whole request stops parsing.
    AnswerManualCheck {
        check_id: String,
        ok: bool,
        note: String,
        /// fix-65: a deliberate "not ok" accepted for this many days, with
        /// the reason in `note`. None = an ordinary answer.
        #[serde(default)]
        accept_days: Option<u32>,
    },
    /// arch-host-link (homelab-admin, 2026-09-28): what this session asks
    /// of the host. `reads_beside_queue`: read-only commands run at once
    /// instead of waiting behind a deploy; the session then tells replies
    /// apart by `RpcResponse.id`. The CLI and TUI never send this, so their
    /// replies keep arriving in the order they asked (the TUI matches them
    /// by order and shape, not by id).
    SessionOptions {
        reads_beside_queue: bool,
    },
    /// feat-platform-3: what is running now and the newest lines, for a
    /// client that (re)connects mid-operation. Answered as `CurrentOpView`.
    CurrentOp,
    /// arch-history: what the host did since `since` (unix seconds), newest
    /// `limit` entries. Answered as JSON `{ "entries": [...] }`, each a
    /// `homelab_core::history::HistoryEntry`.
    History {
        since: u64,
        #[serde(default = "history_limit_default")]
        limit: usize,
    },
    /// Decision notify-routing (homelab-admin, 2026-09-30): the host's
    /// notices after `after` (a `seq`), oldest first, at most `limit`.
    /// Answered as JSON `{ "notices": [...], "last_seq": n }`, each a
    /// `homelab_core::notify::HostNotice`; `last_seq` is the newest the host
    /// has, so a first read can start from now.
    Notices {
        after: u64,
        #[serde(default = "notices_limit_default")]
        limit: usize,
    },
    /// feat-settings-1 (homelab-admin, 2026-09-28): every key of host.toml
    /// with its value, as JSON [`HostConfigFile`] in the reply's message;
    /// a secret only as "set". Answered to the asking session alone: unlike
    /// `GetConfig`, whose `Config` frame goes to every session and makes the
    /// TUI's settings screen drop its unsaved edits, nothing is broadcast.
    GetHostConfig,
    /// feat-settings-1: change keys of host.toml, checked with the start-up
    /// validation before anything is written. `changes` maps a key to its new
    /// value, or to null to remove it (the host then uses its default).
    /// `expect_sha256` is the file as `GetHostConfig` read it: an edit made
    /// meanwhile (over ssh, or by the TUI) is refused, never overwritten.
    /// Keys `homelab_core::hostconfig` marks as secret, locked or ssh-only are
    /// refused. Answered as JSON [`HostConfigSaved`]; nothing is broadcast.
    SetHostConfig {
        changes: std::collections::BTreeMap<String, serde_json::Value>,
        expect_sha256: String,
    },
    /// fix-120 (per-machine tokens, owner decision 2026-10-01): mint a new
    /// `[[tokens]]` entry at `scope`, named `name` (must be unique, not
    /// "legacy"), and write it to host.toml at once — unlike `tokens`
    /// itself, which `SetHostConfig` refuses (arch-self), this is its own
    /// narrow, scope-`All`-only action. Answered as JSON [`TokenIssued`];
    /// the plaintext token is in that one reply and nowhere else — never
    /// logged, never re-readable.
    TokenIssue {
        name: String,
        scope: Scope,
    },
    /// fix-120: every token the host currently trusts, name and scope only
    /// — never a hash, never a plaintext token. Answered as JSON
    /// `Vec<TokenView>`, the legacy single `token` included as `"legacy"`
    /// when one is set.
    TokenList,
    /// fix-120: remove the `[[tokens]]` entry named `name`, at once, so the
    /// token stops working without touching any other machine's. Refused
    /// for `"legacy"` (the single `token` key; cleared over ssh, see
    /// OPERATIONS_RUNBOOK's migration note) and for a name that is not
    /// there.
    TokenRevoke {
        name: String,
    },
    /// fix-110 (homelab-admin, 2026-10-01): declarative host settings — the
    /// repository's `config/host.toml` sent whole, the way `homelab apply`
    /// sends a stack's files. `toml` holds every non-secret key the
    /// repository declares (any secret key in it is refused: a secret lives
    /// in the host's own vault, never in the repository). The host lays it
    /// over its own host.toml, keeping only the secret keys it already has
    /// and dropping anything the repository does not declare, validates the
    /// result with the same parser `host.toml` has always used, and writes
    /// it. `expect_sha256`, when given, must match the host.toml
    /// `GetHostConfig` last answered — an edit made meanwhile (over ssh, or
    /// a TUI save) is refused, never overwritten; `None` skips the check
    /// (the nightly round and a first `homelab host apply` on a host nobody
    /// has read yet have nothing to compare against). Answered as JSON
    /// [`HostConfigSaved`], exactly like `SetHostConfig`.
    ApplyHostConfig {
        toml: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expect_sha256: Option<String>,
    },
    /// feat-platform-10 (milestone follow): one step of driving the
    /// dashboard's open tabs (`homelab ui <step>`). The host hands it to the
    /// session that sent `UiAttach` and answers with that session's
    /// `UiReply`: JSON `{ok, state, refusal?}` in the reply's message.
    /// Refused at once when no dashboard is attached.
    Ui {
        step: UiStep,
    },
    /// feat-platform-10: this session is the dashboard; UI steps come here.
    /// The newest attach wins; the session's end detaches it.
    UiAttach,
    /// feat-platform-10: the dashboard's answer to the UI step relayed as
    /// `ServerMsg::Ui { relay, .. }`. Taken only from the attached session.
    UiReply {
        relay: u64,
        ok: bool,
        message: String,
    },
    /// Live view (decided 2026-09-29): the dashboard holds relay `relay`
    /// longer than the host's usual wait (the viewer paused it): the host
    /// waits `wait_s` more seconds from now, at most [`UI_HOLD_MAX_S`], and
    /// hands `note` to the waiting CLI as `ServerMsg::UiNote`. Taken only
    /// from the attached session, like `UiReply`.
    UiHold {
        relay: u64,
        wait_s: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        note: Option<String>,
    },
    /// feat-backup-1: per-repository backup status of one stack (a compose
    /// stack has one per owning app, D25; a native stack has exactly one,
    /// named after the unit) — last snapshot, its age and size, and the last
    /// restore-drill verdict recorded for that repository. Read-only.
    GetBackups {
        stack: String,
    },
    /// feat-backup-3: list the files one snapshot holds under `path` ("" =
    /// the snapshot root), read-only. `owner` is a repository name from
    /// `GetBackups` (the owning app, or the native unit).
    BrowseSnapshot {
        owner: String,
        snapshot: String,
        #[serde(default)]
        path: String,
    },
    /// feat-backup-2: restore a native (adopted) service from a snapshot
    /// (default "latest"). Unpacks the archive `restic dump` emits straight
    /// into the container with `tar`, the same pipeline the auto-restore of
    /// an empty unit already uses (`native::restore_empty_unit`) — see
    /// `native::restore_native` for the gated, chosen-snapshot version.
    RestoreNative {
        stack: String,
        snapshot: String,
        /// fix-64-equivalent: the stack name as typed; refused without it,
        /// same rule as `RestoreStack::confirm`.
        #[serde(default)]
        confirm: Option<String>,
    },
    /// feat-secrets-1: the content of one secret, read from the host's own
    /// vault (the value a deploy last sealed there — `ops::deploy`'s
    /// `{state_dir}/secrets/...`), never from a process the transcript or
    /// journal could echo. Every call is audit-logged on the host (the
    /// request, never the value). `confirm` carries the stack name typed by
    /// the viewer requesting the reveal, so an empty confirm (an old client)
    /// is refused rather than silently handed the value.
    RevealSecret {
        stack: String,
        secret: SecretRef,
    },
    /// feat-secrets-2: change ONE secret, writing through latch exactly as
    /// `homelab deploy` reads one (`latch put <stack>/<app>/.env --env …` or
    /// `latch put <stack>/<from> --env …`), from the host's own intent repo
    /// checkout. The sibling files in latch are untouched — only this one
    /// relative path is written. `content` is never logged: the host side
    /// hands it to `latch put` on stdin and the line it runs is traced with
    /// the secret value itself stripped, not merely masked.
    SetSecret {
        stack: String,
        secret: SecretRef,
        content: String,
    },
}

// feat-secrets-1/2: `SecretRef` is a domain type (core has no I/O of its
// own but every other wire type that is also domain-shaped lives there —
// see the `pub use homelab_core::...` block above), defined in
// `homelab_core::ops::secrets` and re-exported here for the wire.
pub use homelab_core::ops::secrets::SecretRef;

/// Live view: the longest one `UiHold` may ask the host to wait. The
/// dashboard's own longest pause (`HOMELAB_ADMIN_LIVE_MAX_PAUSE_S`, at most
/// an hour) fits inside it with room for the step itself.
pub const UI_HOLD_MAX_S: u64 = 3_900;

/// feat-platform-10: how long the host waits for the dashboard's answer to
/// a UI step it does not hold (`UiHold`).
pub const UI_RELAY_WAIT_S: u64 = 20;

/// feat-platform-10 (milestone follow): one step of Claude driving the
/// dashboard, as `homelab ui <step>` sends it. The dashboard validates it
/// against the form descriptions it draws its dialogs from, applies it to
/// its one shared "Claude is driving" state and pushes it to every tab that
/// follows. The final press runs on the dashboard's server, once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum UiStep {
    /// Show a page: a path under /app/, e.g. `/app/stacks/media`.
    Goto { path: String },
    /// Open an action's dialog: `form` is the action (`deploy`), `target`
    /// the stack; None for a host-wide action (`patch`).
    Open {
        form: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// Type text into a text field of the open form, by the field's id.
    Type { field: String, text: String },
    /// Choose a value of a choice field.
    Pick { field: String, value: String },
    /// Tick (`on`) or untick a check field.
    Check { field: String, on: bool },
    /// Press a button of the open form: `next`, `back` or `confirm`; in
    /// an edit form's dialog also `save`, `cancel` or `default`.
    Press { button: String },
    /// Set the whole text of a multi-line field (the raw editor's file, a
    /// commit note), at once.
    Edit { field: String, text: String },
    /// Act on one row of the open edit form's table: `add`, and `edit`,
    /// `up`, `down` or `delete` a firewall rule by its number (from 1), or
    /// `edit` a host.toml key by its name.
    Row {
        op: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// Owner decision 2026-09-30: tick rows of the Overview fleet table's
    /// multiselect, exactly as a click would, so a batch action can be
    /// opened from that selection (`homelab ui select <stack>,<stack>`,
    /// `homelab ui select none` clears it). Only on the Overview page.
    Select { stacks: Vec<String> },
    /// Close the open dialog.
    Close,
    /// Change nothing; answer what is on screen now.
    State,
    /// Stop driving: the tabs are the viewer's again.
    Done,
    /// Live view: the whole sequence up front (`homelab ui plan`). Changes
    /// nothing on screen; the tabs that follow list it beside the page and
    /// mark each step as it is taken. A later step that is not the next one
    /// of the plan is still taken, and marks the plan "changed".
    Plan { steps: Vec<UiStep> },
}

impl UiStep {
    /// Reading what is on screen needs only `Read`; every other step can
    /// end in a press, which the dashboard checks again against the
    /// action's own scope.
    pub fn scope(&self) -> Scope {
        match self {
            UiStep::State => Scope::Read,
            _ => Scope::Operate,
        }
    }

    /// The verb as `homelab ui` spells it.
    pub fn verb(&self) -> &'static str {
        match self {
            UiStep::Goto { .. } => "goto",
            UiStep::Open { .. } => "open",
            UiStep::Type { .. } => "type",
            UiStep::Pick { .. } => "pick",
            UiStep::Check { .. } => "check",
            UiStep::Press { .. } => "press",
            UiStep::Edit { .. } => "edit",
            UiStep::Row { .. } => "row",
            UiStep::Select { .. } => "select",
            UiStep::Close => "close",
            UiStep::State => "state",
            UiStep::Done => "done",
            UiStep::Plan { .. } => "plan",
        }
    }
}

fn history_limit_default() -> usize {
    2000
}

fn notices_limit_default() -> usize {
    200
}

/// feat-platform-3: a step starting or ending, as structured data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepMark {
    pub op: String,
    pub step: String,
    /// false = started, true = finished.
    pub finished: bool,
    /// On a finished step: whether it changed anything.
    #[serde(default)]
    pub changed: bool,
}

/// feat-platform-3: what `CurrentOp` answers (JSON in `RpcResponse.message`):
/// what holds the operation lock, and the newest lines the host kept, so a
/// client that connects mid-operation can catch up.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CurrentOpView {
    /// What holds the lock, e.g. "deploy media"; None when idle.
    pub holder: Option<String>,
    pub started_unix: Option<u64>,
    /// Oldest first; only `Log` lines.
    pub lines: Vec<ServerMsg>,
}

/// arch-tokens (homelab-admin, 2026-09-28): what a token may do. Ordered:
/// a token may run every command whose scope is at or below its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Look: fleet, findings, doctor, incidents, settings.
    Read,
    /// Act without destroying: deploy, back up, restore, update, answer.
    Operate,
    /// Everything, including destroy, wipe, exec, host settings and a new
    /// host binary.
    All,
}

impl Command {
    /// The one table of which command needs which scope. The match has no
    /// wildcard on purpose: a new command does not compile until it is
    /// given a row here.
    pub fn scope(&self) -> Scope {
        use Command::*;
        match self {
            Ping
            | Status
            | Doctor { .. }
            | Incidents { .. }
            | IncidentShow { .. }
            | GetState
            | ListTemplates
            | GetApplied { .. }
            | GetConfig
            | FleetCheck { .. }
            | Today { .. }
            | ListManualChecks { .. }
            | Tiles { .. }
            | BackupCalendar { .. }
            | SessionOptions { .. }
            | CurrentOp
            | History { .. }
            | Notices { .. }
            | GetHostConfig
            | TokenList
            | GetBackups { .. }
            | BrowseSnapshot { .. } => Scope::Read,
            Ui { step } => step.scope(),
            DeployStack(_)
            | StageNativeBinary { .. }
            | BackupStack(_)
            | RestoreStack { .. }
            | UpdateStack { .. }
            | PatchFleet
            | BuildTemplate { .. }
            | ApplyResources(_)
            | BackupHostMeta
            | AdoptService(_)
            | InstallNative { .. }
            | InstallNativeRelease { .. }
            | BackupNative { .. }
            | UpdateNative { .. }
            | ReleaseUpdateNative { .. }
            | RollbackNative { .. }
            | Answer { .. }
            | ZfsReplicate
            | ApplyGuards { .. }
            | SetStackEnabled { .. }
            | BackupDevices
            | AnswerManualCheck { .. }
            | UiAttach
            | UiReply { .. }
            | UiHold { .. }
            | RestoreNative { .. }
            | RevealSecret { .. } => Scope::Operate,
            DestroyStack { .. }
            | SelfUpdateHost { .. }
            | RestartHost
            | ExecIn { .. }
            | SetConfig(_)
            | ForgetStack { .. }
            | DestroyRecorded { .. }
            | WipeRetired { .. }
            | PruneOrphans { .. }
            | SetHostConfig { .. }
            | TokenIssue { .. }
            | TokenRevoke { .. }
            | ApplyHostConfig { .. }
            | SetSecret { .. } => Scope::All,
        }
    }

    /// The command's wire name (`deploy_stack`), for audit lines and logs
    /// that must name the command without carrying its payload.
    pub fn name(&self) -> &'static str {
        use Command::*;
        match self {
            Ping => "ping",
            Status => "status",
            DeployStack(_) => "deploy_stack",
            StageNativeBinary { .. } => "stage_native_binary",
            Doctor { .. } => "doctor",
            Incidents { .. } => "incidents",
            IncidentShow { .. } => "incident_show",
            GetState => "get_state",
            DestroyStack { .. } => "destroy_stack",
            BackupStack(_) => "backup_stack",
            RestoreStack { .. } => "restore_stack",
            UpdateStack { .. } => "update_stack",
            SelfUpdateHost { .. } => "self_update_host",
            RestartHost => "restart_host",
            PatchFleet => "patch_fleet",
            ExecIn { .. } => "exec_in",
            BuildTemplate { .. } => "build_template",
            ApplyResources(_) => "apply_resources",
            ListTemplates => "list_templates",
            GetApplied { .. } => "get_applied",
            GetConfig => "get_config",
            SetConfig(_) => "set_config",
            BackupHostMeta => "backup_host_meta",
            AdoptService(_) => "adopt_service",
            InstallNative { .. } => "install_native",
            InstallNativeRelease { .. } => "install_native_release",
            BackupNative { .. } => "backup_native",
            UpdateNative { .. } => "update_native",
            ReleaseUpdateNative { .. } => "release_update_native",
            RollbackNative { .. } => "rollback_native",
            Answer { .. } => "answer",
            ZfsReplicate => "zfs_replicate",
            ApplyGuards { .. } => "apply_guards",
            ForgetStack { .. } => "forget_stack",
            DestroyRecorded { .. } => "destroy_recorded",
            WipeRetired { .. } => "wipe_retired",
            PruneOrphans { .. } => "prune_orphans",
            FleetCheck { .. } => "fleet_check",
            Today { .. } => "today",
            SetStackEnabled { .. } => "set_stack_enabled",
            BackupDevices => "backup_devices",
            ListManualChecks { .. } => "list_manual_checks",
            Tiles { .. } => "tiles",
            BackupCalendar { .. } => "backup_calendar",
            AnswerManualCheck { .. } => "answer_manual_check",
            SessionOptions { .. } => "session_options",
            CurrentOp => "current_op",
            History { .. } => "history",
            Notices { .. } => "notices",
            GetHostConfig => "get_host_config",
            SetHostConfig { .. } => "set_host_config",
            TokenIssue { .. } => "token_issue",
            TokenList => "token_list",
            TokenRevoke { .. } => "token_revoke",
            ApplyHostConfig { .. } => "apply_host_config",
            Ui { .. } => "ui",
            UiAttach => "ui_attach",
            UiReply { .. } => "ui_reply",
            UiHold { .. } => "ui_hold",
            GetBackups { .. } => "get_backups",
            BrowseSnapshot { .. } => "browse_snapshot",
            RestoreNative { .. } => "restore_native",
            RevealSecret { .. } => "reveal_secret",
            SetSecret { .. } => "set_secret",
        }
    }

    /// Read-only commands change nothing, so a session that matches replies
    /// by id may run them beside the queue.
    pub fn is_read_only(&self) -> bool {
        self.scope() == Scope::Read
    }
}

/// G8: the host settings the TUI may inspect and edit. Token/listen/state_dir
/// are deliberately NOT here — those change over ssh only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfigView {
    /// Hour (0-23, host local) for the nightly backup+update run; None = off.
    pub backup_hour: Option<u8>,
    /// Webhook POSTed once per completed operation; None = off.
    pub notify_webhook: Option<String>,
    /// Tiered snapshot retention.
    pub retention: Vec<RetentionTier>,
    /// fix-122 (AR15's runtime debug toggle, Kenny's go 2026-10-01): a
    /// `tracing`/`EnvFilter` directive (e.g. "info", "debug",
    /// "homelab_host=debug,info"), applied live to the journal and the
    /// JSONL ring — no restart.
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

/// feat-settings-1: host.toml as `GetHostConfig` answers it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HostConfigFile {
    /// Where the host read it, e.g. `/etc/homelab/host.toml`.
    pub path: String,
    /// SHA-256 of the file's bytes ("" hashed when there is no file); sent
    /// back with `SetHostConfig` so a change made meanwhile is refused.
    pub sha256: String,
    /// Every key the file sets, as JSON, secrets left out and scoped tokens
    /// without their hashes (`homelab_core::hostconfig::redact`).
    pub values: std::collections::BTreeMap<String, serde_json::Value>,
    /// The secret keys the file sets.
    #[serde(default)]
    pub secrets_set: Vec<String>,
    /// Dotted paths the file sets that the host does not read (F186).
    #[serde(default)]
    pub unknown: Vec<String>,
}

/// feat-settings-1: what `SetHostConfig` answers when it wrote the file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostConfigSaved {
    /// The file's new SHA-256.
    pub sha256: String,
    /// Changed keys already in force.
    pub live: Vec<String>,
    /// Changed keys that take effect at the host's next start.
    pub restart: Vec<String>,
}

/// fix-120: what `TokenList` answers for one `[[tokens]]` entry (or the
/// legacy single `token`, named `"legacy"`) — never a hash, never a
/// plaintext token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenView {
    pub name: String,
    pub scope: Scope,
}

/// fix-120: what `TokenIssue` answers. `token` is the plaintext bearer the
/// new machine must save (e.g. into `HOMELAB_TOKEN`) — this is the only
/// place it is ever sent; the host keeps only its SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenIssued {
    pub name: String,
    pub scope: Scope,
    pub token: String,
}

/// A stack as the TUI sees it — structured, not free text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackView {
    pub name: String,
    pub vmid: u16,
    pub hostname: String,
    pub apps: Vec<AppView>,
    /// intent hash differs from applied → true (B4). Computed client-side
    /// by comparing `applied_hash` with the local stack directory's hash.
    pub drift: bool,
    /// B4: fingerprint the host recorded at the last successful deploy.
    #[serde(default)]
    pub applied_hash: String,
    pub env_sealed: bool,
    pub online: bool,
    /// H8 (light): false = parked — nightly scheduler skips it, onboot off.
    #[serde(default = "enabled_default")]
    pub enabled: bool,
    /// feat-platform-2: what Proxmox measured for this guest at the host's
    /// last status reading. None before the first reading.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<GuestUsage>,
    /// arch-deploy-guard: where the last deploy came from, as the host
    /// recorded it ("a1b2c3d4e5f6 + 1 uncommitted file(s)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_source: Option<String>,
}

/// feat-platform-2: one guest's measured use.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct GuestUsage {
    /// Share of one core ×1000.
    pub cpu_permille: u32,
    pub ram_used_mb: u32,
    pub ram_max_mb: u32,
    pub uptime_s: u64,
}

fn enabled_default() -> bool {
    true
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppView {
    pub name: String,
    pub running: bool,
    pub restarts: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostView {
    pub name: String,
    /// fix-175: the host's own busy share of all CPUs, measured between two
    /// status-poll samples. `None` before the first poll has a predecessor
    /// to diff against, or when the pair could not be trusted — never a
    /// fabricated 0 (it used to be a hard-coded 0, same mistake ram_pct had
    /// before feat-platform-2). `#[serde(default)]` so an older host that
    /// still sends a bare number keeps working.
    #[serde(default)]
    pub cpu_pct: Option<u64>,
    pub ram_pct: u64,
    pub disk_pct: u64,
    pub tls_fingerprint: String,
    /// C6 capacity. For LXC the honest constraint is ACTUAL usage vs physical
    /// total — committed limits routinely exceed 100% (overcommit is normal),
    /// so committed is shown only as context, not as the primary gauge.
    #[serde(default)]
    pub ram_total_mb: u32,
    /// Real RAM in use across the host (sum of actual, not limits).
    #[serde(default)]
    pub ram_used_mb: u32,
    /// Sum of per-stack RAM ceilings (informational; may exceed total).
    #[serde(default)]
    pub ram_committed_mb: u32,
    #[serde(default)]
    pub cores_total: u16,
    /// 1-minute load average ×100 (so 250 = 2.50), avoids f64 on the wire.
    #[serde(default)]
    pub load1_x100: u32,
    /// arch-exposure: the house's public address, as the host last read it
    /// from the router (fix-94). The dashboard answers only from there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_address: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetState {
    pub host: HostView,
    pub stacks: Vec<StackView>,
    /// feat-platform-2: unix seconds of the status reading `online`,
    /// `running` and `restarts` come from. None = no reading yet, and those
    /// fields are the old fixed values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_measured_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub id: u64,
    #[serde(flatten)]
    pub command: Command,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub id: u64,
    pub ok: bool,
    pub message: String,
    /// Set when the operation deliberately did not run: `ok` is false because
    /// nothing happened, and this says it was a decision rather than a fault.
    /// A caller that treats every `!ok` as broken would park a media stack
    /// for the crime of being watched (F280). `serde(default)` keeps an older
    /// client able to read a newer host.
    #[serde(default)]
    pub deferred: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl From<homelab_core::sink::Level> for LogLevel {
    fn from(l: homelab_core::sink::Level) -> Self {
        use homelab_core::sink::Level as L;
        match l {
            L::Debug => LogLevel::Debug,
            L::Info => LogLevel::Info,
            L::Warn => LogLevel::Warn,
            L::Error => LogLevel::Error,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMsg {
    Hello {
        version: String,
        proto: u32,
        /// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci):
        /// `git describe --dirty` of the tree the host was built from, so a
        /// hand-built binary no longer passes for the release of the same
        /// version. None from a host older than this field.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        build: Option<String>,
    },
    Log {
        level: LogLevel,
        source: String,
        msg: String,
        /// feat-platform-3 (homelab-admin, 2026-09-28): the request whose
        /// operation printed this line; None for the nightly round and other
        /// work nobody asked for over the line.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        req: Option<u64>,
        /// Unix seconds the host printed it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ts: Option<u64>,
        /// Set on a step's start and end, so a client can count steps
        /// without parsing `msg` (feat-ops-6, "step 3/35").
        #[serde(default, skip_serializing_if = "Option::is_none")]
        step: Option<StepMark>,
        /// milestone act (homelab-admin, 2026-09-28): the name of the token
        /// whose session asked for the operation that printed this line
        /// ("admin", "wsl", "legacy"); None for the host's own work. `req`
        /// alone is per session, so two sessions' request 5 look alike; this
        /// tells them apart, and milestone `follow` shows who is acting.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        by: Option<String>,
    },
    /// T69: an operation has stopped and is waiting for a person.
    ///
    /// The host sends this and suspends that step. It carries what happened
    /// AND what each answer sets in motion, because a bare allow/stop pair
    /// is vocabulary rather than an answer to "what happens if I press
    /// this" — the same reasoning as the consequences box on Kenny's forms.
    Ask {
        id: u64,
        op: String,
        step: String,
        what: String,
        if_allowed: String,
        if_stopped: String,
        /// arch-host-link (homelab-admin, 2026-09-28): which start of the
        /// host asked. Question ids restart at 1 on every start, so an
        /// answer carries this back and a stale one is refused.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        boot: Option<String>,
    },
    /// Real byte counters for transfer visuals (G6).
    Transfer {
        op: String,
        label: String,
        done: u64,
        total: Option<u64>,
    },
    /// Structured fleet snapshot (reply to GetState).
    State(Box<FleetState>),
    /// feat-platform-10: a UI step for the attached dashboard session only
    /// (never broadcast). `by` is the driving token's name, `scope` its
    /// scope, so the dashboard refuses a press the driver may not make
    /// itself. The dashboard answers with `Command::UiReply { relay, .. }`.
    Ui {
        relay: u64,
        by: String,
        scope: Scope,
        step: UiStep,
    },
    /// Live view: a word for the CLI whose UI step the dashboard holds
    /// ("paused by the viewer …"), sent to that session alone.
    UiNote {
        note: String,
    },
    /// G8: host settings (reply to GetConfig).
    Config(Box<HostConfigView>),
    RpcDone(RpcResponse),
}

#[cfg(test)]
mod wire_tests {
    use super::*;

    /// Every command must survive the wire, and the wire is an RpcRequest —
    /// not the bare enum.
    ///
    /// `RpcRequest` flattens the command into the same JSON object as its own
    /// `id`, so a command carrying a field called `id` collides with it: one
    /// key, two meanings, and `serde_json::from_str::<RpcRequest>` fails. The
    /// host's read loop skips a request it cannot parse and says nothing, so
    /// the client waits for a reply that will never come — which is exactly
    /// what `homelab checks answer <id> ok` did on 2026-09-02.
    #[test]
    fn no_command_field_collides_with_the_envelope() {
        let cases: Vec<Command> = vec![
            Command::AnswerManualCheck {
                check_id: "c4bca102".into(),
                ok: true,
                note: String::new(),
                accept_days: None,
            },
            Command::ListManualChecks { json: false },
            Command::SetStackEnabled {
                stack: "home".into(),
                enabled: true,
            },
        ];
        for c in cases {
            let req = RpcRequest {
                id: 42,
                command: c.clone(),
            };
            let json = serde_json::to_string(&req).expect("serialise");
            let back: RpcRequest = serde_json::from_str(&json).unwrap_or_else(|e| {
                panic!(
                    "{:?} does not survive the envelope: {}\n  wire: {}",
                    c, e, json
                )
            });
            assert_eq!(back.id, 42, "the envelope's id must not be overwritten");
        }
    }

    /// AR5 as amended 2026-09-27 (architecture-decisions-not-built): frames
    /// are bare JSON objects, `kind`-tagged from the host and `cmd`-tagged
    /// with an `id` from the client. There is no `{v, topic, id, payload}`
    /// envelope; the protocol version rides in `Hello`.
    #[test]
    fn frames_are_bare_json_and_the_version_rides_in_hello() {
        let hello = serde_json::to_value(ServerMsg::Hello {
            build: None,
            version: "3.60.0".into(),
            proto: PROTO_VERSION,
        })
        .unwrap();
        assert_eq!(hello["kind"], "hello");
        assert_eq!(hello["proto"], PROTO_VERSION);
        let req = serde_json::to_value(RpcRequest {
            id: 7,
            command: Command::Ping,
        })
        .unwrap();
        assert_eq!(req["cmd"], "ping");
        assert_eq!(req["id"], 7);
        for frame in [&hello, &req] {
            for key in ["v", "topic", "payload"] {
                assert!(frame.get(key).is_none(), "{key} in {frame}");
            }
        }
    }

    /// feat-platform-10: a UI step survives the envelope, reading is `Read`,
    /// everything else `Operate`, and the relayed frame names its driver.
    #[test]
    fn follow_ui_steps_travel_in_the_envelope_with_their_scope() {
        let steps = vec![
            UiStep::Goto {
                path: "/app/stacks/media".into(),
            },
            UiStep::Open {
                form: "deploy".into(),
                target: Some("media".into()),
            },
            UiStep::Type {
                field: "act-snapshot".into(),
                text: "latest".into(),
            },
            UiStep::Pick {
                field: "act-app".into(),
                value: "jellyfin".into(),
            },
            UiStep::Check {
                field: "act-force".into(),
                on: true,
            },
            UiStep::Press {
                button: "confirm".into(),
            },
            UiStep::Edit {
                field: "raw-text".into(),
                text: "a: 1\nb: 2\n".into(),
            },
            UiStep::Row {
                op: "up".into(),
                target: Some("2".into()),
            },
            UiStep::Row {
                op: "add".into(),
                target: None,
            },
            UiStep::Select {
                stacks: vec!["media".into(), "uptime".into()],
            },
            UiStep::Select { stacks: vec![] },
            UiStep::Close,
            UiStep::State,
            UiStep::Done,
        ];
        for step in steps {
            let req = RpcRequest {
                id: 9,
                command: Command::Ui { step: step.clone() },
            };
            let json = serde_json::to_string(&req).unwrap();
            let back: RpcRequest = serde_json::from_str(&json).unwrap();
            assert_eq!(back.id, 9);
            let Command::Ui { step: got } = back.command else {
                panic!("{json}")
            };
            assert_eq!(got, step);
            let want = if step == UiStep::State {
                Scope::Read
            } else {
                Scope::Operate
            };
            assert_eq!(Command::Ui { step }.scope(), want);
        }
        assert_eq!(Command::UiAttach.scope(), Scope::Operate);
        let reply = Command::UiReply {
            relay: 3,
            ok: true,
            message: "{}".into(),
        };
        assert_eq!(reply.scope(), Scope::Operate);
        assert_eq!(reply.name(), "ui_reply");
        let frame = serde_json::to_value(ServerMsg::Ui {
            relay: 3,
            by: "wsl".into(),
            scope: Scope::Operate,
            step: UiStep::Close,
        })
        .unwrap();
        assert_eq!(frame["kind"], "ui");
        assert_eq!(frame["step"]["do"], "close");
        assert_eq!(frame["by"], "wsl");
        // Live view: the plan carries its steps, the hold its wait.
        let plan = UiStep::Plan {
            steps: vec![UiStep::Close, UiStep::Done],
        };
        let v = serde_json::to_value(&plan).unwrap();
        assert_eq!(v["do"], "plan");
        assert_eq!(v["steps"][1]["do"], "done");
        assert_eq!(serde_json::from_value::<UiStep>(v).unwrap(), plan);
        assert_eq!(plan.verb(), "plan");
        let hold = Command::UiHold {
            relay: 3,
            wait_s: 60,
            note: Some("paused by the viewer kenny".into()),
        };
        assert_eq!((hold.scope(), hold.name()), (Scope::Operate, "ui_hold"));
        let note = serde_json::to_value(ServerMsg::UiNote { note: "x".into() }).unwrap();
        assert_eq!(note["kind"], "ui_note");
    }
}
