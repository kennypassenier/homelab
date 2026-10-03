//! drive-reach (review L3): one `homelab ui` step from the line to the
//! answer, apart from the process around it, so it can be tested: the
//! `state` read, the catalog check (or why it was skipped), the steps sent
//! in order and what the process ends with.
//!
//! `main.rs` gives it the host line ([`Line`]) and a catalog cache
//! ([`Cache`]); it answers what to print and the exit code ([`Driven`]).

use homelab_core::drivecatalog::{Catalog, SCHEMA, hash_text};
use homelab_proto::UiStep;

use crate::ui_catalog::{Screen, check};

/// The host's answer to one step: `RpcResponse`'s `ok` and `message`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub ok: bool,
    pub message: String,
}

/// The line to the host: one step out, its answer back; `None` when the
/// host closed the line before it answered.
pub trait Line {
    fn send(&mut self, step: UiStep) -> impl std::future::Future<Output = Option<Reply>>;
}

/// Where the client keeps the catalogs it fetched, by hash.
pub trait Cache {
    fn get(&self, hash: &str) -> Option<String>;
    fn put(&self, hash: &str, text: &str);
}

/// How the line ends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum End {
    /// Print this answer as `ui_print` does (its own exit code).
    Print(Reply),
    /// Refused here: print this, exit 1. Nothing reached the screen.
    Refused(String),
    /// The host closed the line before it answered: exit 1.
    HostGone,
}

/// What `drive` did: the notes for the driver (stderr, first), then the end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Driven {
    pub notes: Vec<String>,
    pub end: End,
}

/// The steps whose names are checked before anything is sent.
fn checked(step: &UiStep) -> bool {
    matches!(
        step,
        UiStep::Click { .. }
            | UiStep::Goto { .. }
            | UiStep::Plan { .. }
            | UiStep::Press { .. }
            | UiStep::Type { .. }
            | UiStep::Edit { .. }
            | UiStep::Pick { .. }
            | UiStep::Check { .. }
    )
}

/// The catalog a `state` answer names, read: `Ok(None)` from a dashboard
/// that names none (older than the catalog: unchecked, review H2);
/// `Err` when it names one this client cannot read (a refusal).
pub async fn catalog_for<L: Line, C: Cache>(
    line: &mut L,
    state: &serde_json::Value,
    cache: &C,
) -> Result<Option<Catalog>, String> {
    let Some(hash) = state["catalog_hash"].as_str() else {
        return Ok(None);
    };
    let unreadable = |why: String| {
        format!(
            "refused here, nothing was sent to the dashboard: the dashboard names a control catalog ({}) but it could not be read: {why}",
            &hash[..hash.len().min(12)]
        )
    };
    let schema = state["catalog_schema"].as_u64().unwrap_or(0);
    if schema != u64::from(SCHEMA) {
        return Err(unreadable(format!(
            "it is catalog schema {schema}, this client reads schema {SCHEMA}; update the client (`homelab self-install` from the release the dashboard runs)"
        )));
    }
    let read = |text: &str| -> Result<Catalog, String> {
        if hash_text(text) != hash {
            return Err("its text does not match the hash the dashboard named".into());
        }
        let c = Catalog::parse(text)?;
        if c.schema != SCHEMA {
            return Err(format!("its text is schema {}", c.schema));
        }
        Ok(c)
    };
    if let Some(c) = cache.get(hash).and_then(|t| read(&t).ok()) {
        return Ok(Some(c));
    }
    let reply = line
        .send(UiStep::Controls)
        .await
        .ok_or_else(|| unreadable("the host closed the line before it answered".into()))?;
    let v: serde_json::Value = serde_json::from_str(&reply.message)
        .map_err(|_| unreadable(format!("the answer was not JSON: {}", reply.message)))?;
    let text = v["text"]
        .as_str()
        .ok_or_else(|| unreadable("the answer carried no catalog".into()))?;
    let c = read(text).map_err(unreadable)?;
    cache.put(hash, text);
    Ok(Some(c))
}

/// One step, checked against the running dashboard's catalog first.
pub async fn drive<L: Line, C: Cache>(line: &mut L, step: UiStep, cache: &C) -> Driven {
    let mut notes = Vec::new();
    if !checked(&step) {
        return send_all(line, vec![step], notes).await;
    }
    let Some(state) = line.send(UiStep::State).await else {
        return Driven {
            notes,
            end: End::HostGone,
        };
    };
    let v: serde_json::Value = serde_json::from_str(&state.message).unwrap_or_default();
    let cat = match catalog_for(line, &v, cache).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            notes.push(
                "this dashboard serves no control catalog (it is older than 3.71.0): the step is sent unchecked"
                    .into(),
            );
            return send_all(line, vec![step], notes).await;
        }
        Err(e) => {
            return Driven {
                notes,
                end: End::Refused(e),
            };
        }
    };
    let screen = Screen::of(&v["state"]);
    let (verb, name) = (step.verb().to_string(), name_of(&step));
    match check(step, &cat, &screen) {
        Ok(c) => {
            notes.extend(c.notes);
            send_all(line, c.steps, notes).await
        }
        Err(e) => {
            // review M4: the dashboard counts it with its own refusals
            // (the verb and the name; never typed text). Best effort.
            let _ = line.send(UiStep::RefusedLocally { verb, name }).await;
            Driven {
                notes,
                end: End::Refused(e),
            }
        }
    }
}

/// The name a step uses, for the refusal log: a control, a field, a
/// button or an address; never typed text.
fn name_of(step: &UiStep) -> String {
    match step {
        UiStep::Click { control, .. } => control.clone(),
        UiStep::Press { button } => button.clone(),
        UiStep::Goto { path } => path.clone(),
        UiStep::Type { field, .. }
        | UiStep::Edit { field, .. }
        | UiStep::Pick { field, .. }
        | UiStep::Check { field, .. } => field.clone(),
        UiStep::Plan { steps } => format!("a plan of {} steps", steps.len()),
        other => other.verb().to_string(),
    }
}

/// The catalog cache on disk: `<dir>/<hash>.json`, at most [`CACHE_KEEP`]
/// files (the oldest go first; nothing balloons).
pub struct FileCache {
    pub dir: std::path::PathBuf,
}

/// How many catalogs the cache keeps.
pub const CACHE_KEEP: usize = 4;

impl FileCache {
    /// `$XDG_CACHE_HOME/homelab/drivecatalog`, else `~/.cache/homelab/drivecatalog`.
    pub fn default_dir() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .filter(|v| !v.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".cache"))
            })?;
        Some(base.join("homelab").join("drivecatalog"))
    }

    fn path(&self, hash: &str) -> Option<std::path::PathBuf> {
        hash.chars()
            .all(|c| c.is_ascii_hexdigit())
            .then(|| self.dir.join(format!("{hash}.json")))
    }
}

impl Cache for FileCache {
    fn get(&self, hash: &str) -> Option<String> {
        std::fs::read_to_string(self.path(hash)?).ok()
    }

    fn put(&self, hash: &str, text: &str) {
        let Some(p) = self.path(hash) else { return };
        if std::fs::create_dir_all(&self.dir).is_err() || std::fs::write(&p, text).is_err() {
            return;
        }
        let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> =
            std::fs::read_dir(&self.dir)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
                .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
                .collect();
        files.sort();
        while files.len() > CACHE_KEEP {
            let (_, old) = files.remove(0);
            let _ = std::fs::remove_file(old);
        }
    }
}

/// Send `steps` in order; the first refused one ends the line with its
/// answer, else the last one's answer is printed.
async fn send_all<L: Line>(line: &mut L, mut steps: Vec<UiStep>, notes: Vec<String>) -> Driven {
    let last = steps.pop().unwrap_or(UiStep::State);
    for step in steps {
        let Some(reply) = line.send(step).await else {
            return Driven {
                notes,
                end: End::HostGone,
            };
        };
        let refused = serde_json::from_str::<serde_json::Value>(&reply.message)
            .map(|v| v["ok"] != serde_json::Value::Bool(true))
            .unwrap_or(true);
        if refused {
            return Driven {
                notes,
                end: End::Print(reply),
            };
        }
    }
    match line.send(last).await {
        Some(reply) => Driven {
            notes,
            end: End::Print(reply),
        },
        None => Driven {
            notes,
            end: End::HostGone,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};

    /// A host line that answers from a script and records what was sent.
    #[derive(Default)]
    struct Mock {
        sent: Vec<UiStep>,
        answers: VecDeque<Option<Reply>>,
    }

    impl Mock {
        fn with(answers: Vec<Option<serde_json::Value>>) -> Mock {
            Mock {
                sent: Vec::new(),
                answers: answers
                    .into_iter()
                    .map(|a| {
                        a.map(|v| Reply {
                            ok: true,
                            message: v.to_string(),
                        })
                    })
                    .collect(),
            }
        }
        fn verbs(&self) -> Vec<&'static str> {
            self.sent.iter().map(UiStep::verb).collect()
        }
    }

    impl Line for Mock {
        async fn send(&mut self, step: UiStep) -> Option<Reply> {
            self.sent.push(step);
            self.answers.pop_front().unwrap_or(None)
        }
    }

    #[derive(Default)]
    struct MemCache(RefCell<HashMap<String, String>>);

    impl Cache for MemCache {
        fn get(&self, hash: &str) -> Option<String> {
            self.0.borrow().get(hash).cloned()
        }
        fn put(&self, hash: &str, text: &str) {
            self.0.borrow_mut().insert(hash.into(), text.into());
        }
    }

    fn catalog_text() -> String {
        serde_json::json!({
            "schema": homelab_core::drivecatalog::SCHEMA,
            "controls": [
                {"id": "schedule-menu", "page": "schedules", "what": "open one schedule's menu",
                 "opens": "dialog", "row": "<schedule id>", "href": "/activity?view=planned",
                 "was": [{"id": "edit-schedule", "press": "edit"}]},
                {"id": "new-schedule", "page": "schedules", "what": "open the New schedule drawer",
                 "opens": "dialog", "href": "/activity?view=planned", "was": []}
            ],
            "fields": [
                {"id": "shell-target", "page": "shell", "what": "the container a command runs in",
                 "was": ["shell-vmid"]},
                {"id": "key", "page": "settings", "what": "one host.toml key", "row": "<key>"}
            ],
            "addresses": ["", "apps", "inbox", "status", "stacks"],
            "redirects": {"status": "/inbox", "stacks/{stack}/checks": "/stacks/{stack}"},
            "stack_slot": "{stack}",
            "stack_tabs": ["overview", "logs", "settings", "checks"]
        })
        .to_string()
    }

    /// A `state` answer naming the catalog by its hash, with a screen.
    fn state(screen: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            "state": screen,
            "catalog_hash": homelab_core::drivecatalog::hash_text(&catalog_text()),
            "catalog_schema": homelab_core::drivecatalog::SCHEMA,
        })
    }

    fn controls_answer() -> serde_json::Value {
        serde_json::json!({
            "ok": true,
            "hash": homelab_core::drivecatalog::hash_text(&catalog_text()),
            "schema": homelab_core::drivecatalog::SCHEMA,
            "text": catalog_text(),
        })
    }

    fn ok() -> serde_json::Value {
        serde_json::json!({"ok": true, "state": {}})
    }

    fn click(c: &str, row: Option<&str>) -> UiStep {
        UiStep::Click {
            control: c.into(),
            row: row.map(str::to_string),
        }
    }

    fn run<L: Line, C: Cache>(line: &mut L, step: UiStep, cache: &C) -> Driven {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(drive(line, step, cache))
    }

    /// covers: drive-reach review L3/M3. The state is read, the catalog
    /// fetched once by its hash and kept; the second click reads it from
    /// the cache (no `controls` step), and the click goes out.
    #[test]
    fn drive_reach_the_catalog_is_fetched_by_hash_once_and_then_kept() {
        let cache = MemCache::default();
        let mut line = Mock::with(vec![
            Some(state(serde_json::json!({}))),
            Some(controls_answer()),
            Some(ok()),
        ]);
        let d = run(&mut line, click("new-schedule", None), &cache);
        assert_eq!(
            d.end,
            End::Print(Reply {
                ok: true,
                message: ok().to_string()
            })
        );
        assert_eq!(line.verbs(), ["state", "controls", "click"]);
        let mut again = Mock::with(vec![Some(state(serde_json::json!({}))), Some(ok())]);
        run(&mut again, click("new-schedule", None), &cache);
        assert_eq!(
            again.verbs(),
            ["state", "click"],
            "the cached catalog is used"
        );
    }

    /// covers: drive-reach review H2. An older dashboard (no catalog named
    /// in its state) is driven unchecked, and the driver is told so.
    #[test]
    fn drive_reach_an_older_dashboard_is_driven_unchecked_with_a_note() {
        let mut line = Mock::with(vec![
            Some(serde_json::json!({"ok": true, "state": {}})),
            Some(ok()),
        ]);
        let d = run(&mut line, click("whatever", None), &MemCache::default());
        assert_eq!(line.verbs(), ["state", "click"]);
        assert!(matches!(d.end, End::Print(_)));
        assert!(
            d.notes.iter().any(|n| n.contains("sent unchecked")),
            "{:?}",
            d.notes
        );
    }

    /// covers: drive-reach review H2. A catalog the state names but that
    /// cannot be read (a newer schema, a broken answer, a hash that does
    /// not match) refuses the step here: nothing reaches the screen.
    #[test]
    fn drive_reach_a_named_catalog_that_does_not_read_refuses_locally() {
        // A schema this client does not read.
        let mut s = state(serde_json::json!({}));
        s["catalog_schema"] = serde_json::json!(99);
        let mut line = Mock::with(vec![Some(s), Some(ok())]);
        let d = run(&mut line, click("new-schedule", None), &MemCache::default());
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("schema")),
            "{d:?}"
        );
        assert!(!line.verbs().contains(&"click"), "{:?}", line.verbs());
        // A catalog answer that is not the one the hash names.
        let mut bad = controls_answer();
        bad["text"] = serde_json::json!("{\"controls\": [}");
        let mut line = Mock::with(vec![
            Some(state(serde_json::json!({}))),
            Some(bad),
            Some(ok()),
        ]);
        let d = run(&mut line, click("new-schedule", None), &MemCache::default());
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("could not be read")),
            "{d:?}"
        );
        assert!(!line.verbs().contains(&"click"), "{:?}", line.verbs());
    }

    /// covers: drive-reach review M4/L3. An unknown name is refused here
    /// (exit 1), and the refusal is reported to the dashboard's log as the
    /// verb and the name only.
    #[test]
    fn drive_reach_a_local_refusal_is_reported_and_ends_with_exit_one() {
        let mut line = Mock::with(vec![
            Some(state(serde_json::json!({}))),
            Some(controls_answer()),
            Some(serde_json::json!({"ok": true})),
        ]);
        let d = run(&mut line, click("new-schedul", None), &MemCache::default());
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("nothing was sent")),
            "{d:?}"
        );
        assert_eq!(line.verbs(), ["state", "controls", "refused_locally"]);
        assert_eq!(
            line.sent[2],
            UiStep::RefusedLocally {
                verb: "click".into(),
                name: "new-schedul".into()
            }
        );
    }

    /// covers: drive-reach review L3. An old name is sent as the click and
    /// the press, in order; a refused first step ends the line with its
    /// answer and the press is never sent.
    #[test]
    fn drive_reach_the_chained_send_stops_at_the_first_refused_step() {
        let refused = serde_json::json!({"ok": false, "refusal": {"why": "no row s9"}});
        let mut line = Mock::with(vec![
            Some(state(serde_json::json!({}))),
            Some(controls_answer()),
            Some(refused.clone()),
            Some(ok()),
        ]);
        let d = run(
            &mut line,
            click("edit-schedule", Some("s9")),
            &MemCache::default(),
        );
        assert_eq!(line.verbs(), ["state", "controls", "click"]);
        assert_eq!(
            d.end,
            End::Print(Reply {
                ok: true,
                message: refused.to_string()
            })
        );
        assert!(d.notes[0].contains("edit-schedule is an item of schedule-menu"));
        // The host gone mid-line.
        let mut gone = Mock::with(vec![None]);
        let d = run(&mut gone, click("new-schedule", None), &MemCache::default());
        assert_eq!(d.end, End::HostGone);
    }

    /// covers: drive-reach review H3. Inside a page-level dialog a button is
    /// checked against what that dialog offers (its names and labels), not
    /// against the page catalog: a label the dashboard allows is sent, a
    /// button the dialog lacks is refused here.
    #[test]
    fn drive_reach_inside_a_page_dialog_its_own_buttons_decide() {
        let screen = serde_json::json!({"form": null, "page_dialog": {
            "control": "schedule-menu", "title": "Schedule s1",
            "controls": [{"id": "edit", "label": "Edit"}, {"id": "", "label": "Run now"}],
            "fields": ["sched-at"]
        }});
        let mut line = Mock::with(vec![
            Some(state(screen.clone())),
            Some(controls_answer()),
            Some(ok()),
        ]);
        let cache = MemCache::default();
        let d = run(&mut line, click("Run now", None), &cache);
        assert!(matches!(d.end, End::Print(_)), "{d:?}");
        assert_eq!(line.verbs(), ["state", "controls", "click"]);
        let mut line = Mock::with(vec![
            Some(state(screen.clone())),
            Some(serde_json::json!({"ok": true})),
        ]);
        let d = run(
            &mut line,
            UiStep::Press {
                button: "delete".into(),
            },
            &cache,
        );
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("no control delete in Schedule s1")),
            "{d:?}"
        );
        let mut line = Mock::with(vec![
            Some(state(screen)),
            Some(serde_json::json!({"ok": true})),
        ]);
        let d = run(
            &mut line,
            UiStep::Type {
                field: "sched-when".into(),
                text: "x".into(),
            },
            &cache,
        );
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("has no field sched-when")),
            "{d:?}"
        );
    }

    /// covers: drive-reach review M5. A page field is checked like a click:
    /// an unknown one is refused with the closest, an old id rewritten, a
    /// field per row accepted by its row.
    #[test]
    fn drive_reach_page_fields_are_checked_against_the_catalog() {
        let cache = MemCache::default();
        let mut line = Mock::with(vec![
            Some(state(serde_json::json!({}))),
            Some(controls_answer()),
            Some(serde_json::json!({"ok": true})),
        ]);
        let d = run(
            &mut line,
            UiStep::Type {
                field: "shel-target".into(),
                text: "secret-text".into(),
            },
            &cache,
        );
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("no page declares a field shel-target")),
            "{d:?}"
        );
        assert_eq!(
            line.sent.last(),
            Some(&UiStep::RefusedLocally {
                verb: "type".into(),
                name: "shel-target".into()
            })
        );
        let mut line = Mock::with(vec![Some(state(serde_json::json!({}))), Some(ok())]);
        let d = run(
            &mut line,
            UiStep::Pick {
                field: "shell-vmid".into(),
                value: "104".into(),
            },
            &cache,
        );
        assert_eq!(
            line.sent[1],
            UiStep::Pick {
                field: "shell-target".into(),
                value: "104".into()
            }
        );
        assert!(
            d.notes[0].contains("shell-vmid is called shell-target"),
            "{:?}",
            d.notes
        );
        let mut line = Mock::with(vec![Some(state(serde_json::json!({}))), Some(ok())]);
        run(
            &mut line,
            UiStep::Type {
                field: "key-ask-timeout".into(),
                text: "600".into(),
            },
            &cache,
        );
        assert_eq!(line.verbs(), ["state", "type"]);
    }

    /// The cache on disk keeps a catalog by its hash and at most
    /// CACHE_KEEP of them.
    #[test]
    fn drive_reach_the_file_cache_is_bounded() {
        let dir = std::env::temp_dir().join(format!("ui-drive-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = FileCache { dir: dir.clone() };
        for i in 0..(CACHE_KEEP + 3) {
            c.put(&format!("{i:064x}"), "{}");
        }
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), CACHE_KEEP);
        assert!(c.get("../etc/passwd").is_none(), "a hash is hex only");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// covers: drive-reach review L1. `/stacks/…` is checked against the
    /// router's shape: a tab the hub never had is refused; a retired tab
    /// says where it lands.
    #[test]
    fn drive_reach_a_stack_address_is_checked_against_the_router() {
        let cache = MemCache::default();
        let mut line = Mock::with(vec![
            Some(state(serde_json::json!({}))),
            Some(controls_answer()),
            Some(serde_json::json!({"ok": true})),
        ]);
        let d = run(
            &mut line,
            UiStep::Goto {
                path: "/stacks/films/nope".into(),
            },
            &cache,
        );
        assert!(
            matches!(&d.end, End::Refused(e) if e.contains("no tab nope")),
            "{d:?}"
        );
        let mut line = Mock::with(vec![Some(state(serde_json::json!({}))), Some(ok())]);
        let d = run(
            &mut line,
            UiStep::Goto {
                path: "/stacks/films/checks".into(),
            },
            &cache,
        );
        assert!(matches!(d.end, End::Print(_)));
        assert!(
            d.notes[0].contains("it is /stacks/films now"),
            "{:?}",
            d.notes
        );
    }
}
