//! The Executor trait (AR2): every side effect in the system flows through
//! here. Production implements it with real processes and files (in the host
//! crate); tests use [`MockExecutor`] to script responses and record calls.

use async_trait::async_trait;

use crate::error::CoreError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub timeout_s: u64,
}

impl Cmd {
    pub fn new(program: &str, args: &[&str], timeout_s: u64) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            timeout_s,
        }
    }

    pub fn rendered(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

#[derive(Debug, Clone, Default)]
pub struct CmdOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: i32,
}

impl CmdOutput {
    pub fn ok(stdout: &str) -> Self {
        Self {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: 0,
        }
    }
    pub fn failed(code: i32, stderr: &str) -> Self {
        Self {
            stdout: String::new(),
            stderr: stderr.to_string(),
            code,
        }
    }
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

/// All side effects. `write_file` MUST be atomic (tmp + rename) per AR4.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Run a command; returns Ok even when the command exits non-zero — the
    /// caller decides what a failure means. Err is reserved for spawn
    /// failures and timeouts.
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError>;

    /// Atomically write a host-local file with the given mode.
    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError>;

    /// Read a host-local file; Err when it does not exist.
    async fn read_file(&self, path: &str) -> Result<String, CoreError>;

    /// Sleep — routed through the trait so tests run instantly.
    async fn sleep_ms(&self, ms: u64);
}

/// Decorator that emits every command as a `[run ]` transcript line through a
/// sink, so transcripts are streamed live (F2) AND captured for incident
/// replay (AR16) from one place. Wrap the real/mock executor with this in any
/// path that should produce a transcript.
pub struct TracingExecutor<'a> {
    inner: &'a dyn Executor,
    sink: &'a dyn crate::sink::Sink,
}

impl<'a> TracingExecutor<'a> {
    pub fn new(inner: &'a dyn Executor, sink: &'a dyn crate::sink::Sink) -> Self {
        Self { inner, sink }
    }
    fn line(&self, msg: String) {
        self.sink.emit(crate::sink::PipelineEvent::Line {
            level: crate::sink::Level::Debug,
            source: "HOST".into(),
            msg,
        });
    }
}

#[async_trait]
impl Executor for TracingExecutor<'_> {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        self.line(format!("[run ] {}", cmd.rendered()));
        let out = self.inner.run(cmd).await?;
        for l in out.stdout.lines().chain(out.stderr.lines()).take(20) {
            if !l.trim().is_empty() {
                self.line(format!("  {}", l));
            }
        }
        Ok(out)
    }
    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
        self.inner.write_file(path, content, mode).await
    }
    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        self.inner.read_file(path).await
    }
    async fn sleep_ms(&self, ms: u64) {
        self.inner.sleep_ms(ms).await
    }
}

/// Convenience: run and require success.
pub async fn run_ok(exec: &dyn Executor, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
    let out = exec.run(cmd).await?;
    if out.success() {
        Ok(out)
    } else {
        Err(CoreError::Command {
            rendered: cmd.rendered(),
            detail: format!("rc={} :: {}", out.code, out.stderr.trim()),
        })
    }
}

/// Run a shell script inside an LXC via `pct exec`.
pub async fn pct_sh(
    exec: &dyn Executor,
    vmid: u16,
    script: &str,
    timeout_s: u64,
) -> Result<CmdOutput, CoreError> {
    let vm = vmid.to_string();
    exec.run(&Cmd::new(
        "pct",
        &["exec", &vm, "--", "sh", "-c", script],
        timeout_s,
    ))
    .await
}

// ── Mock ────────────────────────────────────────────────────────────────────

use std::collections::HashMap;
use std::sync::Mutex;

/// Scripted test double. Default behavior: every command succeeds with empty
/// output; files behave like an in-memory filesystem. Script exceptions with
/// [`MockExecutor::enqueue`] (consumed once, in order, first match wins) or
/// [`MockExecutor::respond_always`].
#[derive(Default)]
pub struct MockExecutor {
    calls: Mutex<Vec<String>>,
    /// Rendered command paired with the timeout it was given. A timeout is
    /// part of whether an operation can succeed at all — a restore that dies
    /// at thirty minutes fails for a reason no argv assertion can see — so it
    /// has to be assertable (deployment project, F38).
    timeouts: Mutex<Vec<(String, u64)>>,
    queue: Mutex<Vec<(String, CmdOutput)>>,
    always: Mutex<Vec<(String, CmdOutput)>>,
    files: Mutex<HashMap<String, (String, u32)>>,
    /// What `pct push` has landed inside the container, keyed by destination.
    container_files: Mutex<HashMap<String, String>>,
    /// What `pct set` has changed about the container, keyed by config key.
    container_config: Mutex<HashMap<String, String>>,
    /// What `pct create`/`pct clone` asked for; only fills gaps.
    container_created: Mutex<HashMap<String, String>>,
    /// Which app containers `docker compose up -d` has started.
    container_running: Mutex<std::collections::BTreeSet<String>>,
}

impl MockExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Next command whose rendered form contains `matcher` returns `out`
    /// (consumed once).
    pub fn enqueue(&self, matcher: &str, out: CmdOutput) {
        self.queue.lock().unwrap().push((matcher.to_string(), out));
    }

    /// Like `respond_always`, but wins over rules registered earlier. Needed
    /// because the shared test harness now models a HEALTHY container, and a
    /// test about an unhealthy one has to be able to say so afterwards.
    pub fn respond_first(&self, matcher: &str, out: CmdOutput) {
        self.always
            .lock()
            .unwrap()
            .insert(0, (matcher.to_string(), out));
    }

    /// Every command whose rendered form contains `matcher` returns `out`
    /// (unless a queued rule matched first).
    pub fn respond_always(&self, matcher: &str, out: CmdOutput) {
        self.always.lock().unwrap().push((matcher.to_string(), out));
    }

    /// Rendered forms of every executed command, in order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// Timeouts given to every command whose rendered form contains `needle`.
    pub fn timeouts_for(&self, needle: &str) -> Vec<u64> {
        self.timeouts
            .lock()
            .unwrap()
            .iter()
            .filter(|(c, _)| c.contains(needle))
            .map(|(_, t)| *t)
            .collect()
    }

    pub fn calls_containing(&self, needle: &str) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.contains(needle))
            .collect()
    }

    /// All paths written through write_file (for scan-style assertions).
    pub fn file_paths(&self) -> Vec<String> {
        self.files.lock().unwrap().keys().cloned().collect()
    }

    pub fn file(&self, path: &str) -> Option<String> {
        self.files.lock().unwrap().get(path).map(|(c, _)| c.clone())
    }

    pub fn file_mode(&self, path: &str) -> Option<u32> {
        self.files.lock().unwrap().get(path).map(|(_, m)| *m)
    }

    pub fn seed_file(&self, path: &str, content: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), (content.to_string(), 0o644));
    }
}

#[async_trait]
impl Executor for MockExecutor {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        let rendered = cmd.rendered();
        self.calls.lock().unwrap().push(rendered.clone());
        self.timeouts
            .lock()
            .unwrap()
            .push((rendered.clone(), cmd.timeout_s));
        // S2: model the container's filesystem well enough that a step which
        // reads its own work back gets a truthful answer. `pct push` moves
        // the staged file to a destination; a later `sha256sum` of that
        // destination must then agree. Without this the mock answers every
        // read with silence, and a verified step can only ever fail — which
        // would make the harness argue against the very check it should be
        // proving. Scripted responses still win: a test that wants a push to
        // land wrong says so, and this stays out of its way.
        // Likewise for `pct set`: a deploy that corrects a container's boot
        // policy or attaches a missing mount must be able to read that back
        // afterwards. Recorded as plain `pct config` lines so the scripted
        // "before" answer and the modelled "after" can be merged below.
        // `set` takes the vmid then flag/value pairs; `create` and `clone`
        // take one more positional first. Both describe the container that
        // exists afterwards, which is what a reconciliation reads back.
        // `set` is a deliberate later change and outranks whatever a test
        // scripted; `create`/`clone` only describe the starting state, and a
        // scripted answer may legitimately know more than the command line
        // did — Proxmox assigns net0's hwaddr itself, so replacing the
        // scripted net0 with the one from `pct create` argv would throw the
        // MAC address away.
        let cfg_args = match cmd.args.first().map(|a| a.as_str()) {
            Some("set") if cmd.program == "pct" => Some((2, true)),
            Some("create") | Some("clone") if cmd.program == "pct" => Some((3, false)),
            _ => None,
        };
        if let Some((skip, authoritative)) = cfg_args {
            let mut it = cmd.args.iter().skip(skip);
            while let Some(flag) = it.next() {
                if !flag.starts_with('-') {
                    continue;
                }
                let Some(value) = it.next() else { break };
                let key = flag.trim_start_matches('-');
                let mut target = if authoritative {
                    self.container_config.lock().unwrap()
                } else {
                    self.container_created.lock().unwrap()
                };
                target.insert(key.to_string(), value.clone());
            }
        }
        // Which app containers are up. `docker compose up -d` in an app's own
        // directory starts it; `down` stops it. Modelled because the check
        // that matters — is it actually running — cannot be answered by the
        // exit code of the command that started it.
        if rendered.contains("docker compose") {
            if let Some(dir) = rendered.split('\'').nth(1) {
                if let Some(app) = dir.rsplit('/').next() {
                    let mut up = self.container_running.lock().unwrap();
                    if rendered.contains("compose up") {
                        up.insert(app.to_string());
                    } else if rendered.contains("compose down") {
                        up.remove(app);
                    }
                }
            }
        }
        if rendered.contains("docker ps --format") {
            let up = self.container_running.lock().unwrap();
            if !up.is_empty() {
                let scripted = {
                    let always = self.always.lock().unwrap();
                    always.iter().any(|(m, _)| rendered.contains(m))
                };
                if !scripted {
                    let mut out: Vec<&str> = up.iter().map(|s| s.as_str()).collect();
                    out.sort();
                    return Ok(CmdOutput::ok(&format!("{}\n", out.join("\n"))));
                }
            }
        }
        if cmd.program == "pct" && cmd.args.first().map(|a| a.as_str()) == Some("push") {
            if let (Some(src), Some(dest)) = (cmd.args.get(2), cmd.args.get(3)) {
                let content = self.files.lock().unwrap().get(src).map(|(c, _)| c.clone());
                if let Some(c) = content {
                    self.container_files.lock().unwrap().insert(dest.clone(), c);
                }
            }
        }
        {
            let mut queue = self.queue.lock().unwrap();
            if let Some(pos) = queue.iter().position(|(m, _)| rendered.contains(m)) {
                return Ok(queue.remove(pos).1);
            }
        }
        if cmd.program == "pct" && cmd.args.first().map(|a| a.as_str()) == Some("config") {
            let cfg = self.container_config.lock().unwrap();
            let created = self.container_created.lock().unwrap();
            if !cfg.is_empty() || !created.is_empty() {
                let scripted = {
                    let always = self.always.lock().unwrap();
                    always
                        .iter()
                        .find(|(m, _)| rendered.contains(m))
                        .map(|(_, o)| o.stdout.clone())
                        .unwrap_or_default()
                };
                let mut out = scripted;
                // Additive first: only keys the scripted answer does not
                // already carry.
                for (k, v) in created.iter() {
                    if !out.lines().any(|l| l.starts_with(&format!("{}:", k))) {
                        if !out.is_empty() && !out.ends_with('\n') {
                            out.push('\n');
                        }
                        out.push_str(&format!("{}: {}\n", k, v));
                    }
                }
                for (k, v) in cfg.iter() {
                    // A key the scripted answer already carries is REPLACED,
                    // not appended: a drifted `onboot: 0` that the deploy has
                    // since corrected must not still be readable.
                    out = out
                        .lines()
                        .filter(|l| !l.starts_with(&format!("{}:", k)))
                        .collect::<Vec<_>>()
                        .join("\n");
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str(&format!("{}: {}\n", k, v));
                }
                return Ok(CmdOutput::ok(&out));
            }
        }
        {
            let always = self.always.lock().unwrap();
            if let Some((_, out)) = always.iter().find(|(m, _)| rendered.contains(m)) {
                return Ok(out.clone());
            }
        }
        if rendered.contains("sha256sum") {
            let files = self.container_files.lock().unwrap();
            let mut out = String::new();
            for (path, content) in files.iter() {
                if rendered.contains(path.as_str()) {
                    out.push_str(&format!(
                        "{}  {}\n",
                        crate::manifest::sha256_hex(content.as_bytes()),
                        path
                    ));
                }
            }
            if !out.is_empty() {
                return Ok(CmdOutput::ok(&out));
            }
        }
        Ok(CmdOutput::ok(""))
    }

    async fn write_file(&self, path: &str, content: &str, mode: u32) -> Result<(), CoreError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("write_file {} (mode {:o})", path, mode));
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), (content.to_string(), mode));
        Ok(())
    }

    async fn read_file(&self, path: &str) -> Result<String, CoreError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .map(|(c, _)| c.clone())
            .ok_or_else(|| CoreError::State(format!("no such file: {}", path)))
    }

    async fn sleep_ms(&self, _ms: u64) {}
}
