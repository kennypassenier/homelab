//! The scripted test double for [`Executor`].
//!
//! mock-executor-weak-assertions (expert panel, 2026-09-27): compiled only
//! for tests (`cfg(test)` or the `test-support` feature, which the crates'
//! dev-dependencies enable), so it no longer ships inside the host binary.
//! Calls are recorded as program and arguments as well as rendered text, so
//! a test can ask whether a verb ran ([`MockExecutor::ran`]) instead of
//! whether a substring appears; and a scripted rule that never matched is
//! reported ([`MockExecutor::unused_rules`], [`MockExecutor::strict`]).

use std::collections::HashMap;
use std::sync::Mutex;

use async_trait::async_trait;

use crate::error::CoreError;
use crate::executor::{Cmd, CmdOutput, Executor};

/// A scripted answer and whether any command has matched it yet.
struct Rule {
    matcher: String,
    out: CmdOutput,
    used: bool,
}

impl Rule {
    fn new(matcher: &str, out: CmdOutput) -> Self {
        Self {
            matcher: matcher.to_string(),
            out,
            used: false,
        }
    }
}

/// Scripted test double. Default behavior: every command succeeds with empty
/// output; files behave like an in-memory filesystem. Script exceptions with
/// [`MockExecutor::enqueue`] (consumed once, in order, first match wins) or
/// [`MockExecutor::respond_always`].
#[derive(Default)]
pub struct MockExecutor {
    calls: Mutex<Vec<String>>,
    /// The same commands as program and arguments, for [`MockExecutor::ran`].
    structured: Mutex<Vec<Cmd>>,
    /// Set by [`MockExecutor::strict`]: dropping the mock with a rule that
    /// never matched fails the test.
    strict: bool,
    /// Rendered command paired with the timeout it was given. A timeout is
    /// part of whether an operation can succeed at all — a restore that dies
    /// at thirty minutes fails for a reason no argv assertion can see — so it
    /// has to be assertable (deployment project, F38).
    timeouts: Mutex<Vec<(String, u64)>>,
    queue: Mutex<Vec<(String, CmdOutput)>>,
    always: Mutex<Vec<Rule>>,
    files: Mutex<HashMap<String, (String, u32)>>,
    /// What `pct push` has landed inside the container, keyed by destination.
    container_files: Mutex<HashMap<String, String>>,
    /// What `pct set` has changed about the container, keyed by config key.
    container_config: Mutex<HashMap<String, String>>,
    /// What `pct create`/`pct clone` asked for; only fills gaps.
    container_created: Mutex<HashMap<String, String>>,
    /// Which app containers `docker compose up -d` has started.
    container_running: Mutex<std::collections::BTreeSet<String>>,
    /// Paths whose read fails for a reason other than absence (EACCES, EIO).
    read_errors: Mutex<HashMap<String, String>>,
}

impl MockExecutor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A mock that fails the test, when it is dropped, if any scripted rule
    /// never matched a command. A rule that stops matching after a change in
    /// how a command is rendered otherwise turns into "success, empty
    /// output" without a word.
    pub fn strict() -> Self {
        let mut m = Self::default();
        m.strict = true;
        m
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
            .insert(0, Rule::new(matcher, out));
    }

    /// Every command whose rendered form contains `matcher` returns `out`
    /// (unless a queued rule matched first).
    pub fn respond_always(&self, matcher: &str, out: CmdOutput) {
        self.always.lock().unwrap().push(Rule::new(matcher, out));
    }

    /// Matchers of `respond_*` rules no command has matched, in order, then
    /// queued rules still waiting to be consumed.
    pub fn unused_rules(&self) -> Vec<String> {
        let always = self.always.lock().unwrap();
        let queue = self.queue.lock().unwrap();
        always
            .iter()
            .filter(|r| !r.used)
            .map(|r| r.matcher.clone())
            .chain(queue.iter().map(|(m, _)| m.clone()))
            .collect()
    }

    /// How many commands ran `program` with arguments starting with `verb`.
    ///
    /// Read from the recorded program and arguments, and from the words of
    /// any `sh -c` script, so neither a path before the program
    /// (`/usr/sbin/pct`), extra spaces, nor running it through a shell hides
    /// it. Prefer `ran(..) == 0` over `calls_containing(..).is_empty()`: the
    /// substring form passes without testing anything once the rendering
    /// changes.
    pub fn ran(&self, program: &str, verb: &[&str]) -> usize {
        self.structured
            .lock()
            .unwrap()
            .iter()
            .filter(|c| {
                let argv: Vec<&str> = std::iter::once(c.program.as_str())
                    .chain(c.args.iter().map(String::as_str))
                    .collect();
                has_invocation(&argv, program, verb)
                    || c.args.iter().any(|a| {
                        let words = script_words(a);
                        let words: Vec<&str> = words.iter().map(String::as_str).collect();
                        words.len() > 1 && has_invocation(&words, program, verb)
                    })
            })
            .count()
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

    /// Make every read of `path` fail with `why`, as a real read does on
    /// EACCES or EIO: the file is there, it just cannot be read.
    pub fn fail_read(&self, path: &str, why: &str) {
        self.read_errors
            .lock()
            .unwrap()
            .insert(path.to_string(), why.to_string());
    }

    pub fn seed_file(&self, path: &str, content: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), (content.to_string(), 0o644));
    }
}

impl Drop for MockExecutor {
    fn drop(&mut self) {
        if !self.strict || std::thread::panicking() {
            return;
        }
        let unused = self.unused_rules();
        if !unused.is_empty() {
            panic!("scripted rules never matched a command: {unused:?}");
        }
    }
}

/// Does `argv` hold `program` (by its last path segment) followed by `verb`?
fn has_invocation(argv: &[&str], program: &str, verb: &[&str]) -> bool {
    (0..argv.len()).any(|i| {
        argv[i].rsplit('/').next() == Some(program)
            && argv.len() > i + verb.len()
            && argv[i + 1..=i + verb.len()] == *verb
    })
}

/// The words of a shell script, unquoted, with `;`, `&`, `|`, `(` and `)`
/// as separators of their own. Enough of `sh` to read the scripts the
/// operations write; not a shell parser.
fn script_words(script: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    for c in script.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => cur.push(c),
            None if c == '\'' || c == '"' => quote = Some(c),
            None if c.is_whitespace() || ";&|()".contains(c) => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
                if !c.is_whitespace() {
                    words.push(c.to_string());
                }
            }
            None => cur.push(c),
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

/// The directory a script `cd`s into first, quoted or not.
fn cd_target(script: &str) -> Option<String> {
    let words = script_words(script);
    words
        .iter()
        .position(|w| w == "cd")
        .and_then(|i| words.get(i + 1).cloned())
}

#[async_trait]
impl Executor for MockExecutor {
    async fn run(&self, cmd: &Cmd) -> Result<CmdOutput, CoreError> {
        let rendered = cmd.rendered();
        self.calls.lock().unwrap().push(rendered.clone());
        self.structured.lock().unwrap().push(cmd.clone());
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
        // The app directory is the script's `cd` target, read as a shell
        // would (mock-executor-weak-assertions, 2026-09-27: it was the text
        // between the first two quotes, so it depended on deploy.rs quoting).
        if rendered.contains("docker compose")
            && let Some(dir) = cmd.args.iter().find_map(|a| cd_target(a))
            && let Some(app) = dir.rsplit('/').next()
        {
            let mut up = self.container_running.lock().unwrap();
            if rendered.contains("compose up") {
                up.insert(app.to_string());
            } else if rendered.contains("compose down") {
                up.remove(app);
            }
        }
        // fix-132's health read (`docker compose ps --format json`, scoped to
        // one app's directory) needs its own model: it is not the
        // fleet-wide `docker ps --format` query above, and an app the mock
        // never saw a scripted answer for must report itself running once
        // `compose up` has put it in `container_running` — otherwise every
        // verify-health step reads back "no running services" for a
        // container the test never meant to be down.
        if rendered.contains("docker compose ps") && rendered.contains("--format json") {
            let scripted = {
                let always = self.always.lock().unwrap();
                always.iter().any(|r| rendered.contains(&r.matcher))
            } || {
                let queue = self.queue.lock().unwrap();
                queue.iter().any(|(m, _)| rendered.contains(m))
            };
            if !scripted
                && let Some(dir) = cmd.args.iter().find_map(|a| cd_target(a))
                && let Some(app) = dir.rsplit('/').next()
            {
                let up = self.container_running.lock().unwrap();
                if up.contains(app) {
                    return Ok(CmdOutput::ok(&format!(
                        "{{\"Service\":\"{}\",\"State\":\"running\",\"Health\":\"\"}}\n",
                        app
                    )));
                }
            }
        }
        if rendered.contains("docker ps --format") {
            let up = self.container_running.lock().unwrap();
            if !up.is_empty() {
                let scripted = {
                    let always = self.always.lock().unwrap();
                    always.iter().any(|r| rendered.contains(&r.matcher))
                };
                if !scripted {
                    let mut out: Vec<&str> = up.iter().map(|s| s.as_str()).collect();
                    out.sort();
                    return Ok(CmdOutput::ok(&format!("{}\n", out.join("\n"))));
                }
            }
        }
        if cmd.program == "pct"
            && cmd.args.first().map(|a| a.as_str()) == Some("push")
            && let (Some(src), Some(dest)) = (cmd.args.get(2), cmd.args.get(3))
        {
            let content = self.files.lock().unwrap().get(src).map(|(c, _)| c.clone());
            if let Some(c) = content {
                self.container_files.lock().unwrap().insert(dest.clone(), c);
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
                    let mut always = self.always.lock().unwrap();
                    match always.iter_mut().find(|r| rendered.contains(&r.matcher)) {
                        Some(r) => {
                            r.used = true;
                            r.out.stdout.clone()
                        }
                        None => String::new(),
                    }
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
            let mut always = self.always.lock().unwrap();
            if let Some(r) = always.iter_mut().find(|r| rendered.contains(&r.matcher)) {
                r.used = true;
                return Ok(r.out.clone());
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
        if let Some(why) = self.read_errors.lock().unwrap().get(path) {
            return Err(CoreError::State(format!("{}: {}", path, why)));
        }
        self.files
            .lock()
            .unwrap()
            .get(path)
            .map(|(c, _)| c.clone())
            .ok_or_else(|| CoreError::NotFound(format!("no such file: {}", path)))
    }

    async fn sleep_ms(&self, _ms: u64) {}
}
