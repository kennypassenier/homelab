//! Shared doubles for the milestone-act tests: a mock host, a recording live
//! channel, a recording push route (never kyu), stack files in memory and a
//! clock that only moves when a test moves it.
//!
//! The mock host keeps the host's session behaviour the actions depend on
//! (arch-tests): ONE serial worker runs commands in arrival order; ids are
//! handed out as the command goes on the line; every `Log` is broadcast to
//! every listener before the reply, stamped with its request id; and lines
//! of another session's request (id 1, token "wsl") flow past on the same
//! channel. It is a double of the session, not the real loop: the host is a
//! binary crate, and its own tests drive the real loop (fix-66, arch-tokens).

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use homelab_admin::core::actions::{ActionKind, Material, Needs, Refusal};
use homelab_admin::shell::actions::{Clock, CommitInfo, HostPort, Publish, StackFiles};
use homelab_admin::shell::actions_notify::Pusher;
use homelab_core::ops::deployguard::Ancestry;
use homelab_proto::{
    Command, DeploySpec, LogLevel, NativeServiceManifest, RpcResponse, ServerMsg, StackManifest,
    StepMark,
};
use tokio::sync::{broadcast, mpsc, oneshot};

pub fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// A real stack's manifest from the repository (read only).
pub fn manifest(stack: &str) -> StackManifest {
    homelab_client::spec::build_manifest(&repo().join("stacks").join(stack)).expect("a manifest")
}

pub fn native(stack: &str) -> NativeServiceManifest {
    let raw = std::fs::read_to_string(repo().join("stacks").join(stack).join("service.yml"))
        .expect("a service.yml");
    serde_yaml::from_str(&raw).expect("a native manifest")
}

pub fn spec(stack: &str) -> DeploySpec {
    DeploySpec {
        secret_files: Vec::new(),
        manifest: manifest(stack),
        files: Vec::new(),
        env: BTreeMap::new(),
        gateway_route: None,
        extra_routes: Vec::new(),
        checks: BTreeMap::new(),
        native_binaries: BTreeMap::new(),
        native_manifests: BTreeMap::new(),
        source: None,
    }
}

// ── clock ────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TestClock(pub Arc<AtomicI64>);

impl TestClock {
    pub fn at(t: i64) -> Self {
        TestClock(Arc::new(AtomicI64::new(t)))
    }
    pub fn now(&self) -> i64 {
        self.0.load(Ordering::SeqCst)
    }
    pub fn set(&self, t: i64) {
        self.0.store(t, Ordering::SeqCst)
    }
    pub fn advance(&self, s: i64) {
        self.0.fetch_add(s, Ordering::SeqCst);
    }
    pub fn clock(&self) -> Clock {
        let c = self.0.clone();
        Arc::new(move || c.load(Ordering::SeqCst))
    }
}

// ── live channel ─────────────────────────────────────────────────────────

#[derive(Default)]
pub struct Recorder(pub Mutex<Vec<(String, serde_json::Value)>>);

impl Publish for Recorder {
    fn publish(&self, event: &str, data: serde_json::Value) {
        self.0.lock().unwrap().push((event.to_string(), data));
    }
}

impl Recorder {
    pub fn events(&self, name: &str) -> Vec<serde_json::Value> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| e == name)
            .map(|(_, v)| v.clone())
            .collect()
    }
}

// ── push route ───────────────────────────────────────────────────────────

#[derive(Default)]
pub struct RecPusher {
    pub sent: Mutex<Vec<String>>,
    pub fail: bool,
}

impl Pusher for RecPusher {
    fn push(
        &self,
        payload: String,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), String>> + Send + '_>> {
        Box::pin(async move {
            self.sent.lock().unwrap().push(payload);
            if self.fail {
                Err("HTTP 503".into())
            } else {
                Ok(())
            }
        })
    }
}

// ── stack files ──────────────────────────────────────────────────────────

pub struct MemFiles {
    pub present: bool,
    pub ancestry: Ancestry,
    pub commits: Vec<CommitInfo>,
    /// Stacks whose directory is gone (a destroy falls back to the record).
    pub gone: Vec<String>,
    pub reads: Mutex<Vec<(String, ActionKind, Option<String>)>>,
}

impl Default for MemFiles {
    fn default() -> Self {
        MemFiles {
            present: true,
            ancestry: Ancestry::Contained,
            commits: Vec::new(),
            gone: Vec::new(),
            reads: Mutex::new(Vec::new()),
        }
    }
}

impl StackFiles for MemFiles {
    fn read(
        &self,
        stack: &str,
        kind: ActionKind,
        commit: Option<&str>,
    ) -> Result<Material, Refusal> {
        self.reads
            .lock()
            .unwrap()
            .push((stack.to_string(), kind, commit.map(str::to_string)));
        if !self.present {
            return Err(Refusal::new("read", "no working copy", "clone it"));
        }
        let real = |s: &str| {
            // Tests act on "media" (compose) and "admin" (native); any other
            // name is read as "media" renamed, so a stack name is just a name.
            let mut m = manifest(if s == "admin" { "admin" } else { "media" });
            m.stack_name = s.to_string();
            m
        };
        Ok(match kind.needs() {
            Needs::Nothing | Needs::Vmid => Material::None,
            Needs::Manifest if self.gone.iter().any(|g| g == stack) => Material::None,
            Needs::Manifest => Material::Manifest(Box::new(real(stack))),
            Needs::Spec => {
                let mut s = spec("media");
                s.manifest = real(stack);
                Material::Spec(Box::new(s))
            }
            Needs::NativeManifest => Material::Native(Box::new(native("admin"))),
            // Read elsewhere (releases, the whole stacks directory).
            Needs::HostRelease | Needs::NativeRelease | Needs::Apply => Material::None,
        })
    }
    fn ancestry(&self, _commit: &str) -> Ancestry {
        self.ancestry
    }
    fn commits(&self, _stack: &str, _limit: usize) -> Result<Vec<CommitInfo>, Refusal> {
        Ok(self.commits.clone())
    }
    fn native_units(&self, stack: &str) -> Vec<String> {
        if stack == "admin" {
            vec!["admin".into()]
        } else {
            Vec::new()
        }
    }
    fn present(&self) -> bool {
        self.present
    }
}

// ── mock host ────────────────────────────────────────────────────────────

/// What the mock host does with one command.
#[derive(Debug, Clone)]
pub struct Script {
    /// Steps with how many seconds each takes on the test clock.
    pub steps: Vec<(String, i64)>,
    pub ok: bool,
    pub deferred: Option<String>,
    pub message: String,
    /// The line drops before the answer.
    pub drop_line: bool,
}

impl Script {
    pub fn ok(steps: &[(&str, i64)]) -> Self {
        Script {
            steps: steps.iter().map(|(s, d)| (s.to_string(), *d)).collect(),
            ok: true,
            deferred: None,
            message: "complete".into(),
            drop_line: false,
        }
    }
    pub fn failed(steps: &[(&str, i64)], why: &str) -> Self {
        Script {
            ok: false,
            message: why.into(),
            ..Script::ok(steps)
        }
    }
}

type Asked = (
    Command,
    oneshot::Sender<Result<RpcResponse, String>>,
    Option<oneshot::Sender<u64>>,
);

pub type Behaviour = Arc<dyn Fn(&Command) -> Script + Send + Sync>;

pub struct MockHost {
    events: broadcast::Sender<ServerMsg>,
    tx: mpsc::UnboundedSender<Asked>,
    /// (id, command name, stack) of every command the worker ran, in order.
    pub ran: Arc<Mutex<Vec<(u64, String, String)>>>,
}

/// The stack a command is about, for the record.
pub fn stack_of(c: &Command) -> String {
    use Command::*;
    match c {
        DeployStack(s) => s.manifest.stack_name.clone(),
        BackupStack(m) | ApplyResources(m) => m.stack_name.clone(),
        RestoreStack { manifest, .. }
        | UpdateStack { manifest, .. }
        | DestroyStack { manifest, .. }
        | PruneOrphans { manifest, .. } => manifest.stack_name.clone(),
        AdoptService(m) => m.stack_name.clone(),
        SetStackEnabled { stack, .. }
        | BackupNative { stack }
        | UpdateNative { stack }
        | ReleaseUpdateNative { stack }
        | RollbackNative { stack, .. }
        | ForgetStack { stack }
        | DestroyRecorded { stack, .. }
        | StageNativeBinary { stack, .. } => stack.clone(),
        WipeRetired { name, .. } => name.clone(),
        ApplyGuards { vmid } => vmid.to_string(),
        _ => String::new(),
    }
}

impl MockHost {
    /// `history`: what `History` answers; `incidents`: what `Incidents`
    /// lists (shared, so a test can add a bundle later).
    pub fn start(
        clock: TestClock,
        behaviour: Behaviour,
        history: serde_json::Value,
        incidents: Arc<Mutex<Vec<String>>>,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(4096);
        let (tx, mut rx) = mpsc::unbounded_channel::<Asked>();
        let ran = Arc::new(Mutex::new(Vec::new()));
        let host = Arc::new(MockHost {
            events: events.clone(),
            tx,
            ran: ran.clone(),
        });
        tokio::spawn(async move {
            let mut next: u64 = 5_000;
            while let Some((command, reply, sent)) = rx.recv().await {
                let id = next;
                next += 1;
                if let Some(s) = sent {
                    let _ = s.send(id);
                }
                let log = |msg: String, req: Option<u64>, step: Option<StepMark>, by: &str| {
                    ServerMsg::Log {
                        level: LogLevel::Info,
                        source: "HOST".into(),
                        msg,
                        req,
                        ts: Some(clock.now() as u64),
                        step,
                        by: Some(by.into()),
                    }
                };
                let ok_reply = |message: String| {
                    Ok(RpcResponse {
                        id,
                        ok: true,
                        message,
                        deferred: None,
                    })
                };
                match &command {
                    Command::History { .. } => {
                        let _ = reply.send(ok_reply(history.to_string()));
                        continue;
                    }
                    Command::Incidents { .. } => {
                        let list = incidents.lock().unwrap().clone();
                        let _ = reply.send(ok_reply(
                            serde_json::json!({ "incidents": list }).to_string(),
                        ));
                        continue;
                    }
                    _ => {}
                }
                ran.lock()
                    .unwrap()
                    .push((id, command.name().to_string(), stack_of(&command)));
                let script = behaviour(&command);
                let op = format!(
                    "{}-{}",
                    command.name().split('_').next().unwrap_or("op"),
                    stack_of(&command)
                );
                // Another session's request flows past on the same channel.
                let _ = events.send(log("a CLI line".into(), Some(1), None, "wsl"));
                for (step, secs) in &script.steps {
                    let mark = |finished| StepMark {
                        op: op.clone(),
                        step: step.clone(),
                        finished,
                        changed: finished,
                    };
                    let _ = events.send(log(
                        format!("[sync][run ] {op} :: {step}"),
                        Some(id),
                        Some(mark(false)),
                        "admin",
                    ));
                    let _ = events.send(log(format!("working on {step}"), Some(id), None, "admin"));
                    clock.advance(*secs);
                    let _ = events.send(log(
                        format!("[sync][exit] {op} :: {step}"),
                        Some(id),
                        Some(mark(true)),
                        "admin",
                    ));
                }
                if script.drop_line {
                    let _ = reply.send(Err(
                        "the line to the host dropped before the answer; the outcome is unknown"
                            .into(),
                    ));
                    continue;
                }
                let _ = reply.send(Ok(RpcResponse {
                    id,
                    ok: script.ok,
                    message: script.message.clone(),
                    deferred: script.deferred.clone(),
                }));
            }
        });
        host
    }

    pub fn ran(&self) -> Vec<(u64, String, String)> {
        self.ran.lock().unwrap().clone()
    }
}

impl HostPort for MockHost {
    fn subscribe(&self) -> broadcast::Receiver<ServerMsg> {
        self.events.subscribe()
    }
    fn ask_traced(
        &self,
        command: Command,
        timeout: Duration,
        sent: Option<oneshot::Sender<u64>>,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<RpcResponse, String>> + Send + '_>> {
        Box::pin(async move {
            let (reply, wait) = oneshot::channel();
            self.tx
                .send((command, reply, sent))
                .map_err(|_| "the host link is not running".to_string())?;
            match tokio::time::timeout(timeout, wait).await {
                Ok(Ok(r)) => r,
                _ => Err("no answer".into()),
            }
        })
    }
}

/// History in the host's reply shape: runs of `subject` with these step
/// durations (seconds), `ok` each.
pub fn history(subject: &str, runs: &[(&[(&str, u64)], bool)]) -> serde_json::Value {
    let mut entries = Vec::new();
    let mut t = 1_000_000u64;
    for (steps, ok) in runs {
        let start = t;
        let mut timings = Vec::new();
        for (s, d) in steps.iter() {
            timings.push(serde_json::json!({"step": s, "start": t, "end": t + d, "changed": true}));
            t += d;
        }
        entries.push(serde_json::json!({
            "kind": "op", "start": start, "end": t, "label": "deploy",
            "subject": subject, "ok": ok, "steps": timings,
        }));
        t += 1000;
    }
    serde_json::json!({ "entries": entries })
}

/// A fleet snapshot holding `stacks` as (name, vmid, applied_source).
pub fn shared(stacks: &[(&str, u16, Option<&str>)]) -> homelab_admin::shell::host_link::Shared {
    let state: homelab_proto::FleetState = serde_json::from_value(serde_json::json!({
        "host": {"name": "pve", "cpu_pct": 1, "ram_pct": 1, "disk_pct": 1, "tls_fingerprint": ""},
        "stacks": stacks.iter().map(|(n, v, a)| serde_json::json!({
            "name": n, "vmid": v, "hostname": n, "apps": [], "drift": false,
            "env_sealed": true, "online": true, "enabled": true, "applied_source": a,
        })).collect::<Vec<_>>(),
    }))
    .expect("a fleet state");
    let snap = homelab_admin::shell::host_link::Snapshot {
        fleet: Some(homelab_admin::core::fleet::fleet_view(&state, 1)),
        ..Default::default()
    };
    Arc::new(tokio::sync::RwLock::new(snap))
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("homelab-admin-act-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Wait (on real time, briefly) until `f` holds.
pub async fn until(what: &str, f: impl Fn() -> bool) {
    for _ in 0..500 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for: {what}");
}
