//! The shared state of the running host: caches and counters in AppState, the scheduler heartbeat, pending questions and the failed-authentication tracker.

use crate::*;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) config: Config,
    pub(crate) log_tx: broadcast::Sender<ServerMsg>,
    pub(crate) op_lock: Arc<Mutex<()>>, // AR12: mutations strictly serial
    /// fix-104 (long-silences, 2026-09-27): what holds `op_lock`, so a
    /// command that has to wait is told what for, at once.
    pub(crate) busy: Arc<std::sync::Mutex<Option<homelab_core::oplock::Holder>>>,
    /// G8: live mutable settings (scheduler hour, webhook, retention).
    pub(crate) settings: Arc<std::sync::RwLock<homelab_proto::HostConfigView>>,
    /// H13: failure-repeat damping for F3 notifications.
    pub(crate) damper: Arc<std::sync::Mutex<homelab_core::notify::NotifyDamper>>,
    /// T69: questions a step is waiting on, by id. The client's answer
    /// arrives as an ordinary RPC and is delivered through one of these.
    pub(crate) pending_asks: Arc<std::sync::Mutex<std::collections::HashMap<u64, PendingAsk>>>,
    /// Monotonic id for those questions. Not a clock: two questions in the
    /// same second must still be distinguishable.
    pub(crate) next_ask_id: Arc<std::sync::atomic::AtomicU64>,
    /// fix-120: connections refused for their token since this daemon
    /// started, for `homelab doctor`.
    pub(crate) auth_failures: Arc<AuthFailures>,
    /// feat-platform-2: the newest reading of every container's real status.
    pub(crate) live_status:
        Arc<std::sync::RwLock<Option<homelab_core::ops::livestatus::LiveStatus>>>,
    /// redesign-stackhub-1: each stack's "Env sealed" verdict as the status
    /// loop last read it (`ops::facts::sealed_verdicts`, every
    /// `ENV_SEALED_INTERVAL_S`). A stack absent here was not read yet and
    /// goes out as unknown, never as sealed.
    pub(crate) env_sealed: Arc<std::sync::RwLock<std::collections::BTreeMap<String, bool>>>,
    /// fix-175: the host's own busy share of all CPUs, computed in
    /// `status_loop` from the delta between two `/proc/stat` samples taken
    /// `status_interval_s` apart. `None` before the first poll has a
    /// predecessor to compare against, or when the pair cannot be trusted
    /// (see `homelab_core::ops::hostcpu::cpu_pct`) — reported as "unknown",
    /// never as 0.
    pub(crate) host_cpu_pct: Arc<std::sync::RwLock<Option<u64>>>,
    /// fix-175: the `/proc/stat` sample `status_loop` took last time, kept
    /// so the next tick can take the delta.
    pub(crate) host_cpu_prev:
        Arc<std::sync::RwLock<Option<homelab_core::ops::hostcpu::CpuSnapshot>>>,
    /// fix-222: the newest `gather_host_disk_detail` reading, refreshed only
    /// once `DISK_DETAIL_REFRESH_S` has passed since it was taken (it runs
    /// `du` over the whole root filesystem) — never on every status poll.
    pub(crate) disk_detail: Arc<std::sync::RwLock<Option<homelab_proto::HostDiskDetail>>>,
    /// redesign-host-4: whether this running binary is a signed release,
    /// verified once (the binary does not change while it runs).
    pub(crate) release_verdict: Arc<std::sync::OnceLock<homelab_proto::ReleaseVerdict>>,
    /// feat-platform-3: the newest operation lines, for `CurrentOp`.
    pub(crate) recent: Arc<std::sync::Mutex<std::collections::VecDeque<ServerMsg>>>,
    /// arch-host-link: this start of the host, stamped on every question.
    pub(crate) boot_id: String,
    /// fix-121: when this daemon started. A self-update marker armed before
    /// this moment names this binary as the new one; one armed later was
    /// armed by this daemon for its successor.
    pub(crate) started_at: u64,
    /// feat-platform-10: `homelab ui` steps, handed to the dashboard.
    pub(crate) ui: Arc<ui_relay::UiRelay>,
    /// Decision notify-routing: the newest notice's `seq`, read back from
    /// notices.jsonl at start so it only grows.
    pub(crate) notice_seq: Arc<std::sync::Mutex<u64>>,
    /// fix-120: the `[[tokens]]` list in force right now. Starts as
    /// `config.tokens` (what host.toml said at start) and is overwritten in
    /// place by `TokenIssue`/`TokenRevoke`, so a newly issued token works at
    /// once — host.toml marks `tokens` `Apply::Restart` for the generic
    /// settings path, but this dedicated one does not wait for a restart.
    pub(crate) tokens: Arc<std::sync::RwLock<Vec<TokenEntry>>>,
    /// fix-52 residual: unix seconds of the scheduler's last sign of
    /// progress — touched when a tick wakes and after each stack's night
    /// work. The watchdog feeder refuses to send `WATCHDOG=1` once this
    /// goes stale, so a scheduler wedged inside a single await (not a
    /// panic, which `supervise()` already catches) is still noticed.
    pub(crate) scheduler_heartbeat: Arc<std::sync::atomic::AtomicU64>,
    /// fix-122: the runtime debug toggle's live end — reloading this swaps
    /// the `EnvFilter` both the journald sink and the JSONL ring read from,
    /// with no restart. A throwaway handle outside `main()` (every test, and
    /// anything built before `init_production_logging` runs): reloading it
    /// changes nothing, because nothing reads from it.
    pub(crate) log_filter: LogFilterHandle,
    /// fix-180: every restic repository's cached snapshot list, so
    /// `BackupCalendar`/`GetBackups` answer without shelling out to restic
    /// on the request path — see `homelab_core::ops::snapshot_cache`.
    pub(crate) snapshot_cache: Arc<homelab_core::ops::snapshot_cache::SnapshotCache>,
}

/// fix-180: restic reads this cache may run at once, across EVERY caller
/// (a cache-miss RPC, the periodic sweep, a post-backup refresh) — one
/// `Semaphore` inside `SnapshotCache` enforces it globally. rclone's Google
/// Drive remote is rate-limited; the nightly backup phase's own measured
/// concurrency note (`backup::phase_duration_line`) is the same band.
pub(crate) const SNAPSHOT_REFRESH_CONCURRENCY: usize = 3;

/// fix-180: how often the background sweep re-reads every repository the
/// fleet currently declares, so a cache entry is never older than this even
/// when nothing backed up, pruned or restored it in the meantime (and so a
/// repository that existed before this cache did gets its first read without
/// anyone having to open a page). 20 min: inside the UI's own "N min ago"
/// granularity, and nowhere near rclone's rate limit at 2-3 repositories at
/// a time.
pub(crate) const SNAPSHOT_SWEEP_INTERVAL_S: u64 = 20 * 60;

/// fix-122: the subscriber's filter, reloadable from `Rpc::SetHostConfig`
/// and `Rpc::ApplyHostConfig` without a restart.
pub(crate) type LogFilterHandle =
    tracing_subscriber::reload::Handle<tracing_subscriber::EnvFilter, tracing_subscriber::Registry>;

/// fix-122: a handle usable nowhere — not wired to any global subscriber —
/// for `AppState`s that will never have logging live (every test, and
/// `AppState::new` before `main` attaches the real one).
pub(crate) fn inert_log_filter_handle() -> LogFilterHandle {
    tracing_subscriber::reload::Layer::new(tracing_subscriber::EnvFilter::new("info")).1
}

/// fix-52 residual: how long the scheduler may go without a heartbeat touch
/// before the watchdog feeder stops pinging systemd. Idle ticks are 20
/// minutes apart (`SCHEDULER_FIRST_CHECK_S` then `20 * 60`) and a single
/// stack's backup or update can legitimately run long, so this has to clear
/// both with margin — it is not a tight liveness check, it is "a hung
/// nightly round is eventually noticed" rather than "never".
pub(crate) const SCHEDULER_WATCHDOG_STALE_S: u64 = 3600;

/// fix-52 residual: the decision the watchdog-feeder loop makes every tick.
/// Pure so it is unit-tested without a real systemd socket.
pub(crate) fn scheduler_is_alive(now: u64, heartbeat: u64, stale_after_s: u64) -> bool {
    now.saturating_sub(heartbeat) <= stale_after_s
}

impl AppState {
    pub(crate) fn new(config: Config, log_tx: broadcast::Sender<ServerMsg>) -> Self {
        let last_seq = std::fs::read_to_string(notices_path(&config.state_dir))
            .map(|t| {
                homelab_core::notify::parse_notices(&t)
                    .iter()
                    .map(|n| n.seq)
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        AppState {
            started_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            settings: Arc::new(std::sync::RwLock::new(config.initial_settings.clone())),
            tokens: Arc::new(std::sync::RwLock::new(config.tokens.clone())),
            config,
            log_tx,
            op_lock: Arc::new(Mutex::new(())),
            busy: Arc::new(std::sync::Mutex::new(None)),
            pending_asks: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            next_ask_id: Arc::new(std::sync::atomic::AtomicU64::new(1)),
            ui: Arc::new(ui_relay::UiRelay::default()),
            notice_seq: Arc::new(std::sync::Mutex::new(last_seq)),
            damper: Arc::new(std::sync::Mutex::new(
                homelab_core::notify::NotifyDamper::new(20 * 3600),
            )),
            auth_failures: Arc::new(AuthFailures::default()),
            live_status: Arc::new(std::sync::RwLock::new(None)),
            env_sealed: Arc::new(std::sync::RwLock::new(Default::default())),
            host_cpu_pct: Arc::new(std::sync::RwLock::new(None)),
            host_cpu_prev: Arc::new(std::sync::RwLock::new(None)),
            disk_detail: Arc::new(std::sync::RwLock::new(None)),
            release_verdict: Arc::new(std::sync::OnceLock::new()),
            recent: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
            boot_id: format!(
                "{}-{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
                std::process::id()
            ),
            // fix-52 residual: fresh as of construction, so the watchdog
            // feeder does not see a stale scheduler before it has had its
            // first chance to tick.
            scheduler_heartbeat: Arc::new(std::sync::atomic::AtomicU64::new(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            )),
            log_filter: inert_log_filter_handle(),
            snapshot_cache: Arc::new(homelab_core::ops::snapshot_cache::SnapshotCache::new(
                SNAPSHOT_REFRESH_CONCURRENCY,
            )),
        }
    }

    /// fix-122: attaches the real, globally-wired filter handle
    /// `init_production_logging` returned — only `main` calls this; every
    /// test keeps the inert one from `new`, which is exactly as capable as
    /// no handle at all, since no test installs a global subscriber for it
    /// to reach.
    pub(crate) fn with_log_filter(mut self, handle: LogFilterHandle) -> Self {
        self.log_filter = handle;
        self
    }

    /// fix-120: a snapshot of the tokens in force right now, for the
    /// runtime paths that authenticate a connection — never `config.tokens`
    /// directly, which stays frozen at what host.toml said at start.
    pub(crate) fn live_tokens(&self) -> Vec<TokenEntry> {
        self.tokens
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

/// fix-52 residual: stamp the scheduler's heartbeat with the current time.
pub(crate) fn touch_scheduler_heartbeat(state: &AppState) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    state
        .scheduler_heartbeat
        .store(now, std::sync::atomic::Ordering::Relaxed);
}

/// T69: a question waiting for its answer.
pub(crate) struct PendingAsk {
    pub(crate) reply: tokio::sync::oneshot::Sender<bool>,
    /// fix-127: the question as sent, so a client that lagged past it gets
    /// it again.
    pub(crate) ask: ServerMsg,
}

/// fix-120 (expert panel, api-token-is-root, 2026-09-27): the 401 branch
/// logged nothing, so a probe from a compromised container or a stolen token
/// tried from a new machine left no trace. Kept in memory: a flood of bad
/// attempts must not turn into a flood of state writes.
#[derive(Default)]
pub(crate) struct AuthFailures {
    pub(crate) count: std::sync::atomic::AtomicU64,
    pub(crate) last: std::sync::Mutex<Option<(String, u64)>>,
}

impl AuthFailures {
    /// Count one refusal from `peer` and return the running total.
    pub(crate) fn record(&self, peer: &str, now: u64) -> u64 {
        *self.last.lock().unwrap_or_else(PoisonError::into_inner) = Some((peer.to_string(), now));
        self.count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1
    }

    /// What has been seen, in the shape doctor reads.
    pub(crate) fn snapshot(&self) -> homelab_core::doctor::FailedAuth {
        let last = self
            .last
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        homelab_core::doctor::FailedAuth {
            count: self.count.load(std::sync::atomic::Ordering::Relaxed),
            last_peer: last.as_ref().map(|(p, _)| p.clone()),
            last_at: last.map(|(_, t)| t).unwrap_or(0),
        }
    }
}
