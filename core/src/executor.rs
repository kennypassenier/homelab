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
    queue: Mutex<Vec<(String, CmdOutput)>>,
    always: Mutex<Vec<(String, CmdOutput)>>,
    files: Mutex<HashMap<String, (String, u32)>>,
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

    /// Every command whose rendered form contains `matcher` returns `out`
    /// (unless a queued rule matched first).
    pub fn respond_always(&self, matcher: &str, out: CmdOutput) {
        self.always.lock().unwrap().push((matcher.to_string(), out));
    }

    /// Rendered forms of every executed command, in order.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    pub fn calls_containing(&self, needle: &str) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| c.contains(needle))
            .collect()
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
        {
            let mut queue = self.queue.lock().unwrap();
            if let Some(pos) = queue.iter().position(|(m, _)| rendered.contains(m)) {
                return Ok(queue.remove(pos).1);
            }
        }
        {
            let always = self.always.lock().unwrap();
            if let Some((_, out)) = always.iter().find(|(m, _)| rendered.contains(m)) {
                return Ok(out.clone());
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
