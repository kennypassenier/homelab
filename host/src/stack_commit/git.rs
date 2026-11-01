//! key-move: running git for the host's stacks clone — the environment no
//! workstation or root setting may change, the deploy key handed to ssh, and
//! a deadline on every command so a hung remote cannot hold the lock.
//!
//! A plain `std::process::Command`, not `Executor`: these commands run on a
//! blocking thread beside the operation queue, and the file text they write
//! is not a line anything should trace.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use homelab_core::ops::stackcommit::{StackCommitConfig, is_local};

/// How often a running command is looked at while its deadline runs.
const POLL: Duration = Duration::from_millis(25);
/// The most lines of git's own error text one refusal carries.
const ERROR_LINES: usize = 12;

pub(crate) struct Git<'a> {
    cfg: &'a StackCommitConfig,
    pub(crate) dir: PathBuf,
}

impl<'a> Git<'a> {
    pub(crate) fn new(cfg: &'a StackCommitConfig) -> Self {
        Git {
            cfg,
            dir: PathBuf::from(&cfg.dir),
        }
    }

    fn command(&self, dir: Option<&Path>) -> Command {
        let mut c = Command::new("git");
        if let Some(d) = dir {
            c.arg("-C").arg(d);
        }
        c.env_remove("GIT_DIR")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_WORK_TREE")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("LC_ALL", "C")
            .env("GIT_AUTHOR_NAME", &self.cfg.author_name)
            .env("GIT_AUTHOR_EMAIL", &self.cfg.author_email)
            .env("GIT_COMMITTER_NAME", &self.cfg.author_name)
            .env("GIT_COMMITTER_EMAIL", &self.cfg.author_email)
            // No root or system setting may change what the host writes.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        if !is_local(&self.cfg.remote) {
            c.env(
                "GIT_SSH_COMMAND",
                format!(
                    "ssh -i {} -o IdentitiesOnly=yes -o BatchMode=yes -o UserKnownHostsFile={} \
                     -o StrictHostKeyChecking=yes -o ConnectTimeout=20",
                    shell_quote(&self.cfg.key),
                    shell_quote(&self.cfg.known_hosts),
                ),
            );
        }
        c
    }

    /// Run git in `dir` (None: no `-C`), answering stdout, or git's own
    /// error text (its first lines) when it fails or outlives the deadline.
    pub(crate) fn run_in(&self, dir: Option<&Path>, args: &[&str]) -> Result<String, String> {
        let mut c = self.command(dir);
        c.args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let what = args.first().copied().unwrap_or("git");
        let (ok, out, err) = run_with_deadline(c, Duration::from_secs(self.cfg.timeout_s))
            .map_err(|e| format!("git {what}: {e}"))?;
        if ok {
            return Ok(out);
        }
        let text = if err.trim().is_empty() { out } else { err };
        Err(format!("git {what}: {}", first_lines(&text)))
    }

    /// Run git in the clone.
    pub(crate) fn run(&self, args: &[&str]) -> Result<String, String> {
        self.run_in(Some(self.dir.as_path()), args)
    }

    /// `rev-parse <rev>`, trimmed.
    pub(crate) fn rev(&self, rev: &str) -> Result<String, String> {
        self.run(&["rev-parse", "--verify", "--quiet", rev])
            .map(|s| s.trim().to_string())
    }
}

/// Spawn `c`, read both pipes on their own threads (a full pipe would stall
/// the child), and kill it when `limit` passes.
fn run_with_deadline(mut c: Command, limit: Duration) -> Result<(bool, String, String), String> {
    let mut child = c.spawn().map_err(|e| format!("did not start: {e}"))?;
    let out = reader(child.stdout.take());
    let err = reader(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if started.elapsed() >= limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "did not finish within {} s and was stopped (stack_commits.timeout_s)",
                    limit.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(POLL),
            Err(e) => return Err(format!("could not be waited for: {e}")),
        }
    };
    let joined = |h: std::thread::JoinHandle<String>| h.join().unwrap_or_default();
    Ok((status.success(), joined(out), joined(err)))
}

fn reader<R: std::io::Read + Send + 'static>(pipe: Option<R>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        String::from_utf8_lossy(&buf).into_owned()
    })
}

fn first_lines(s: &str) -> String {
    s.lines()
        .take(ERROR_LINES)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
