//! The queue's shared state ([`Inner`]), its limits and [`Actions::start`].

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Duration;

use tokio::sync::mpsc;

use super::{
    Clock, HostPort, ImageCommitter, Job, JobView, PauseGate, Publish, SettledFn, StackFiles,
};
use crate::core::actions_progress;
use crate::shell::actions_notify::NotifyCenter;
use crate::shell::host_link::Shared;

/// Jobs kept for `GET /data/actions/jobs` and reloads, unless
/// [`Actions::set_limits`] says otherwise (`HOMELAB_ADMIN_KEEP_JOBS`).
pub(super) const KEEP_JOBS: usize = 200;
/// feat-logs-1: the most `action_log` lines kept per job for
/// `GET /data/actions/jobs/{job}/log` (the newest win). The page showed 1000
/// (web `LOG_LIMIT`); a deploy writes some hundreds, so 20000 is the whole
/// log of any real job while one runaway job cannot grow memory without
/// bound (200 kept jobs at most).
pub const JOB_LOG_MAX: usize = 20_000;
/// History read for the expected durations, unless [`Actions::set_limits`]
/// says otherwise (`HOMELAB_ADMIN_HISTORY_WINDOW_S`).
pub(super) const HISTORY_WINDOW_S: i64 = 180 * 86_400;

pub(super) struct Inner {
    pub(super) host: Arc<dyn HostPort>,
    pub(super) publish: Arc<dyn Publish>,
    pub(super) files: Arc<dyn StackFiles>,
    pub(super) shared: Shared,
    pub(super) notify: Arc<NotifyCenter>,
    pub(super) clock: Clock,
    pub(super) timeout: Duration,
    pub(super) queue: mpsc::UnboundedSender<Job>,
    pub(super) jobs: std::sync::Mutex<VecDeque<JobView>>,
    pub(super) batches: std::sync::Mutex<BTreeMap<u64, Vec<u64>>>,
    /// one-process-batch-1: each batch's step plan (every member's steps),
    /// made before its first job starts; and the gate that makes it once.
    pub(super) batch_plans: std::sync::Mutex<BTreeMap<u64, Vec<actions_progress::MemberPlan>>>,
    pub(super) batch_plan_gate: tokio::sync::Mutex<()>,
    /// feat-logs-1: the `action_log` lines per kept job.
    pub(super) job_logs: std::sync::Mutex<BTreeMap<u64, VecDeque<serde_json::Value>>>,
    pub(super) next_job: AtomicU64,
    /// TUI parity: where "Update host" and install-native read releases.
    pub(super) releases: std::sync::OnceLock<Arc<dyn crate::shell::releases::Releases>>,
    /// How long "Update host" waits for the host to come back.
    pub(super) reconnect_wait: std::sync::OnceLock<Duration>,
    /// Decision "23 constants": [`KEEP_JOBS`] and [`HISTORY_WINDOW_S`],
    /// overridden from `ActConfig` (mount only).
    pub(super) limits: std::sync::OnceLock<(usize, i64)>,
    /// feat-secrets-2: a new secret value, staged by `POST
    /// /data/secrets/{stack}/stage` and taken exactly once by the
    /// `change-secret` job it is for — never part of `ActionArgs`/`JobView`,
    /// never logged, expired after [`SECRET_STAGE_TTL`] either way.
    pub(super) secret_stage:
        std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, String)>>,
    /// fix-172: the drive session's pause gate, set once the driver exists
    /// (mount wires it after both are built). None for a queue the driver
    /// never reached (most tests): the queue then never waits.
    pub(super) pause_gate: std::sync::OnceLock<Arc<dyn PauseGate>>,
    /// redesign-flows-6: the stack editor's commit, the undo records' file
    /// and the verify's timing (mount and tests set them).
    pub(super) committer: std::sync::OnceLock<Arc<dyn ImageCommitter>>,
    pub(super) flow_store: std::sync::OnceLock<chassis::storage::AppDb>,
    /// jobs-kept-1: tells the writer task which job to write down again
    /// (`true`: with its log). Set by `Actions::set_job_store` (mount).
    pub(super) job_writes: std::sync::OnceLock<mpsc::UnboundedSender<(u64, bool)>>,
    pub(super) flow_timing: std::sync::OnceLock<(Duration, Duration)>,
    /// fix-stale-refresh-1: told the stack of every deploy, update or
    /// restart that finished well (the fleet check forgets its stale rows).
    pub(super) settled: std::sync::OnceLock<SettledFn>,
    /// The running commands a viewer asked to stop, job -> who: `run` turns
    /// the host's `cancelled` reply into "cancelled by <who>".
    pub(super) cancelling: std::sync::Mutex<std::collections::HashMap<u64, String>>,
    /// eten-home-ip-1: the house's address against Cloudflare (mount and
    /// tests set it).
    pub(super) home_address: std::sync::OnceLock<Arc<crate::shell::homeaddress::HomeAddress>>,
}

/// The first host release that knows `Command::CancelExec`. An older host
/// drops the command unanswered, so the dashboard says so instead of
/// waiting for a reply that cannot come (a version check informs; the
/// command itself still runs to its end there).
pub const CANCEL_EXEC_SINCE: &str = "4.0.0";

/// feat-secrets-2: how long a staged secret value waits for the job that
/// asked for it before it is dropped unused.
pub(super) const SECRET_STAGE_TTL: Duration = Duration::from_secs(300);

/// The action queue.
#[derive(Clone)]
pub struct Actions {
    pub(super) inner: Arc<Inner>,
}

pub struct ActionsDeps {
    pub host: Arc<dyn HostPort>,
    pub publish: Arc<dyn Publish>,
    pub files: Arc<dyn StackFiles>,
    pub shared: Shared,
    pub notify: Arc<NotifyCenter>,
    pub clock: Clock,
    /// The longest one command is waited for.
    pub timeout: Duration,
}

impl Actions {
    /// The queue, with its worker running on the current runtime.
    pub fn start(deps: ActionsDeps) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<Job>();
        let inner = Arc::new(Inner::new(deps, tx));
        let worker = Actions {
            inner: inner.clone(),
        };
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                worker.wait_while_paused().await;
                worker.run(job).await;
            }
        });
        Actions { inner }
    }
}

impl Inner {
    fn new(deps: ActionsDeps, queue: mpsc::UnboundedSender<Job>) -> Self {
        // Job ids from the start time, so a restarted dashboard never
        // reuses one a page still shows.
        let first = ((deps.clock)().max(0) as u64) << 8;
        Inner {
            host: deps.host,
            publish: deps.publish,
            files: deps.files,
            shared: deps.shared,
            notify: deps.notify,
            clock: deps.clock,
            timeout: deps.timeout,
            queue,
            jobs: std::sync::Mutex::new(VecDeque::new()),
            batches: std::sync::Mutex::new(BTreeMap::new()),
            batch_plans: std::sync::Mutex::new(BTreeMap::new()),
            batch_plan_gate: tokio::sync::Mutex::new(()),
            job_logs: std::sync::Mutex::new(BTreeMap::new()),
            next_job: AtomicU64::new(first.max(1)),
            releases: std::sync::OnceLock::new(),
            reconnect_wait: std::sync::OnceLock::new(),
            limits: std::sync::OnceLock::new(),
            secret_stage: std::sync::Mutex::new(std::collections::HashMap::new()),
            pause_gate: std::sync::OnceLock::new(),
            committer: std::sync::OnceLock::new(),
            flow_store: std::sync::OnceLock::new(),
            job_writes: std::sync::OnceLock::new(),
            flow_timing: std::sync::OnceLock::new(),
            settled: std::sync::OnceLock::new(),
            cancelling: std::sync::Mutex::new(std::collections::HashMap::new()),
            home_address: std::sync::OnceLock::new(),
        }
    }
}
