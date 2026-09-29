//! The Executor trait (AR2): every side effect in the system flows through
//! here. Production implements it with real processes and files (in the host
//! crate); tests use [`MockExecutor`] to script responses and record calls.

use async_trait::async_trait;

use crate::error::CoreError;

/// The test double, kept at its old path (mock-executor-weak-assertions,
/// 2026-09-27: it now lives in `crate::mock` and only in test builds).
#[cfg(any(test, feature = "test-support"))]
pub use crate::mock::MockExecutor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    pub timeout_s: u64,
    /// fix-39: the output is a secret. The tracing executor logs the command
    /// but never its output, so the value reaches no transcript, journal or
    /// incident bundle.
    pub quiet: bool,
}

impl Cmd {
    pub fn new(program: &str, args: &[&str], timeout_s: u64) -> Self {
        Self {
            program: program.to_string(),
            args: args.iter().map(|s| s.to_string()).collect(),
            timeout_s,
            quiet: false,
        }
    }

    /// Mark the output as secret (fix-39): traced as a command, never echoed.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    pub fn rendered(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }

    /// The command as a shell would have to be given it: every argument that
    /// is not a plain word is single-quoted.
    ///
    /// fix-38: the transcript's `[run ]` lines, and so an incident's
    /// `commands.sh`, used `rendered()`, which joins arguments with spaces.
    /// A `sh -c` script argument then came back as loose words, and replaying
    /// `pct exec 110 -- sh -c cd '/opt/x' && docker compose up -d` ran the
    /// second half on the host.
    pub fn shell_line(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .map(shell_word)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn shell_word(a: &str) -> String {
    let plain = !a.is_empty()
        && a.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./:=@%+,-".contains(c));
    if plain {
        a.to_string()
    } else {
        shq(a)
    }
}

/// Single-quote a string for `sh -c`, escaping any quote it contains.
///
/// The one quoting helper (shell-strings-quoting, expert panel 2026-09-27):
/// there were three copies, and every value that enters a script goes
/// through this one rather than between bare `'{}'` quotes.
pub fn shq(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
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

    /// Read a host-local file; `Err(CoreError::NotFound)` when it does not
    /// exist and any other error when it exists but cannot be read (fix-50).
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

/// fix-30: the longest command-output line the transcript carries.
///
/// `base64 -w0` answers with the whole binary on ONE line: 40 MB for kyu.
/// Echoed whole, that line became a single 40 MB message on the link, the
/// client's websocket (16 MiB frame limit) dropped the connection, and
/// `release-update-native` reported "connection closed before RPC completed"
/// for installs the host finished successfully (2026-09-27, almanac and
/// kyu). No reader needs more than the start of a line to know what it was.
pub const TRACE_LINE_MAX: usize = 300;

/// fix-39, second layer, widened by fix-56 (expert panel,
/// secret-mask-too-narrow, 2026-09-27): the one masker every transcript line
/// passes through, ported from the house filter
/// `dev-procedure/hooks/mask-secrets.sed`.
///
/// `Cmd::quiet` keeps known secret reads out entirely; this catches the paths
/// nobody marked, such as `docker inspect` printing a container's environment
/// on 2026-09-01. The fix-39 version knew one shape, upper-case `NAME=value`
/// unquoted; measured against it, `KYU_TOKEN="abc"`, `export API_KEY='abc'`,
/// `password=abc`, YAML `KEY: v`, JSON, `postgres://u:p@`,
/// `Authorization: Bearer` and `?api_key=` all passed in plain text. Now:
///
/// - a name holding TOKEN, SECRET, KEY, PASS, BEARER or CREDENTIAL (any case)
///   followed by `=` or `:` loses its value, quoted or not, in env, shell,
///   TOML, YAML, JSON and query-string shapes;
/// - `scheme://user:password@host` loses the password;
/// - `Bearer <token>` loses the token.
///
/// Left alone on purpose: an empty value (a grep pattern like
/// `'^REGISTRY_TOKEN='`), a value that is a shell reference (`$VAR`,
/// `$(cat f)`: it names where the secret is, and a replayed command needs it),
/// and names ending in `_FILE`, `_PATH` or `_DIR`, which hold a path.
pub fn mask_secrets(l: &str) -> String {
    mask_bearer(&mask_url_passwords(&mask_named_values(l)))
}

const REDACTED: &str = "<redacted>";

fn is_name_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'-'
}

fn names_a_secret(name: &str) -> bool {
    const MARKERS: [&str; 6] = ["token", "secret", "key", "pass", "bearer", "credential"];
    const PATHS: [&str; 6] = ["_file", "-file", "_path", "-path", "_dir", "-dir"];
    let lower = name.to_ascii_lowercase();
    MARKERS.iter().any(|m| lower.contains(m)) && !PATHS.iter().any(|p| lower.ends_with(p))
}

/// A value that is a shell reference or already masked is not a secret.
fn keeps_its_value(v: &str) -> bool {
    v.is_empty() || v.starts_with('$') || v.starts_with(REDACTED)
}

fn mask_named_values(l: &str) -> String {
    let bytes = l.as_bytes();
    let mut out = String::with_capacity(l.len());
    let mut i = 0;
    while i < l.len() {
        let at_boundary = i == 0 || !is_name_byte(bytes[i - 1]);
        if !(at_boundary && is_name_byte(bytes[i])) {
            let ch = l[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let mut j = i;
        while j < l.len() && is_name_byte(bytes[j]) {
            j += 1;
        }
        let name = &l[i..j];
        if !names_a_secret(name) {
            out.push_str(name);
            i = j;
            continue;
        }
        // A JSON key or quoted YAML key: step over its closing quote.
        let mut k = j;
        if i > 0
            && (bytes[i - 1] == b'"' || bytes[i - 1] == b'\'')
            && k < l.len()
            && bytes[k] == bytes[i - 1]
        {
            k += 1;
        }
        while k < l.len() && (bytes[k] == b' ' || bytes[k] == b'\t') {
            k += 1;
        }
        let is_sep = k < l.len()
            && (bytes[k] == b'=' || bytes[k] == b':')
            && !(k + 1 < l.len() && bytes[k + 1] == bytes[k]);
        if !is_sep {
            out.push_str(name);
            i = j;
            continue;
        }
        let mut v = k + 1;
        while v < l.len() && (bytes[v] == b' ' || bytes[v] == b'\t') {
            v += 1;
        }
        if v < l.len() && (bytes[v] == b'"' || bytes[v] == b'\'') {
            let q = bytes[v];
            let mut e = v + 1;
            while e < l.len() && bytes[e] != q {
                if bytes[e] == b'\\' && q == b'"' {
                    e += 1;
                }
                e += 1;
            }
            // No closing quote: this quote closes an enclosing string (a grep
            // pattern ending in `NAME='`), so there is no value here.
            if e >= l.len() || keeps_its_value(&l[v + 1..e]) {
                out.push_str(name);
                i = j;
                continue;
            }
            out.push_str(&l[i..=v]);
            out.push_str(REDACTED);
            i = e;
            continue;
        }
        let mut e = v;
        while e < l.len() && !bytes[e].is_ascii_whitespace() && !b"\"'&;".contains(&bytes[e]) {
            e += 1;
        }
        if keeps_its_value(&l[v..e]) {
            out.push_str(name);
            i = j;
            continue;
        }
        out.push_str(&l[i..v]);
        out.push_str(REDACTED);
        i = e;
    }
    out
}

fn mask_url_passwords(l: &str) -> String {
    let mut out = String::with_capacity(l.len());
    let mut rest = l;
    while let Some(at) = rest.find("://") {
        let (head, tail) = rest.split_at(at + 3);
        out.push_str(head);
        let authority_end = tail
            .find(|c: char| {
                c == '/' || c == '?' || c == '#' || c == '"' || c == '\'' || c.is_whitespace()
            })
            .unwrap_or(tail.len());
        let authority = &tail[..authority_end];
        match (authority.rfind('@'), authority.find(':')) {
            (Some(amp), Some(colon))
                if colon < amp && !keeps_its_value(&authority[colon + 1..amp]) =>
            {
                out.push_str(&authority[..=colon]);
                out.push_str(REDACTED);
                out.push_str(&authority[amp..]);
            }
            _ => out.push_str(authority),
        }
        rest = &tail[authority_end..];
    }
    out.push_str(rest);
    out
}

fn mask_bearer(l: &str) -> String {
    let lower = l.to_ascii_lowercase();
    let bytes = l.as_bytes();
    let mut out = String::with_capacity(l.len());
    let mut i = 0;
    while let Some(off) = lower[i..].find("bearer") {
        let b = i + off;
        let after = b + "bearer".len();
        let boundary = b == 0 || !bytes[b - 1].is_ascii_alphanumeric();
        let mut t = after;
        while t < l.len() && bytes[t] == b' ' {
            t += 1;
        }
        let mut e = t;
        while e < l.len()
            && !bytes[e].is_ascii_whitespace()
            && bytes[e] != b'"'
            && bytes[e] != b'\''
        {
            e += 1;
        }
        if boundary && t > after && !keeps_its_value(&l[t..e]) {
            out.push_str(&l[i..t]);
            out.push_str(REDACTED);
            i = e;
        } else {
            out.push_str(&l[i..after]);
            i = after;
        }
    }
    out.push_str(&l[i..]);
    out
}

/// A line as the transcript shows it: whole when short, else its start and
/// the size of what was left out.
pub fn trace_line(l: &str) -> String {
    let masked = mask_secrets(l);
    let l = masked.as_str();
    if l.len() <= TRACE_LINE_MAX {
        return l.to_string();
    }
    let mut cut = TRACE_LINE_MAX;
    while !l.is_char_boundary(cut) {
        cut -= 1;
    }
    format!(
        "{}… ({} bytes, truncated in the transcript)",
        &l[..cut],
        l.len()
    )
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
        // fix-56: the command line is masked like its output; a secret passed
        // as an argument reached the transcript, journal and bundle whole.
        self.line(format!("[run ] {}", mask_secrets(&cmd.shell_line())));
        let out = self.inner.run(cmd).await?;
        if cmd.quiet {
            self.line(format!(
                "  (output withheld: {} bytes, secret)",
                out.stdout.len()
            ));
            return Ok(out);
        }
        for l in out.stdout.lines().chain(out.stderr.lines()).take(20) {
            if !l.trim().is_empty() {
                self.line(format!("  {}", trace_line(l)));
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

/// Read a secret inside an LXC via `pct exec`: like `pct_sh`, but the output
/// never reaches the transcript (fix-39).
pub async fn pct_sh_secret(
    exec: &dyn Executor,
    vmid: u16,
    script: &str,
    timeout_s: u64,
) -> Result<CmdOutput, CoreError> {
    let vm = vmid.to_string();
    let limit = timeout_s.to_string();
    exec.run(
        &Cmd::new(
            "pct",
            &[
                "exec", &vm, "--", "timeout", "-k", "10", &limit, "sh", "-c", script,
            ],
            timeout_s,
        )
        .quiet(),
    )
    .await
}

/// The PATH a read-only probe gets inside the container: the Debian default,
/// set explicitly because [`attach_cmd`] clears the host's environment.
pub const ATTACH_PATH: &str = "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";

/// Where Proxmox keeps a container's configuration (pmxcfs).
pub fn lxc_conf_path(vmid: u16) -> String {
    format!("/etc/pve/lxc/{}.conf", vmid)
}

/// A READ-ONLY command inside a running container, through `lxc-attach`
/// directly instead of `pct exec`.
///
/// Measured on pve 2026-09-29: `pct exec 109 -- true` takes 0.45-0.48 s every
/// time, `lxc-attach -n 109 -- true` 0.01 s. The difference is pct's Perl
/// start-up (loading the PVE modules), paid once per call per container, and
/// the long reads (check, doctor, today) made some eighty such calls. `pct
/// exec` itself ends in the same `lxc-attach`.
///
/// Read-only probes only: every path that changes a container stays on `pct`,
/// whose locking and checks it relies on. A stopped container fails here as
/// it failed under `pct exec` (non-zero, nothing on stdout), so a probe that
/// never ran is still no fact. The environment is cleared and PATH set, so
/// what the probe sees does not depend on the daemon's own environment.
pub fn attach_cmd(vmid: u16, argv: &[&str], timeout_s: u64) -> Cmd {
    let vm = vmid.to_string();
    let mut args: Vec<&str> = vec!["-n", &vm, "--clear-env", "--set-var", ATTACH_PATH, "--"];
    args.extend_from_slice(argv);
    Cmd::new("lxc-attach", &args, timeout_s)
}

/// [`attach_cmd`] running `sh -c <script>`.
pub fn attach_sh(vmid: u16, script: &str, timeout_s: u64) -> Cmd {
    attach_cmd(vmid, &["sh", "-c", script], timeout_s)
}

/// The main section of a container's configuration file: everything before
/// the first `[section]` (a snapshot or `[pve:pending]`). That is what `pct
/// config` prints without `--pending`, so parsing it gives the same current
/// values.
pub fn lxc_conf_current(conf: &str) -> &str {
    let mut end = conf.len();
    let mut at = 0;
    for line in conf.split_inclusive('\n') {
        if line.trim_start().starts_with('[') {
            end = at;
            break;
        }
        at += line.len();
    }
    &conf[..end]
}

/// Run a shell script inside an LXC via `pct exec`.
///
/// fix-53 (expert panel, timeout-leaves-container-work-running, 2026-09-27):
/// the script runs under the container's own `timeout` with the same limit.
/// The host's timeout only ends the wait for `pct`; without this a timed-out
/// `docker compose pull` kept running inside the container next to whatever
/// the step did next. GNU `timeout` signals its whole process group, so the
/// script's children go too, and `-k 10` follows a TERM that is ignored with
/// a KILL. Measured 2026-09-27: every running container has GNU coreutils
/// `timeout` (9.1 or 9.7).
pub async fn pct_sh(
    exec: &dyn Executor,
    vmid: u16,
    script: &str,
    timeout_s: u64,
) -> Result<CmdOutput, CoreError> {
    let vm = vmid.to_string();
    let limit = timeout_s.to_string();
    exec.run(&Cmd::new(
        "pct",
        &[
            "exec", &vm, "--", "timeout", "-k", "10", &limit, "sh", "-c", script,
        ],
        timeout_s,
    ))
    .await
}
