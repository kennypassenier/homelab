//! milestone act: the dashboard acts. feat-stacks-4 (every action per
//! stack), feat-stacks-5 (several stacks at once), feat-stacks-6 (roll
//! back), feat-stacks-7 (copy as CLI command) and feat-ops-6 (step progress
//! with the expected duration).
//!
//! A press becomes a job in ONE queue, run one at a time in the order they
//! came, which is the order the host's own queue would run them in anyway
//! (AR12). The route answers at once with the job id; what happens next goes
//! over the live channel:
//!
//! * `action`: the job, whenever its state changes (queued, running, done,
//!   failed, deferred, refused, unknown);
//! * `action_log`: every host line of the job's request;
//! * `action_progress`: "step n/m, expected x" (feat-ops-6);
//! * `action_batch`: a batch's jobs and totals, after each of its jobs.
//!
//! The pieces that touch the world are traits ([`HostPort`], [`Publish`],
//! [`StackFiles`]) so the act tests drive the real queue against a mock
//! host, a recording channel and stack files in memory.

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use axum::extract::rejection::JsonRejection;
use axum::extract::{Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use homelab_core::ops::deployguard::{self, Ancestry};
use homelab_proto::{Command, RpcResponse, ServerMsg};
use serde::Serialize;
use tokio::sync::{broadcast, mpsc, oneshot};

use super::actions_notify::NotifyCenter;
use super::host_link::{HostClient, Shared};
use crate::core::actions::{
    self, ActionArgs, ActionKind, ActionRequest, BatchRequest, Material, Needs, Refusal,
};
use crate::core::actions_cli::cli_line_typed;
use crate::core::actions_progress::{Progress, Tracker};
use crate::core::notify::{Draft, Kind};

type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Unix seconds, injected so tests can hold time still.
pub type Clock = Arc<dyn Fn() -> i64 + Send + Sync>;

pub fn system_clock() -> Clock {
    Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    })
}

/// The live channel, as the actions see it.
pub trait Publish: Send + Sync + 'static {
    fn publish(&self, event: &str, data: serde_json::Value);
}

impl Publish for chassis::shell::live::Live {
    fn publish(&self, event: &str, data: serde_json::Value) {
        let _ = chassis::shell::live::Live::publish(self, event, &data);
    }
}

/// The one line to the host, as the actions see it.
pub trait HostPort: Send + Sync + 'static {
    fn subscribe(&self) -> broadcast::Receiver<ServerMsg>;
    fn ask_traced(
        &self,
        command: Command,
        timeout: Duration,
        sent: Option<oneshot::Sender<u64>>,
    ) -> BoxFut<'_, Result<RpcResponse, String>>;
}

impl HostPort for HostClient {
    fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        HostClient::subscribe(self)
    }
    fn ask_traced(
        &self,
        command: Command,
        timeout: Duration,
        sent: Option<oneshot::Sender<u64>>,
    ) -> BoxFut<'_, Result<RpcResponse, String>> {
        Box::pin(HostClient::ask_traced(self, command, timeout, sent))
    }
}

/// One commit that touched a stack, for the roll-back list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitInfo {
    pub commit: String,
    /// Unix seconds of the commit.
    pub at: i64,
    pub subject: String,
}

/// The stack files, as the actions see them. Every method blocks (files,
/// git, latch); the queue calls them off the async threads.
pub trait StackFiles: Send + Sync + 'static {
    /// What `needs` asks for, from the working copy, or from `commit` when
    /// given (feat-stacks-6). A destroy of a stack whose directory is gone
    /// answers `Material::None` (DestroyRecorded).
    fn read(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: Option<&str>,
    ) -> Result<Material, Refusal>;
    /// Where the host's commit stands against the working copy's HEAD.
    fn ancestry(&self, commit: &str) -> Ancestry;
    /// Newest first.
    fn commits(&self, stack: &str, limit: usize) -> Result<Vec<CommitInfo>, Refusal>;
    /// The native units the stack's files declare.
    fn native_units(&self, stack: &str) -> Vec<String>;
    /// Whether the working copy is there at all.
    fn present(&self) -> bool;
    /// install-native: the unit's manifest, its unit file and the directory
    /// the CLI names (`stacks/kyu/kyu-runner`), from the working copy.
    fn native_release(
        &self,
        stack: &str,
        unit: Option<&str>,
    ) -> Result<(homelab_proto::NativeServiceManifest, String, String), Refusal> {
        let _ = unit;
        Err(Refusal::new(
            format!("install-native {stack}"),
            "these stack files cannot say which native services a stack holds",
            "use the dashboard with its working copy",
        ))
    }
    /// apply and drift: every declared stack of the working copy, with its
    /// intent hash (latch runs for the secrets the hash covers) and, when
    /// `with_specs`, the spec a deploy sends (programs included).
    fn local_stacks(&self, with_specs: bool) -> Result<LocalStacks, Refusal> {
        let _ = with_specs;
        Err(Refusal::new(
            "the stacks directory",
            "these stack files cannot be listed",
            "use the dashboard with its working copy",
        ))
    }
    /// The Deploy review's plan: a stack's files without secrets.
    fn stack_files(&self, stack: &str) -> Result<Vec<homelab_proto::FileBlob>, Refusal> {
        Err(Refusal::new(
            format!("the files of {stack}"),
            "these stack files cannot be read one by one",
            "use the dashboard with its working copy",
        ))
    }
}

/// What the working copy's stacks directory holds, for apply and drift.
#[derive(Debug, Clone, Default)]
pub struct LocalStacks {
    /// Every declared (not ephemeral) stack with its intent hash, or why it
    /// could not be built.
    pub hashes: Vec<(String, Result<String, String>)>,
    /// The name of every directory under stacks/, deployable or not.
    pub dirs: Vec<String>,
    /// Stacks deployed by name only, never by apply.
    pub ephemeral: Vec<String>,
    /// The specs, when asked for.
    pub specs: BTreeMap<String, homelab_proto::DeploySpec>,
}

/// The homelab working copy (arch-state: `…/repo`).
pub struct RepoFiles {
    pub repo: PathBuf,
    /// Where a checkout of an earlier commit is made, and removed again.
    pub scratch: PathBuf,
    /// milestone edit: the working copy the dashboard keeps itself. A read
    /// first takes the remote's newest commits (best effort) and holds the
    /// copy still, so a deploy never reads a stack an edit is writing.
    pub wc: Option<Arc<super::workcopy::WorkingCopy>>,
}

fn no_repo(repo: &Path, what: String) -> Refusal {
    Refusal::new(
        what,
        format!(
            "the dashboard has no working copy of the homelab repository at {}",
            repo.display()
        ),
        "the dashboard clones it there itself once the deploy key is provisioned (the \
         working copy panel says what is missing); actions that need only the stack name work \
         without it",
    )
}

impl RepoFiles {
    fn git(&self, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(&self.repo)
            .env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .args(args)
            .output()
            .map_err(|e| format!("git did not run: {e}"))?;
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }

    fn read_dir(
        &self,
        dir: &Path,
        stack: &str,
        kind: ActionKind,
        what: &str,
    ) -> Result<Material, Refusal> {
        let failed = |why: String| {
            Refusal::new(
                what.to_string(),
                why,
                format!(
                    "fix stacks/{stack} in the working copy; `homelab plan {stack}` says the same"
                ),
            )
        };
        match kind.needs() {
            Needs::Nothing
            | Needs::Vmid
            | Needs::HostRelease
            | Needs::NativeRelease
            | Needs::Apply => Ok(Material::None),
            Needs::Manifest => {
                if kind == ActionKind::Destroy && !dir.join("lxc-compose.yml").exists() {
                    return Ok(Material::None);
                }
                homelab_client::spec::build_manifest(dir)
                    .map(|m| Material::Manifest(Box::new(m)))
                    .map_err(failed)
            }
            Needs::Spec => {
                let spec = homelab_client::spec::build_spec(dir).map_err(failed)?;
                homelab_core::manifest::validate(&spec)
                    .map_err(|e| failed(format!("validation failed: {e}")))?;
                Ok(Material::Spec(Box::new(spec)))
            }
            Needs::NativeManifest => {
                let path = dir.join("service.yml");
                let raw = std::fs::read_to_string(&path)
                    .map_err(|e| failed(format!("cannot read {}: {e}", path.display())))?;
                let m: homelab_proto::NativeServiceManifest = serde_yaml::from_str(&raw)
                    .map_err(|e| failed(format!("service.yml parse: {e}")))?;
                homelab_core::native::validate_native(&m)
                    .map_err(|p| failed(format!("service.yml invalid: {}", p.join("; "))))?;
                Ok(Material::Native(Box::new(m)))
            }
        }
    }
}

impl StackFiles for RepoFiles {
    fn present(&self) -> bool {
        self.repo.join("stacks").is_dir()
    }

    fn read(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: Option<&str>,
    ) -> Result<Material, Refusal> {
        let what = format!("{} {}", kind.slug(), stack);
        if matches!(
            kind.needs(),
            Needs::Nothing | Needs::Vmid | Needs::HostRelease | Needs::NativeRelease | Needs::Apply
        ) {
            return Ok(Material::None);
        }
        if let Some(wc) = &self.wc {
            if commit.is_none() {
                if let Err(r) = wc.sync() {
                    tracing::warn!(why = %r.why, "the working copy was not brought up to date before {what}");
                }
            }
        }
        let _held = self.wc.as_ref().map(|wc| wc.hold());
        if !self.present() {
            return Err(no_repo(&self.repo, what));
        }
        let Some(commit) = commit else {
            return self.read_dir(&self.repo.join("stacks").join(stack), stack, kind, &what);
        };
        // feat-stacks-6: a checkout of that commit beside the working copy,
        // read like the working copy, removed again whatever happened.
        let _ = std::fs::create_dir_all(&self.scratch);
        let tree = self
            .scratch
            .join(format!("rollback-{}-{}", std::process::id(), commit));
        let tree_s = tree.display().to_string();
        let _ = self.git(&["worktree", "remove", "--force", &tree_s]);
        self.git(&["worktree", "add", "--detach", &tree_s, commit])
            .map_err(|why| {
                Refusal::new(
                    what.clone(),
                    format!("commit {commit} could not be checked out: {why}"),
                    "pick a commit from the roll-back list; `git fetch` in the working copy first if it is new",
                )
            })?;
        let dir = tree.join("stacks").join(stack);
        let out = if dir.is_dir() {
            self.read_dir(&dir, stack, kind, &what)
        } else {
            Err(Refusal::new(
                what.clone(),
                format!("stacks/{stack} does not exist at commit {commit}"),
                "pick a commit that has the stack",
            ))
        };
        let _ = self.git(&["worktree", "remove", "--force", &tree_s]);
        out
    }

    fn ancestry(&self, commit: &str) -> Ancestry {
        let object = format!("{commit}^{{commit}}");
        if self.git(&["cat-file", "-e", &object]).is_err() {
            Ancestry::Unknown
        } else if self
            .git(&["merge-base", "--is-ancestor", commit, "HEAD"])
            .is_ok()
        {
            Ancestry::Contained
        } else {
            Ancestry::Diverged
        }
    }

    fn commits(&self, stack: &str, limit: usize) -> Result<Vec<CommitInfo>, Refusal> {
        if !self.present() {
            return Err(no_repo(&self.repo, format!("roll-back list of {stack}")));
        }
        let n = format!("-n{}", limit.clamp(1, 200));
        let path = format!("stacks/{stack}");
        let out = self
            .git(&["log", &n, "--format=%H%x09%ct%x09%s", "--", &path])
            .map_err(|why| {
                Refusal::new(
                    format!("roll-back list of {stack}"),
                    format!("git log failed: {why}"),
                    "check the working copy",
                )
            })?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let mut p = l.splitn(3, '\t');
                Some(CommitInfo {
                    commit: p.next()?.to_string(),
                    at: p.next()?.parse().ok()?,
                    subject: p.next().unwrap_or("").to_string(),
                })
            })
            .collect())
    }

    fn native_units(&self, stack: &str) -> Vec<String> {
        homelab_client::spec::native_services(&self.repo.join("stacks").join(stack))
            .into_iter()
            .map(|(m, _)| m.unit)
            .collect()
    }

    fn native_release(
        &self,
        stack: &str,
        unit: Option<&str>,
    ) -> Result<(homelab_proto::NativeServiceManifest, String, String), Refusal> {
        let what = format!("install-native {stack}");
        if let Some(wc) = &self.wc {
            if let Err(r) = wc.sync() {
                tracing::warn!(why = %r.why, "the working copy was not brought up to date before {what}");
            }
        }
        let _held = self.wc.as_ref().map(|wc| wc.hold());
        if !self.present() {
            return Err(no_repo(&self.repo, what));
        }
        let dir = self.repo.join("stacks").join(stack);
        let services = homelab_client::spec::native_services(&dir);
        let pick = match unit {
            Some(u) => services.into_iter().find(|(m, _)| m.unit == u),
            None if services.len() == 1 => services.into_iter().next(),
            None => {
                return Err(Refusal::new(
                    what,
                    format!(
                        "stacks/{stack} holds {} native services; name the one to install",
                        services.len()
                    ),
                    "pick the unit in the form",
                ))
            }
        };
        let Some((m, unit_file)) = pick else {
            return Err(Refusal::new(
                what,
                format!(
                    "stacks/{stack} has no native service {}",
                    unit.unwrap_or("(none at all)")
                ),
                "pick a unit the stack's service.yml files name",
            ));
        };
        homelab_core::native::validate_native(&m).map_err(|p| {
            Refusal::new(
                what.clone(),
                format!("service.yml invalid: {}", p.join("; ")),
                format!("fix stacks/{stack}"),
            )
        })?;
        if m.release_repo.is_none() {
            return Err(Refusal::new(
                what,
                format!(
                    "{} declares no release_repo: there is no release to install",
                    m.unit
                ),
                "add release_repo to its service.yml",
            ));
        }
        let Some(unit_file) = unit_file else {
            return Err(Refusal::new(
                what,
                format!(
                    "no {}.service beside its service.yml: a rebuilt container would have the \
                     binary and nothing to run it",
                    m.unit
                ),
                "commit the unit file next to the service.yml",
            ));
        };
        // The directory the CLI's install-native names: the one whose
        // service.yml describes this unit.
        let rel = if dir.join(&m.unit).join("service.yml").is_file() {
            format!("stacks/{stack}/{}", m.unit)
        } else {
            format!("stacks/{stack}")
        };
        Ok((m, unit_file, rel))
    }

    fn local_stacks(&self, with_specs: bool) -> Result<LocalStacks, Refusal> {
        if let Some(wc) = &self.wc {
            if let Err(r) = wc.sync() {
                tracing::warn!(why = %r.why, "the working copy was not brought up to date before reading the stacks");
            }
        }
        let _held = self.wc.as_ref().map(|wc| wc.hold());
        if !self.present() {
            return Err(no_repo(&self.repo, "the stacks directory".into()));
        }
        let base = self.repo.join("stacks");
        let declared = homelab_client::spec::declared_stacks(&base);
        let mut out = LocalStacks {
            ephemeral: homelab_client::spec::scan_local_stacks(&base)
                .into_iter()
                .map(|(n, _)| n)
                .filter(|n| !declared.iter().any(|(d, _)| d == n))
                .collect(),
            dirs: std::fs::read_dir(&base)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| e.path().is_dir())
                        .map(|e| e.file_name().to_string_lossy().to_string())
                        .collect()
                })
                .unwrap_or_default(),
            ..LocalStacks::default()
        };
        out.dirs.sort();
        for (name, dir) in declared {
            if with_specs {
                let built = homelab_client::spec::build_spec(&dir).and_then(|sp| {
                    homelab_core::manifest::validate(&sp).map_err(|e| e.to_string())?;
                    Ok(sp)
                });
                match built {
                    Ok(sp) => {
                        out.hashes
                            .push((name.clone(), Ok(homelab_core::manifest::intent_hash(&sp))));
                        out.specs.insert(name, sp);
                    }
                    Err(e) => out.hashes.push((name, Err(e))),
                }
            } else {
                let h = homelab_client::spec::local_intent_hash(&dir).map(|(h, _)| h);
                out.hashes.push((name, h));
            }
        }
        Ok(out)
    }

    fn stack_files(&self, stack: &str) -> Result<Vec<homelab_proto::FileBlob>, Refusal> {
        let _held = self.wc.as_ref().map(|wc| wc.hold());
        if !self.present() {
            return Err(no_repo(&self.repo, format!("the files of {stack}")));
        }
        homelab_client::spec::stack_files(&self.repo.join("stacks").join(stack)).map_err(|why| {
            Refusal::new(
                format!("the files of {stack}"),
                why,
                format!("fix stacks/{stack} in the working copy"),
            )
        })
    }
}

/// Where a job came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum Origin {
    Manual,
    Batch {
        batch: u64,
    },
    Schedule {
        schedule: String,
        slot: i64,
    },
    /// feat-platform-10: the final press of a form Claude drove
    /// (`homelab ui press confirm`), with the driving token's name.
    Claude {
        by: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    /// The host stood aside on purpose; nothing changed.
    Deferred,
    /// The dashboard refused before anything reached the host.
    Refused,
    /// The line dropped before the answer; History tells what happened.
    Unknown,
}

impl JobState {
    pub fn finished(self) -> bool {
        !matches!(self, JobState::Queued | JobState::Running)
    }
}

/// One job as the page shows it (the `action` event, `GET …/jobs`).
#[derive(Debug, Clone, Serialize)]
pub struct JobView {
    pub job: u64,
    pub origin: Origin,
    pub stack: String,
    pub action: ActionKind,
    pub args: ActionArgs,
    pub state: JobState,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    /// The host's request ids, in the order they went out.
    pub reqs: Vec<u64>,
    /// The host's final message, or why the dashboard refused.
    pub message: Option<String>,
    /// feat-stacks-7: the same thing from a workstation.
    pub cli: Option<String>,
    pub progress: Option<Progress>,
    /// arch-self: this job restarts the dashboard; its end is read back
    /// after the restart.
    pub restarts_dashboard: bool,
}

struct Job {
    view: JobView,
}

/// Jobs kept for `GET /data/actions/jobs` and reloads.
const KEEP_JOBS: usize = 200;
/// History read for the expected durations.
const HISTORY_WINDOW_S: i64 = 180 * 86_400;

struct Inner {
    host: Arc<dyn HostPort>,
    publish: Arc<dyn Publish>,
    files: Arc<dyn StackFiles>,
    shared: Shared,
    notify: Arc<NotifyCenter>,
    clock: Clock,
    timeout: Duration,
    queue: mpsc::UnboundedSender<Job>,
    jobs: std::sync::Mutex<VecDeque<JobView>>,
    batches: std::sync::Mutex<BTreeMap<u64, Vec<u64>>>,
    next_job: AtomicU64,
    /// TUI parity: where "Update host" and install-native read releases.
    releases: std::sync::OnceLock<Arc<dyn super::releases::Releases>>,
    /// How long "Update host" waits for the host to come back.
    reconnect_wait: std::sync::OnceLock<Duration>,
}

/// The action queue.
#[derive(Clone)]
pub struct Actions {
    inner: Arc<Inner>,
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
        let first = ((deps.clock)().max(0) as u64) << 8;
        let inner = Arc::new(Inner {
            host: deps.host,
            publish: deps.publish,
            files: deps.files,
            shared: deps.shared,
            notify: deps.notify,
            clock: deps.clock,
            timeout: deps.timeout,
            queue: tx,
            jobs: std::sync::Mutex::new(VecDeque::new()),
            batches: std::sync::Mutex::new(BTreeMap::new()),
            // Job ids from the start time, so a restarted dashboard never
            // reuses one a page still shows.
            next_job: AtomicU64::new(first.max(1)),
            releases: std::sync::OnceLock::new(),
            reconnect_wait: std::sync::OnceLock::new(),
        });
        let worker = Actions {
            inner: inner.clone(),
        };
        tokio::spawn(async move {
            while let Some(job) = rx.recv().await {
                worker.run(job).await;
            }
        });
        Actions { inner }
    }

    fn now(&self) -> i64 {
        (self.inner.clock)()
    }

    /// TUI parity: where releases are read (GitHub in the app, a fake in a
    /// test). Set once.
    pub fn set_releases(&self, r: Arc<dyn super::releases::Releases>) {
        let _ = self.inner.releases.set(r);
    }

    /// How long "Update host" waits for the host to come back (a test
    /// shortens it). Set once; five minutes otherwise.
    pub fn set_reconnect_wait(&self, d: Duration) {
        let _ = self.inner.reconnect_wait.set(d);
    }

    /// A read on the host line, for the lists a form's choices come from
    /// (the manual checks, the templates).
    pub async fn ask(&self, command: Command, timeout: Duration) -> Result<RpcResponse, String> {
        self.inner.host.ask_traced(command, timeout, None).await
    }

    fn releases(&self) -> Result<Arc<dyn super::releases::Releases>, Refusal> {
        self.inner.releases.get().cloned().ok_or_else(|| {
            Refusal::new(
                "the releases",
                "this dashboard reads no releases",
                "report this with the dashboard's log",
            )
        })
    }

    fn store(&self, view: &JobView) {
        if let Ok(mut jobs) = self.inner.jobs.lock() {
            if let Some(slot) = jobs.iter_mut().find(|j| j.job == view.job) {
                *slot = view.clone();
            } else {
                jobs.push_back(view.clone());
                while jobs.len() > KEEP_JOBS {
                    jobs.pop_front();
                }
            }
        }
        self.inner
            .publish
            .publish("action", serde_json::to_value(view).unwrap_or_default());
    }

    /// Every job kept, newest first.
    pub fn jobs(&self) -> Vec<JobView> {
        self.inner
            .jobs
            .lock()
            .map(|j| j.iter().rev().cloned().collect())
            .unwrap_or_default()
    }

    /// Decision notify-routing: the job that sent this request to the host,
    /// so the host's notice of it fills that job's notice. The dashboard's
    /// request ids start past any a CLI or TUI uses (arch-host-link).
    pub fn job_for_req(&self, req: u64) -> Option<u64> {
        self.inner.jobs.lock().ok().and_then(|j| {
            j.iter()
                .rev()
                .find(|v| v.reqs.contains(&req))
                .map(|v| v.job)
        })
    }

    pub fn job(&self, id: u64) -> Option<JobView> {
        self.inner
            .jobs
            .lock()
            .ok()
            .and_then(|j| j.iter().find(|v| v.job == id).cloned())
    }

    /// Checks that need the fleet or the working copy, done at the press
    /// for a quick answer and again when the job runs.
    pub fn precheck(&self, req: &ActionRequest) -> Result<(), Refusal> {
        let what = format!("{} {}", req.action.slug(), req.stack);
        if matches!(
            req.action.needs(),
            Needs::Manifest | Needs::Spec | Needs::NativeManifest
        ) && !self.inner.files.present()
            && req.action != ActionKind::Destroy
        {
            return Err(Refusal::new(
                what,
                "the dashboard has no working copy of the homelab repository",
                "the dashboard clones it at start once the deploy key is provisioned (the \
                 working copy panel on the settings page says what is missing); actions that \
                 need only the stack name work without it",
            ));
        }
        // An older host drops a command it does not know without a word.
        if req.action == ActionKind::InstallNative {
            let v = self
                .inner
                .shared
                .try_read()
                .ok()
                .and_then(|s| s.host_version.clone());
            crate::core::hostversion::at_least(
                &what,
                v.as_deref(),
                crate::core::hostversion::NEXT_RELEASE,
                "update the host first (Update the host on the host page), or install from a \
                 workstation: homelab install-native stacks/<stack> <tag>",
            )?;
        }
        if let Ok(jobs) = self.inner.jobs.lock() {
            if let Some(j) = jobs.iter().find(|j| {
                j.state == JobState::Queued && j.stack == req.stack && j.action == req.action
            }) {
                return Err(Refusal::new(
                    what,
                    format!("the same action is already queued as job {}", j.job),
                    "wait for it; the queue runs one action at a time",
                ));
            }
        }
        Ok(())
    }

    /// A validated request, pressed: the checks a press gets (the queue,
    /// the working copy, the deploy guard), then the job. The route and the
    /// driver (feat-platform-10) both go through here, so a driven press
    /// is refused for exactly what a click is.
    pub async fn press(&self, req: ActionRequest, origin: Origin) -> Result<JobView, Refusal> {
        self.precheck(&req)?;
        let a2 = self.clone();
        let r2 = req.clone();
        if let Ok(Err(r)) = tokio::task::spawn_blocking(move || a2.guard(&r2)).await {
            return Err(r);
        }
        Ok(self.submit(req, origin))
    }

    /// feat-stacks-7 before the press: the CLI line (or why there is
    /// none), the deploy guard's refusal, whether it restarts the dashboard.
    /// `typed`: the form's typed name matched (cli-yes: the line then
    /// carries `--yes`).
    pub async fn preview_of(
        &self,
        req: ActionRequest,
        typed: bool,
    ) -> (Result<String, String>, Option<Refusal>, bool) {
        let a2 = self.clone();
        let restarts = actions::restarts_dashboard(&req.stack, req.action);
        let (line, guard) = tokio::task::spawn_blocking(move || {
            let guard = a2.guard(&req);
            (preview_line(&a2, &req, typed), guard)
        })
        .await
        .unwrap_or((Err("internal".into()), Ok(())));
        (line, guard.err(), restarts)
    }

    /// Queue a validated request; the job as it stands now.
    pub fn submit(&self, req: ActionRequest, origin: Origin) -> JobView {
        let job = self.inner.next_job.fetch_add(1, Ordering::Relaxed);
        let view = JobView {
            job,
            origin,
            restarts_dashboard: actions::restarts_dashboard(&req.stack, req.action),
            stack: req.stack,
            action: req.action,
            args: req.args,
            state: JobState::Queued,
            queued_at: self.now(),
            started_at: None,
            finished_at: None,
            reqs: Vec::new(),
            message: None,
            cli: None,
            progress: None,
        };
        self.store(&view);
        let _ = self.inner.queue.send(Job { view: view.clone() });
        view
    }

    /// feat-stacks-5: a batch as the page sends it, checked whole, then
    /// queued. The route and a driven batch dialog (feat-platform-10) both
    /// come here, so a driven batch is the same batch a click makes.
    pub fn run_batch(
        &self,
        b: actions::BatchRequest,
    ) -> Result<serde_json::Value, (StatusCode, Refusal)> {
        let reqs = actions::validate_batch(b).map_err(|r| (StatusCode::BAD_REQUEST, r))?;
        for r in &reqs {
            self.precheck(r).map_err(|r| (StatusCode::CONFLICT, r))?;
        }
        let (batch, jobs) = self.submit_batch(reqs);
        Ok(serde_json::json!({
            "batch": batch,
            "jobs": jobs.iter().map(|j| serde_json::json!({"job": j.job, "stack": j.stack})).collect::<Vec<_>>(),
        }))
    }

    /// feat-stacks-5: every stack of a validated batch, in the order given.
    pub fn submit_batch(&self, reqs: Vec<ActionRequest>) -> (u64, Vec<JobView>) {
        let batch = self.inner.next_job.fetch_add(1, Ordering::Relaxed);
        let jobs: Vec<JobView> = reqs
            .into_iter()
            .map(|r| self.submit(r, Origin::Batch { batch }))
            .collect();
        if let Ok(mut b) = self.inner.batches.lock() {
            b.insert(batch, jobs.iter().map(|j| j.job).collect());
        }
        self.publish_batch(batch);
        (batch, jobs)
    }

    fn publish_batch(&self, batch: u64) {
        let ids = self
            .inner
            .batches
            .lock()
            .ok()
            .and_then(|b| b.get(&batch).cloned())
            .unwrap_or_default();
        let jobs: Vec<serde_json::Value> = ids
            .iter()
            .filter_map(|id| self.job(*id))
            .map(|j| {
                serde_json::json!({
                    "job": j.job, "stack": j.stack, "state": j.state, "message": j.message,
                })
            })
            .collect();
        let count = |s: JobState| {
            ids.iter()
                .filter_map(|id| self.job(*id))
                .filter(|j| j.state == s)
                .count()
        };
        let done = ids
            .iter()
            .filter_map(|id| self.job(*id))
            .all(|j| j.state.finished());
        self.inner.publish.publish(
            "action_batch",
            serde_json::json!({
                "batch": batch,
                "jobs": jobs,
                "done": done,
                "ok": count(JobState::Done),
                "failed": count(JobState::Failed) + count(JobState::Refused) + count(JobState::Unknown),
                "deferred": count(JobState::Deferred),
            }),
        );
    }

    fn fleet_stack(&self, stack: &str) -> Option<(u16, Option<String>)> {
        let s = self.inner.shared.try_read().ok()?;
        s.fleet
            .as_ref()?
            .stacks
            .iter()
            .find(|x| x.name == stack)
            .map(|x| (x.vmid, x.applied_source.clone()))
    }

    /// arch-deploy-guard, with the working copy's history.
    fn guard(&self, req: &ActionRequest) -> Result<(), Refusal> {
        if !matches!(req.action, ActionKind::Deploy | ActionKind::DeployCommit) {
            return Ok(());
        }
        let applied = self.fleet_stack(&req.stack).and_then(|(_, a)| a);
        let ancestry = match applied.as_deref().and_then(deployguard::applied_commit) {
            None => Ancestry::Contained,
            Some(c) => self.inner.files.ancestry(c),
        };
        deployguard::decide(&req.stack, applied.as_deref(), ancestry, req.args.force).map_err(
            |why| {
                Refusal::new(
                    format!("{} {}", req.action.slug(), req.stack),
                    why,
                    "pull the working copy, or send force: true if undoing that deploy is the point",
                )
            },
        )
    }

    async fn material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let what = format!("{} {}", req.action.slug(), req.stack);
        match req.action.needs() {
            Needs::HostRelease => {
                let releases = self.releases()?;
                let tag = match req
                    .args
                    .tag
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    Some(t) => t.to_string(),
                    None => releases
                        .latest_tag(super::releases::HOMELAB_REPO)
                        .await
                        .map_err(|why| {
                            Refusal::new(what.clone(), why, "name the tag, or try again later")
                        })?,
                };
                let binary_b64 = releases.host_binary(&tag).await.map_err(|why| {
                    Refusal::new(
                        what.clone(),
                        why,
                        "nothing was sent to the host; sign the release, or pick a signed one",
                    )
                })?;
                return Ok(Material::HostRelease { tag, binary_b64 });
            }
            Needs::NativeRelease => {
                let files = self.inner.files.clone();
                let (stack, unit) = (req.stack.clone(), req.args.unit.clone());
                let (m, unit_file, dir) = tokio::task::spawn_blocking(move || {
                    files.native_release(&stack, unit.as_deref())
                })
                .await
                .map_err(|e| Refusal::new(what.clone(), e.to_string(), "report this"))??;
                let tag = match req
                    .args
                    .tag
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    Some(t) => t.to_string(),
                    None => {
                        let repo = m.release_repo.clone().unwrap_or_default();
                        self.releases()?.latest_tag(&repo).await.map_err(|why| {
                            Refusal::new(what.clone(), why, "name the tag, or try again later")
                        })?
                    }
                };
                return Ok(Material::NativeRelease {
                    manifest: Box::new(m),
                    unit_file,
                    tag,
                    dir,
                });
            }
            Needs::Apply => return self.apply_material(req).await,
            _ => {}
        }
        if req.action.needs() == Needs::Vmid {
            return match self.fleet_stack(&req.stack) {
                Some((vmid, _)) => Ok(Material::Vmid(vmid)),
                None => Err(Refusal::new(
                    format!("{} {}", req.action.slug(), req.stack),
                    "the host's fleet has no such stack (or the dashboard has not read it yet)",
                    "wait for the fleet page to show the stack",
                )),
            };
        }
        let files = self.inner.files.clone();
        let (stack, kind, commit) = (req.stack.clone(), req.action, req.args.commit.clone());
        let guard_self = self.clone();
        let req2 = req.clone();
        tokio::task::spawn_blocking(move || {
            guard_self.guard(&req2)?;
            files.read(&stack, kind, commit.as_deref())
        })
        .await
        .map_err(|e| Refusal::new("reading the stack files", e.to_string(), "report this"))?
    }

    /// The host's applied hashes, from the newest fleet reading.
    fn host_pairs(&self) -> Option<Vec<(String, String)>> {
        let s = self.inner.shared.try_read().ok()?;
        Some(
            s.fleet
                .as_ref()?
                .stacks
                .iter()
                .map(|x| (x.name.clone(), x.applied_hash.clone()))
                .collect(),
        )
    }

    /// TUI parity (the plan in the plain Deploy review, fix-100): what a
    /// deploy of `stack` changes, file by file, against the files the host
    /// applied last (`GetApplied`), with each file's diff. Secrets are never
    /// part of it: neither side carries them.
    pub async fn deploy_diff(&self, stack: &str) -> Result<serde_json::Value, Refusal> {
        use crate::core::stackedit::FileChange;
        let files = self.inner.files.clone();
        let s2 = stack.to_string();
        let local = tokio::task::spawn_blocking(move || files.stack_files(&s2))
            .await
            .map_err(|e| Refusal::new("the plan", e.to_string(), "report this"))??;
        let known = self
            .inner
            .shared
            .try_read()
            .ok()
            .and_then(|s| {
                s.fleet.as_ref().map(|f| {
                    f.stacks
                        .iter()
                        .any(|x| x.name == stack && !x.applied_hash.is_empty())
                })
            })
            .unwrap_or(false);
        let applied: Vec<homelab_proto::FileBlob> = if known {
            let r = self
                .inner
                .host
                .ask_traced(
                    Command::GetApplied {
                        stack: stack.to_string(),
                    },
                    Duration::from_secs(30),
                    None,
                )
                .await
                .map_err(|e| Refusal::new("the plan", e, "check that the host answers"))?;
            if !r.ok {
                return Err(Refusal::new(
                    "the plan",
                    r.message,
                    "look at the host's intent repository",
                ));
            }
            serde_json::from_str(&r.message).map_err(|e| {
                Refusal::new(
                    "the plan",
                    format!("the host's applied files do not read: {e}"),
                    "update the host and the dashboard to the same release",
                )
            })?
        } else {
            Vec::new()
        };
        let mut changes: Vec<FileChange> = Vec::new();
        for f in &local {
            match applied.iter().find(|x| x.path == f.path) {
                None => changes.push(FileChange {
                    path: f.path.clone(),
                    old: None,
                    new: Some(f.content.clone()),
                }),
                Some(x) if x.content != f.content || x.mode != f.mode => changes.push(FileChange {
                    path: f.path.clone(),
                    old: Some(x.content.clone()),
                    new: Some(f.content.clone()),
                }),
                Some(_) => {}
            }
        }
        for x in &applied {
            if !local.iter().any(|f| f.path == x.path) {
                changes.push(FileChange {
                    path: x.path.clone(),
                    old: Some(x.content.clone()),
                    new: None,
                });
            }
        }
        changes.sort_by(|a, b| a.path.cmp(&b.path));
        let summary = homelab_client::apply::file_changes(&local, &applied);
        // fix-159: the running native units this deploy restarts, and why.
        let restarts = if known {
            homelab_client::apply::native_restarts(&local, &applied)
        } else {
            Vec::new()
        };
        Ok(serde_json::json!({
            "new_stack": !known,
            "files": crate::core::editplan::file_diffs(&changes),
            "summary": summary,
            "restarts": restarts,
            "note": if changes.is_empty() {
                "the files are as the host applied them; secrets or settings may still differ"
            } else {
                "the files as they change; secrets are not shown (neither side carries them here)"
            },
        }))
    }

    /// dash-apply: the plan against the host, as `homelab apply --plan`
    /// prints it; `with_specs` builds what a deploy sends (for the run).
    pub async fn apply_plan(
        &self,
        with_specs: bool,
    ) -> Result<(crate::core::applyview::ApplyView, LocalStacks), Refusal> {
        let files = self.inner.files.clone();
        let local = tokio::task::spawn_blocking(move || files.local_stacks(with_specs))
            .await
            .map_err(|e| Refusal::new("the plan", e.to_string(), "report this"))??;
        let host = self.host_pairs().ok_or_else(|| {
            Refusal::new(
                "the plan",
                "the dashboard has not read the host's fleet yet",
                "wait for the fleet page to show the stacks",
            )
        })?;
        let view =
            crate::core::applyview::plan(&local.hashes, &local.dirs, &host, &local.ephemeral);
        Ok((view, local))
    }

    async fn apply_material(&self, req: &ActionRequest) -> Result<Material, Refusal> {
        let what = "apply".to_string();
        let (view, mut local) = self.apply_plan(true).await?;
        if let Some((name, why)) = view.broken.first() {
            return Err(Refusal::new(
                what,
                format!("{name} does not build: {why} — nothing applied"),
                format!("fix stacks/{name}; the Apply page lists every stack that does not build"),
            ));
        }
        let typed = crate::core::applyview::chosen_destroys(&view, &req.args.destroy_names())
            .map_err(|why| {
                Refusal::new(
                    what.clone(),
                    why,
                    "type only names the plan lists under 'gone from the files'",
                )
            })?;
        // arch-deploy-guard: every planned stack is checked before the
        // first one is sent, so a refusal leaves nothing half-applied.
        if !req.args.force {
            for name in &view.deploy {
                let r = ActionRequest {
                    stack: name.clone(),
                    action: ActionKind::Deploy,
                    args: ActionArgs::default(),
                };
                self.guard(&r)?;
            }
        }
        let deploy = view
            .deploy
            .iter()
            .filter_map(|n| local.specs.remove(n))
            .collect();
        Ok(Material::Apply {
            deploy,
            destroy: typed,
        })
    }

    async fn history(&self) -> Vec<homelab_core::history::HistoryEntry> {
        let since = (self.now() - HISTORY_WINDOW_S).max(0) as u64;
        let reply = self
            .inner
            .host
            .ask_traced(
                Command::History { since, limit: 5000 },
                Duration::from_secs(30),
                None,
            )
            .await;
        reply
            .ok()
            .filter(|r| r.ok)
            .and_then(|r| serde_json::from_str::<serde_json::Value>(&r.message).ok())
            .and_then(|v| serde_json::from_value(v.get("entries")?.clone()).ok())
            .unwrap_or_default()
    }

    fn finish(&self, view: &mut JobView, state: JobState, message: String) {
        view.state = state;
        view.message = Some(message);
        view.finished_at = Some(self.now());
        self.store(view);
    }

    async fn run(&self, job: Job) {
        let mut view = job.view;
        view.state = JobState::Running;
        view.started_at = Some(self.now());
        self.store(&view);
        let req = ActionRequest {
            stack: view.stack.clone(),
            action: view.action,
            args: view.args.clone(),
        };
        let outcome = self.execute(&req, &mut view).await;
        let (state, message) = match outcome {
            Ok(r) if r.ok => (JobState::Done, r.message),
            Ok(r) if r.deferred.is_some() => (JobState::Deferred, r.message),
            Ok(r) => (JobState::Failed, r.message),
            Err(Stop::Refused(r)) => (
                JobState::Refused,
                format!("{}: {} ({})", r.what, r.why, r.fix),
            ),
            Err(Stop::Link(e)) => (
                JobState::Unknown,
                format!("{e}; the activity page shows what the host did"),
            ),
        };
        // arch-secrets-read: what the host printed (an exec's output above
        // all) is masked by shape before it goes over the live channel.
        let message = homelab_core::executor::mask_secrets(&message);
        self.finish(&mut view, state, message.clone());
        let ran_s = view
            .started_at
            .zip(view.finished_at)
            .map(|(s, f)| (f - s).max(0) as u64);
        let kind = match state {
            JobState::Done => Kind::ActionDone,
            JobState::Deferred => Kind::ActionDeferred,
            _ => Kind::ActionFailed,
        };
        let place = if view.stack == actions::HOST_TARGET {
            String::new()
        } else {
            format!(" {}", view.stack)
        };
        let title = format!(
            "{}{}: {}",
            view.action.label(),
            place,
            match state {
                JobState::Done => "done",
                JobState::Deferred => "stood aside",
                JobState::Refused => "refused",
                JobState::Unknown => "outcome unknown",
                _ => "failed",
            }
        );
        // Decision notify-detail: the host's own notice of this job fills in
        // since when, the consequence and the remedy when it arrives
        // (`NotifyFile::import_host`); what the dashboard knows now is here.
        let on_stack = view.stack != actions::HOST_TARGET;
        let failed = !matches!(state, JobState::Done | JobState::Deferred);
        let detail = crate::core::notify::Detail {
            level: match state {
                JobState::Done => crate::core::notify::Level::Ok,
                JobState::Deferred => crate::core::notify::Level::Info,
                _ => crate::core::notify::Level::Warning,
            },
            since: view.started_at,
            link: Some(if on_stack {
                homelab_core::notify::page::stack(&view.stack)
            } else {
                format!("{}?job={}", homelab_core::notify::page::JOBS, view.job)
            }),
            label: Some(view.action.slug().to_string()),
            fixes: if failed {
                crate::core::notify::fix_for(&crate::core::notify::FixSource::Retry {
                    action: view.action.slug(),
                    stack: &view.stack,
                })
                .into_iter()
                .collect()
            } else {
                Vec::new()
            },
            ..Default::default()
        };
        self.inner
            .notify
            .notify(Draft {
                kind,
                op: format!("{}-{}", view.action.slug(), view.stack),
                stack: on_stack.then(|| view.stack.clone()),
                title,
                body: message,
                job: Some(view.job),
                ran_s,
                detail,
            })
            .await;
        if let Origin::Batch { batch } = view.origin {
            self.publish_batch(batch);
        }
    }

    async fn execute(&self, req: &ActionRequest, view: &mut JobView) -> Result<RpcResponse, Stop> {
        let material = self.material(req).await.map_err(Stop::Refused)?;
        let over = actions::cli_override(req, &material);
        let expected = match &material {
            Material::HostRelease { tag, .. } => {
                homelab_client::release::expected_host_version(tag)
            }
            _ => None,
        };
        let commands = actions::commands(req, material).map_err(Stop::Refused)?;
        let main = commands.last().cloned();
        // The name was typed and checked before the job ran (cli-yes).
        view.cli = over.or_else(|| {
            main.as_ref()
                .and_then(|c| cli_line_typed(c, req.args.force, true))
                .map(|l| match &req.args.commit {
                    Some(c) if req.action == ActionKind::DeployCommit => {
                        format!("git checkout {c} -- stacks/{} && {l}", req.stack)
                    }
                    _ => l,
                })
        });
        self.store(view);
        let history = self.history().await;
        let mut tracker = Tracker::new(history);
        let mut last = None;
        for command in commands {
            let r = self.follow(command, view, &mut tracker).await?;
            let ok = r.ok;
            last = Some(r);
            if !ok {
                break;
            }
        }
        let last = last.ok_or_else(|| Stop::Link("nothing was sent".into()))?;
        if req.action == ActionKind::UpdateHost && last.ok {
            return Ok(self.await_updated_host(view, expected, last).await);
        }
        Ok(last)
    }

    /// dash-host-update, fix-121: done means the shipped version answered
    /// after the restart, not that a restart was scheduled. The line drops
    /// and comes back on its own (arch-host-link); this watches the Hello.
    async fn await_updated_host(
        &self,
        view: &JobView,
        expected: Option<String>,
        sent: RpcResponse,
    ) -> RpcResponse {
        use homelab_client::release::{after_update, AfterUpdate};
        let wait = self
            .inner
            .reconnect_wait
            .get()
            .copied()
            .unwrap_or(Duration::from_secs(300));
        let say = |msg: String| {
            self.inner.publish.publish(
                "action_log",
                serde_json::json!({
                    "job": view.job, "req": null, "level": "info", "source": "ADMIN",
                    "msg": msg, "ts": self.now(),
                }),
            );
        };
        say(format!(
            "the host restarts into {}; the dashboard reconnects and waits for it (at most {} s)",
            expected.as_deref().unwrap_or("the new binary"),
            wait.as_secs()
        ));
        let end = tokio::time::Instant::now() + wait;
        let mut seen_down = false;
        loop {
            let (up, version) = {
                let s = self.inner.shared.read().await;
                (s.link_error.is_none(), s.host_version.clone())
            };
            if !up {
                seen_down = true;
            }
            let answered = if up { version.as_deref() } else { None };
            match after_update(expected.as_deref(), seen_down, answered) {
                AfterUpdate::Answered => {
                    let v = version.unwrap_or_default();
                    say(format!("the host answers as {v}: the update is accepted"));
                    return RpcResponse {
                        message: format!(
                            "{}\nthe host came back as {v} and answers; the update is accepted",
                            sent.message
                        ),
                        ..sent
                    };
                }
                AfterUpdate::RolledBack(v) => {
                    return RpcResponse {
                        ok: false,
                        message: format!(
                            "the host came back as {v}, not {}: its rollback ran; the journal on \
                             pve (journalctl -u homelab-host) says why",
                            expected.as_deref().unwrap_or("the new version")
                        ),
                        ..sent
                    };
                }
                AfterUpdate::Wait => {}
            }
            if tokio::time::Instant::now() >= end {
                return RpcResponse {
                    ok: false,
                    message: format!(
                        "the host did not come back as {} within {} s; the host page shows what \
                         answers now",
                        expected.as_deref().unwrap_or("the new version"),
                        wait.as_secs()
                    ),
                    ..sent
                };
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    /// Send one command and follow its lines until its reply.
    async fn follow(
        &self,
        command: Command,
        view: &mut JobView,
        tracker: &mut Tracker,
    ) -> Result<RpcResponse, Stop> {
        let mut events = self.inner.host.subscribe();
        let (sent_tx, mut sent_rx) = oneshot::channel();
        let host = self.inner.host.clone();
        let timeout = self.inner.timeout;
        let ask = host.ask_traced(command, timeout, Some(sent_tx));
        tokio::pin!(ask);
        let mut req: Option<u64> = None;
        let mut waiting_for_id = true;
        let mut events_open = true;
        let mut early: Vec<ServerMsg> = Vec::new();
        let reply = loop {
            // Biased: the id and the lines before the reply, so a reply that
            // is ready at the same moment cannot overtake them.
            tokio::select! {
                biased;
                id = &mut sent_rx, if waiting_for_id => {
                    waiting_for_id = false;
                    if let Ok(id) = id {
                        req = Some(id);
                        view.reqs.push(id);
                        self.store(view);
                        for m in std::mem::take(&mut early) {
                            self.line(m, id, view, tracker);
                        }
                    }
                }
                ev = events.recv(), if events_open => match ev {
                    Ok(m) => match req {
                        Some(id) => self.line(m, id, view, tracker),
                        None => early.push(m),
                    },
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        self.inner.publish.publish("action_log", serde_json::json!({
                            "job": view.job, "req": req, "level": "warn", "source": "ADMIN",
                            "msg": format!("{n} line(s) of the host were missed here; the activity page has them all"),
                            "ts": self.now(),
                        }));
                    }
                    Err(broadcast::error::RecvError::Closed) => events_open = false,
                },
                r = &mut ask => break r,
            }
        };
        // The id is sent before the reply, but both can be ready at once.
        if waiting_for_id {
            if let Ok(id) = sent_rx.try_recv() {
                req = Some(id);
                view.reqs.push(id);
                self.store(view);
                for m in std::mem::take(&mut early) {
                    self.line(m, id, view, tracker);
                }
            }
        }
        // Lines that came before the reply may still wait in the channel.
        if let Some(id) = req {
            while let Ok(m) = events.try_recv() {
                self.line(m, id, view, tracker);
            }
        }
        reply.map_err(Stop::Link)
    }

    fn line(&self, m: ServerMsg, id: u64, view: &mut JobView, tracker: &mut Tracker) {
        let ServerMsg::Log {
            level,
            source,
            msg,
            req: Some(r),
            ts,
            step,
            ..
        } = m
        else {
            return;
        };
        if r != id {
            return;
        }
        let ts = ts.map(|t| t as i64).unwrap_or_else(|| self.now());
        self.inner.publish.publish(
            "action_log",
            serde_json::json!({
                "job": view.job, "req": id, "level": level, "source": source,
                "msg": homelab_core::executor::mask_secrets(&msg), "ts": ts,
            }),
        );
        if let Some(mark) = step {
            let p = tracker.on_mark(&mark, ts.max(0) as u64);
            view.progress = Some(p.clone());
            self.inner.publish.publish(
                "action_progress",
                serde_json::json!({ "job": view.job, "req": id, "progress": p }),
            );
        }
    }

    /// feat-stacks-6: what a stack can go back to.
    pub async fn rollback_options(&self, stack: String) -> Result<serde_json::Value, Refusal> {
        if !actions::valid_stack_name(&stack) {
            return Err(Refusal::new(
                format!("roll-back list of {stack:?}"),
                "not a stack name",
                "use the name the fleet page shows",
            ));
        }
        let applied = self.fleet_stack(&stack).and_then(|(_, a)| a);
        let files = self.inner.files.clone();
        let s = stack.clone();
        let (commits, natives, present) = tokio::task::spawn_blocking(move || {
            (
                files.commits(&s, 30),
                files.native_units(&s),
                files.present(),
            )
        })
        .await
        .map_err(|e| Refusal::new("roll-back list", e.to_string(), "report this"))?;
        let applied_commit = applied
            .as_deref()
            .and_then(deployguard::applied_commit)
            .map(str::to_string);
        let commits: Vec<serde_json::Value> = commits
            .unwrap_or_default()
            .into_iter()
            .map(|c| {
                let is_applied = applied_commit
                    .as_deref()
                    .is_some_and(|a| c.commit.starts_with(a));
                serde_json::json!({
                    "commit": c.commit, "at": c.at, "subject": c.subject, "applied": is_applied,
                })
            })
            .collect();
        Ok(serde_json::json!({
            "stack": stack,
            "applied_source": applied,
            "applied_commit": applied_commit,
            "working_copy": present,
            // deploy-commit: the stack's files as they were at that commit.
            "commits": commits,
            // rollback-native: the host keeps one previous binary per unit
            // (fix-114); it does not say which version that is.
            "native_units": natives,
            "missing": [
                "the host does not list whether a native unit has a kept previous binary, or which version it is",
                "for compose apps the host keeps the previous image only during an update (automatic roll back on a failed health check); going back later means deploying an earlier commit",
            ],
        }))
    }
}

enum Stop {
    Refused(Refusal),
    Link(String),
}

// ── routes ──────────────────────────────────────────────────────────────

fn refusal(status: StatusCode, r: Refusal) -> Response {
    (status, Json(r)).into_response()
}

fn body<T>(b: Result<Json<T>, JsonRejection>, what: &str) -> Result<T, Refusal> {
    b.map(|Json(t)| t).map_err(|e| {
        Refusal::new(
            what,
            format!("the request body does not read: {}", e.body_text()),
            "send the JSON the catalog describes",
        )
    })
}

async fn catalog() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "actions": actions::catalog(),
        "host_target": actions::HOST_TARGET,
        "self_stack": actions::SELF_STACK,
    }))
}

async fn jobs(State(a): State<Actions>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "jobs": a.jobs() }))
}

fn accepted(view: &JobView) -> Response {
    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "job": view.job,
            "stack": view.stack,
            "action": view.action,
            "state": view.state,
            "restarts_dashboard": view.restarts_dashboard,
        })),
    )
        .into_response()
}

async fn start(
    State(a): State<Actions>,
    UrlPath((stack, action)): UrlPath<(String, String)>,
    b: Result<Json<ActionArgs>, JsonRejection>,
) -> Response {
    // An empty body is no arguments.
    let args = match b {
        Err(JsonRejection::MissingJsonContentType(_)) => ActionArgs::default(),
        other => match body(other, &format!("{action} {stack}")) {
            Ok(a) => a,
            Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
        },
    };
    let req = match actions::validate(&stack, &action, args) {
        Ok(r) => r,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    match a.press(req, Origin::Manual).await {
        Ok(view) => accepted(&view),
        Err(r) => refusal(StatusCode::CONFLICT, r),
    }
}

async fn batch(State(a): State<Actions>, b: Result<Json<BatchRequest>, JsonRejection>) -> Response {
    let b = match body(b, "batch") {
        Ok(b) => b,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    match a.run_batch(b) {
        Ok(v) => (StatusCode::ACCEPTED, Json(v)).into_response(),
        Err((status, r)) => refusal(status, r),
    }
}

/// feat-stacks-7 before the press: the CLI line and what the press would
/// do, read from the stack's manifest only (no secrets, no downloads).
async fn preview(
    State(a): State<Actions>,
    UrlPath((stack, action)): UrlPath<(String, String)>,
    b: Result<Json<ActionArgs>, JsonRejection>,
) -> Response {
    let mut args = match b {
        Err(JsonRejection::MissingJsonContentType(_)) => ActionArgs::default(),
        other => match body(other, &format!("{action} {stack}")) {
            Ok(a) => a,
            Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
        },
    };
    // cli-yes: the preview may come before the name is typed; the line
    // carries --yes only once it is.
    let typed = args.confirm.as_deref() == Some(stack.as_str());
    if ActionKind::from_slug(&action).is_some_and(|k| k.confirm()) && !typed {
        args.confirm = Some(stack.clone());
    }
    let req = match actions::validate(&stack, &action, args) {
        Ok(r) => r,
        Err(r) => return refusal(StatusCode::BAD_REQUEST, r),
    };
    let (stack, action) = (req.stack.clone(), req.action);
    let (line, guard, restarts) = a.preview_of(req, typed).await;
    let mut out = serde_json::json!({
        "stack": stack,
        "action": action,
        "entry": action.catalog_entry(),
        "cli": line.as_ref().ok(),
        "cli_unavailable": line.err(),
        "guard": guard,
        "restarts_dashboard": restarts,
    });
    // TUI parity: what the press would change, before it is pressed.
    match action {
        ActionKind::Deploy => match a.deploy_diff(&stack).await {
            Ok(d) => out["plan"] = d,
            Err(r) => out["plan_unavailable"] = serde_json::json!(r.why),
        },
        ActionKind::Apply => match a.apply_plan(false).await {
            Ok((view, _)) => out["apply"] = serde_json::json!(view),
            Err(r) => out["plan_unavailable"] = serde_json::json!(r.why),
        },
        _ => {}
    }
    Json(out).into_response()
}

fn preview_line(a: &Actions, req: &ActionRequest, typed: bool) -> Result<String, String> {
    // The lines that name more than the command carries, without a
    // download: the tag as typed ("latest" when empty), apply's flags.
    let tag = || {
        req.args
            .tag
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .unwrap_or("")
            .to_string()
    };
    match req.action.needs() {
        Needs::HostRelease => {
            return Ok(format!("homelab release-update {}", tag())
                .trim_end()
                .to_string())
        }
        Needs::NativeRelease => {
            let (_, _, dir) = a
                .inner
                .files
                .native_release(&req.stack, req.args.unit.as_deref())
                .map_err(|r| r.why)?;
            return Ok(format!("homelab install-native {dir} {}", tag())
                .trim_end()
                .to_string());
        }
        Needs::Apply => {
            return actions::cli_override(req, &Material::None).ok_or_else(|| "internal".into())
        }
        _ => {}
    }
    let material = match req.action.needs() {
        Needs::Spec => {
            // The line names the stack; a spec with the manifest alone says
            // the same thing without reading secrets or releases.
            let probe = ActionRequest {
                action: ActionKind::Backup,
                ..req.clone()
            };
            match a
                .inner
                .files
                .read(&probe.stack, probe.action, None)
                .map_err(|r| r.why)?
            {
                Material::Manifest(m) => Material::Spec(Box::new(homelab_proto::DeploySpec {
                    secret_files: Vec::new(),
                    manifest: *m,
                    files: Vec::new(),
                    env: BTreeMap::new(),
                    gateway_route: None,
                    extra_routes: Vec::new(),
                    checks: BTreeMap::new(),
                    native_binaries: BTreeMap::new(),
                    native_manifests: BTreeMap::new(),
                    source: None,
                })),
                _ => return Err("the stack files did not read".into()),
            }
        }
        Needs::Vmid => Material::Vmid(
            a.fleet_stack(&req.stack)
                .map(|(v, _)| v)
                .ok_or("the fleet has no such stack")?,
        ),
        _ => a
            .inner
            .files
            .read(&req.stack, req.action, None)
            .map_err(|r| r.why)?,
    };
    let commands = actions::commands(req, material).map_err(|r| r.why)?;
    let main = commands.last().ok_or("no command")?;
    let line = cli_line_typed(main, req.args.force, typed).ok_or("no CLI verb sends this")?;
    Ok(match &req.args.commit {
        Some(c) if req.action == ActionKind::DeployCommit => {
            format!("git checkout {c} -- stacks/{} && {line}", req.stack)
        }
        _ => line,
    })
}

async fn rollback_options(State(a): State<Actions>, UrlPath(stack): UrlPath<String>) -> Response {
    match a.rollback_options(stack).await {
        Ok(v) => Json(v).into_response(),
        Err(r) => refusal(StatusCode::BAD_REQUEST, r),
    }
}

/// Mounted with `dashboard_routes`: the login and both locks stand before
/// every one of them.
pub fn router(actions: Actions) -> Router {
    Router::new()
        .route("/data/actions/catalog", get(catalog))
        .route("/data/actions/jobs", get(jobs))
        .route("/data/actions/batch", post(batch))
        .route(
            "/data/actions/{stack}/rollback-options",
            get(rollback_options),
        )
        .route("/data/actions/{stack}/{action}", post(start))
        .route("/data/actions/{stack}/{action}/preview", post(preview))
        .with_state(actions)
}

/// milestone act, wired into the app: the action queue, the notification
/// center and the scheduler, their routes, and (on the serving path only)
/// the scheduler's tick and the incident poll. Settings come from
/// `HOMELAB_ADMIN_*` (`core::actions_config`).
pub fn mount(
    app: &mut chassis::App,
    host: HostClient,
    live: chassis::shell::live::Live,
    shared: Shared,
    demo_host: bool,
    hooks: super::actions_notify::HookSlot,
) -> Result<(), String> {
    let cfg = crate::core::actions_config::from_env(&|k| std::env::var(k).ok())?;
    let clock = system_clock();
    let live_for_parity = live.clone();
    let publish: Arc<dyn Publish> = Arc::new(live);
    let host: Arc<dyn HostPort> = Arc::new(host);
    let pusher: Arc<dyn super::actions_notify::Pusher> = match &cfg.notify_url {
        Some(url) => Arc::new(super::actions_notify::KyuPusher::new(
            url.clone(),
            cfg.notify_token.clone(),
        )),
        None => Arc::new(super::actions_notify::NoPusher),
    };
    let shared_for_edit = shared.clone();
    let shared_for_drive = shared.clone();
    let publish_for_drive = publish.clone();
    let clock_for_drive = clock.clone();
    let publish_for_edit = publish.clone();
    let notify = NotifyCenter::load(cfg.notify_file(), pusher, publish.clone(), clock.clone())
        .map_err(|e| e.to_string())?;
    // Decision notify-routing / notify-detail: pushes link to this address,
    // and Alertmanager's hook now has somewhere to go.
    notify.set_base_url(&cfg.public_url);
    hooks.fill(notify.clone(), cfg.alerts_token.clone());
    // milestone edit (arch-edit-txn): the working copy, cloned at start.
    let wc = Arc::new(super::workcopy::WorkingCopy::new(
        cfg.repo.clone(),
        cfg.git.clone(),
        cfg.scratch_dir(),
        clock.clone(),
    ));
    let files: Arc<dyn StackFiles> = Arc::new(RepoFiles {
        repo: cfg.repo.clone(),
        scratch: cfg.scratch_dir(),
        wc: Some(wc.clone()),
    });
    let shared_for_parity = shared.clone();
    // replace-kuma: the minute watch, and what it holds for the start page.
    let shared_for_watch = shared.clone();
    let watched: super::watch::Watched = Default::default();
    app.dashboard_routes(super::watch::router(watched.clone()));
    let watch_via = cfg.watch_via.clone();
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: publish.clone(),
        files: files.clone(),
        shared,
        notify: notify.clone(),
        clock: clock.clone(),
        timeout: cfg.action_timeout(),
    });
    let scheduler = super::scheduler::Scheduler::load(
        cfg.schedules_file(),
        actions.clone(),
        notify.clone(),
        publish.clone(),
        clock.clone(),
        cfg.schedule_grace_s,
    )
    .map_err(|e| e.to_string())?;
    let edit_ctx = super::edit::EditCtx {
        wc: wc.clone(),
        actions: actions.clone(),
        host: host.clone(),
        shared: shared_for_edit,
        publish: publish_for_edit,
    };
    app.dashboard_routes(super::edit::router(edit_ctx.clone()));
    // feat-platform-10: the driver, its catch-up route and its relay; it
    // drives the edit forms through the same editor.
    let driver = super::drive::Driver::with_edit(
        actions.clone(),
        shared_for_drive,
        publish_for_drive,
        clock_for_drive,
        Some(edit_ctx),
    );
    driver.set_timing(super::drive::LiveTiming {
        announce: Duration::from_millis(cfg.live_announce_ms),
        max_pause: Duration::from_secs(cfg.live_max_pause_s),
    });
    app.dashboard_routes(super::drive::router(driver.clone()));
    #[cfg(feature = "demo-host")]
    if demo_host {
        app.dashboard_routes(super::drive::demo_router(driver.clone()));
    }
    let _ = demo_host;
    // TUI parity: releases (Update host, install-native, the badge), the
    // host's log stream, and the read routes the TUI and CLI had alone.
    let releases: Arc<dyn super::releases::Releases> = Arc::new(super::releases::GitHub::new());
    actions.set_releases(releases.clone());
    let hostlog = super::hostlog::HostLog::new(publish.clone(), clock.clone());
    app.dashboard_routes(super::hostlog::router(hostlog.clone()));
    app.dashboard_routes(super::parity::router(super::parity::ParityCtx::new(
        host.clone(),
        shared_for_parity.clone(),
        actions.clone(),
        files,
        cfg.repo.clone(),
        cfg.scratch_dir(),
        publish.clone(),
    )));
    app.dashboard_routes(router(actions.clone()));
    app.dashboard_routes(super::actions_notify::router(notify.clone()));
    app.dashboard_routes(super::scheduler::router(scheduler.clone()));
    let (tick, poll) = (cfg.tick(), Duration::from_secs(cfg.incidents_poll_s));
    let host_notices_poll = Duration::from_secs(cfg.host_notices_poll_s);
    let actions_for_notices = actions.clone();
    let job_of: Arc<dyn Fn(u64) -> Option<u64> + Send + Sync> =
        Arc::new(move |req| actions_for_notices.job_for_req(req));
    let digest_host = host.clone();
    let digest_repo = cfg.repo.clone();
    let today: super::actions_notify::TodayRead = Arc::new(move || {
        let host = digest_host.clone();
        let repo = digest_repo.clone();
        Box::pin(async move {
            super::parity::fetch_today(&host, &repo)
                .await
                .map(|t| crate::core::notify::today_lines(&t))
        })
    });
    let git = cfg.git.clone();
    app.on_start(move || {
        // Clone or bring the working copy up to date, off the async threads.
        tokio::task::spawn_blocking(move || {
            // CT 120: the deploy key arrives from latch as an environment
            // variable; ssh wants a file (and GitHub's host keys).
            let key = std::env::var(crate::core::credentials::DEPLOY_KEY_ENV).ok();
            let p = super::workcopy::provision_credentials(&git, key.as_deref());
            if p.key_written {
                tracing::info!(path = %git.key.display(), "the deploy key file was written from the environment (mode 0600)");
            }
            if p.known_hosts_written {
                tracing::info!(path = %git.known_hosts.display(), "GitHub's pinned host keys were written");
            }
            for why in &p.problems {
                tracing::warn!(why = %why, "the working copy's credentials");
            }
            if let Err(r) = wc.sync() {
                tracing::warn!(why = %r.why, fix = %r.fix, "the working copy is not ready");
            }
        });
        scheduler.spawn(tick);
        driver.spawn_relay(host.clone());
        driver.spawn_release();
        hostlog.spawn(host.clone());
        // The demo host answers without the internet; the badge would not.
        if !demo_host {
            super::releases::spawn_watch(
                releases,
                shared_for_parity,
                live_for_parity,
                super::releases::WATCH_EVERY,
            );
        }
        super::actions_notify::spawn_host_notice_poll(
            host.clone(),
            notify.clone(),
            job_of,
            host_notices_poll,
        );
        super::actions_notify::spawn_digest(notify.clone(), today, Duration::from_secs(60));
        super::watch::spawn(
            host.clone(),
            shared_for_watch,
            notify.clone(),
            watch_via,
            watched,
        );
        super::actions_notify::spawn_incident_poll(host, notify, poll);
    });
    Ok(())
}
