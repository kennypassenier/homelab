//! replace-kuma (Kenny, 2026-09-30): the dashboard measures every minute
//! what Uptime Kuma measured. Each tile's own backend address, asked
//! directly (its `probe`, resolved by the client at deploy time,
//! `core::ops::tiles::TileView::probe`); each stack's container, as the
//! host last read it; and the host itself. Five minutes without an answer
//! is a Down notice (urgent: pushed at once), and the return an Up notice.
//! The host watches this dashboard in turn.
//!
//! tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30):
//! `HOMELAB_ADMIN_WATCH_VIA` is gone — measuring a tile through the gateway
//! would reopen what fix-89 closed (see REGISTER.md). A tile with a `probe`
//! is watched, straight to that address; a tile with none is not, the same
//! as one with no reading.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::actions::{Actions, Clock, HostPort, JobState};
use super::actions_notify::NotifyCenter;
use super::actions_state::{read_json, write_json};
use super::host_link::Shared;
use crate::core::notify::{Detail, Draft, Kind, Level};
use crate::core::uptime::{self, Sample, Store};
use crate::core::watch::{Change, DOWN_AFTER_S, Seen, Target, step, step_deploying, view};

/// Fallback cadence, unless the host's `watch_interval_s` says otherwise
/// (decision "default plus per tile", 2026-09-30).
pub const EVERY: Duration = Duration::from_secs(60);

/// How often the loop wakes: the smallest `watch_every` a tile may declare.
pub const TICK: Duration = Duration::from_secs(10);

/// What one round decided to do with a target.
enum Probe {
    /// Asked, with the answer and how long it may fail before it is down
    /// (the fleet default, or a tile's own `down_after`).
    Answer(Result<(), String>, i64),
    /// Skipped: its stack is known to be deploying — a known outage, not a
    /// fault (decision "deploys are known outages", 2026-09-30).
    Deploying,
    /// Skipped: this tile's own `watch_every` has not passed yet
    /// (decision "default plus per tile", 2026-09-30). The previous
    /// reading stands; nothing in `Seen` changes.
    TooSoon,
}

/// The host's answer to `Tiles`, beyond the tile list itself.
#[derive(Default)]
struct TilesAnswer {
    tiles: Vec<serde_json::Value>,
    /// AR12 holds the operation lock one at a time, so at most one stack.
    deploying_stack: Option<String>,
    /// Fleet defaults, from host.toml's `watch_interval_s` /
    /// `watch_down_after_s`; None when the host did not (yet) answer them
    /// (an older host, or the ask failed) — [`EVERY`] / [`DOWN_AFTER_S`]
    /// are the fallback either way.
    watch_interval_s: Option<u64>,
    watch_down_after_s: Option<u64>,
}

/// What the watch holds, for the start page.
#[derive(Default)]
pub struct WatchState {
    pub targets: Vec<Target>,
    pub seen: BTreeMap<String, Seen>,
    /// ask-kp11be-2: what every round measured, as capped hourly cells
    /// (`core::uptime`), loaded from and flushed to `history_file`.
    pub history: Store,
    /// Not yet on disk.
    pub history_dirty: bool,
    pub history_file: Option<PathBuf>,
    /// When the host was last pinged for the history (unix seconds).
    pub last_ping_at: i64,
}

/// How often the history reaches the disk. A power cut loses at most this
/// much of it (the write itself is temp + fsync + rename, never half a file).
pub const FLUSH_EVERY: Duration = Duration::from_secs(600);

/// How often the host is pinged for the Host page's reachability history.
pub const PING_EVERY_S: i64 = 60;

/// Load the history from `file` into `w` (before the watch is shared). A
/// file that does not read is set aside as `.bad` and the history starts
/// empty: it is a measurement log, losing it must not stop the dashboard.
pub fn load_history(w: &mut WatchState, file: PathBuf, now: i64) {
    match read_json::<Store>(&file) {
        Ok(Some(mut s)) => {
            s.trim(now);
            w.history = s;
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!("uptime history not read, starting empty: {e}");
            let _ = std::fs::rename(&file, file.with_extension("json.bad"));
        }
    }
    w.history_file = Some(file);
}

/// Write the history if it changed since the last write.
pub async fn flush_history(w: &Watched) {
    let (file, store) = {
        let mut s = w.lock().await;
        if !s.history_dirty {
            return;
        }
        let Some(f) = s.history_file.clone() else {
            return;
        };
        s.history_dirty = false;
        (f, s.history.clone())
    };
    let mut store = store;
    store.schema_version = 1;
    if let Err(e) = tokio::task::spawn_blocking(move || write_json(&file, &store))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()))
    {
        tracing::warn!("uptime history not written: {e}");
        w.lock().await.history_dirty = true;
    }
}

#[derive(serde::Deserialize)]
struct UptimeQuery {
    #[serde(default)]
    hours: Option<i64>,
    #[serde(default)]
    bucket_s: Option<i64>,
    #[serde(default)]
    offset_s: Option<i64>,
}

pub type Watched = Arc<Mutex<WatchState>>;

pub async fn snapshot(w: &Watched) -> serde_json::Value {
    let s = w.lock().await;
    view(&s.targets, &s.seen)
}

/// `/data/watch`: what the minute watch holds, for the start page.
pub fn router(w: Watched) -> axum::Router {
    axum::Router::new()
        .route(
            "/data/watch",
            axum::routing::get(
                |axum::extract::State(w): axum::extract::State<Watched>| async move {
                    axum::Json(serde_json::json!({ "targets": snapshot(&w).await }))
                },
            ),
        )
        // ask-kp11be-2: the capped history behind the Apps uptime strips and
        // the Host page's reachability. `hours` up to 90 days, `bucket_s`
        // a multiple of an hour, `offset_s` the reader's UTC offset.
        .route(
            "/data/uptime",
            axum::routing::get(
                |axum::extract::State(w): axum::extract::State<Watched>,
                 axum::extract::Query(q): axum::extract::Query<UptimeQuery>| async move {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0);
                    let s = w.lock().await;
                    axum::Json(uptime::view(
                        &s.history,
                        now,
                        q.hours.unwrap_or(24 * 90),
                        q.bucket_s.unwrap_or(86_400),
                        q.offset_s.unwrap_or(0).clamp(-14 * 3600, 14 * 3600),
                    ))
                },
            ),
        )
        .with_state(w)
}

/// The tiles as the host lists them, without their readings, and the
/// host's own watch timing and which stack (if any) it is deploying.
async fn tiles(host: &Arc<dyn HostPort>) -> Result<TilesAnswer, String> {
    let r = host
        .ask_traced(
            homelab_proto::Command::Tiles { bare: true },
            Duration::from_secs(30),
            None,
        )
        .await?;
    let v: serde_json::Value = serde_json::from_str(&r.message).map_err(|e| e.to_string())?;
    Ok(TilesAnswer {
        tiles: v["tiles"].as_array().cloned().unwrap_or_default(),
        deploying_stack: v["deploying_stack"].as_str().map(String::from),
        watch_interval_s: v["watch_interval_s"].as_u64(),
        watch_down_after_s: v["watch_down_after_s"].as_u64(),
    })
}

/// Ask one address: any answer below 500 means the service is there.
async fn ask(http: &reqwest::Client, url: &str) -> Result<(), String> {
    match http.get(url).send().await {
        Ok(r) if r.status().as_u16() < 500 => Ok(()),
        Ok(r) => Err(format!("answered HTTP {}", r.status().as_u16())),
        Err(e) => Err(if e.is_timeout() {
            "no answer within 10 s".to_string()
        } else {
            // fix-watch-1: the whole cause chain (certificate, refused,
            // DNS), not only the outermost "error sending request".
            let e = e.without_url();
            let mut m = format!("no answer: {}", e);
            let mut src = std::error::Error::source(&e);
            while let Some(s) = src {
                m.push_str(&format!(": {}", s));
                src = s.source();
            }
            m
        }),
    }
}

pub fn spawn(
    host: Arc<dyn HostPort>,
    shared: Shared,
    center: Arc<NotifyCenter>,
    watched: Watched,
    actions: Actions,
    clock: Clock,
) {
    let flusher = watched.clone();
    tokio::spawn(async move {
        let mut t = tokio::time::interval(FLUSH_EVERY);
        t.tick().await;
        loop {
            t.tick().await;
            flush_history(&flusher).await;
        }
    });
    tokio::spawn(async move {
        // The loop ticks at the smallest interval a tile may ask for
        // (`watch_every >= 10`); each tile and the fleet default decide for
        // themselves whether this tick is theirs (`too_soon`). The host and
        // container readings cost nothing (they come from the fleet state
        // already read) and down-after is time-based, so a faster tick only
        // makes the dots more current.
        let mut t = tokio::time::interval(TICK);
        loop {
            t.tick().await;
            round(&host, &shared, &center, &watched, &actions, &clock).await;
        }
    });
}

/// `pub` (rather than the module-private a scheduled call needs) so a test
/// can run one round directly instead of waiting on `EVERY`. `clock` is
/// injected (same as the rest of milestone-act, `shell::actions::Clock`) so
/// a test can hold time still rather than racing `SystemTime::now()`.
pub async fn round(
    host: &Arc<dyn HostPort>,
    shared: &Shared,
    center: &Arc<NotifyCenter>,
    watched: &Watched,
    actions: &Actions,
    clock: &Clock,
) {
    let now = clock();
    let mut readings: Vec<(Target, Probe)> = Vec::new();

    // The host: the line's own state.
    let (link_error, stacks) = {
        let s = shared.read().await;
        (
            s.link_error.clone(),
            s.fleet
                .as_ref()
                .map(|f| {
                    f.stacks
                        .iter()
                        .map(|st| (st.name.clone(), st.online, st.enabled))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default(),
        )
    };

    // Decision "deploys are known outages" (Kenny, 2026-09-30): the stacks
    // to treat as a known outage this round, not a fault — the host's own
    // op-lock holder (AR12: at most one, since it is strictly serial, read
    // below from the `Tiles` answer) and every stack this dashboard itself
    // has a job running against.
    let mut deploying_stacks: HashSet<String> = actions
        .jobs()
        .into_iter()
        .filter(|j| j.state == JobState::Running && !j.stack.is_empty())
        .map(|j| j.stack)
        .collect();

    // Decision "default plus per tile" (2026-09-30): the fleet's own watch
    // timing, from the host's `Tiles` answer (`watch_interval_s` /
    // `watch_down_after_s`, host.toml); [`EVERY`] / [`DOWN_AFTER_S`] are
    // the fallback when the host does not send them. One read serves the
    // tile list, the deploying stack and this timing together.
    let mut watch_interval_s = EVERY.as_secs();
    let mut watch_down_after_s = DOWN_AFTER_S;
    let tiles_answer = if link_error.is_none() {
        match tiles(host).await {
            Ok(a) => {
                if let Some(s) = &a.deploying_stack {
                    deploying_stacks.insert(s.clone());
                }
                if let Some(v) = a.watch_interval_s {
                    watch_interval_s = v;
                }
                if let Some(v) = a.watch_down_after_s {
                    watch_down_after_s = v as i64;
                }
                Some(a)
            }
            Err(_) => None,
        }
    } else {
        None
    };

    readings.push((
        Target {
            key: "host".into(),
            name: "the host daemon".into(),
            stack: None,
            link: homelab_core::notify::page::HOST.into(),
        },
        Probe::Answer(
            match &link_error {
                None => Ok(()),
                Some(e) => Err(e.clone()),
            },
            watch_down_after_s,
        ),
    ));

    // Each enabled stack's container, as the host last read it. Only while
    // the line is up: a host that cannot be asked says nothing about them.
    if link_error.is_none() {
        for (name, online, enabled) in stacks {
            if !enabled {
                continue;
            }
            let probe = if deploying_stacks.contains(&name) {
                Probe::Deploying
            } else if online {
                Probe::Answer(Ok(()), watch_down_after_s)
            } else {
                Probe::Answer(
                    Err("the container is not running".into()),
                    watch_down_after_s,
                )
            };
            readings.push((
                Target {
                    key: format!("stack:{}", name),
                    name: format!("container of {}", name),
                    stack: Some(name.clone()),
                    link: homelab_core::notify::page::stack(&name),
                },
                probe,
            ));
        }
    }

    // Each tile's own backend, asked directly: no proxy, no Host header
    // (tile-watch, see REGISTER.md fix-89). Only a tile that carries a
    // `probe` (the client resolved one, at deploy time) is watched; one
    // with none is silently skipped, same as one with no reading.
    let complete_round = link_error.is_none() && tiles_answer.is_some();
    if let Some(a) = tiles_answer {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            // fix-watch-1: LAN backends (OPNsense, Proxmox) answer with their
            // own self-signed certificates, which Traefik accepts through
            // insecureSkipVerify; the watch only asks "is it there", so it
            // does the same instead of reporting them down while they are up.
            .danger_accept_invalid_certs(true)
            .build();
        if let Ok(http) = http {
            // Owner decision "default plus per tile": a tile checked less
            // than its own watch_every ago is skipped this round.
            let checked_at: BTreeMap<String, i64> = {
                let w = watched.lock().await;
                w.seen
                    .iter()
                    .map(|(k, s)| (k.clone(), s.checked_at))
                    .collect()
            };
            for t in a.tiles {
                let (Some(probe_url), Some(name), Some(host_key)) =
                    (t["probe"].as_str(), t["name"].as_str(), t["host"].as_str())
                else {
                    continue;
                };
                let stack = t["stack"].as_str().map(String::from);
                let key = format!("tile:{}", host_key);
                let watch_every = t["watch_every"].as_u64().unwrap_or(watch_interval_s) as i64;
                let down_after = t["down_after"]
                    .as_u64()
                    .map(|v| v as i64)
                    .unwrap_or(watch_down_after_s);
                let deploying = stack
                    .as_deref()
                    .is_some_and(|s| deploying_stacks.contains(s));
                let outcome = if deploying {
                    Probe::Deploying
                } else if crate::core::watch::too_soon(
                    checked_at.get(&key).copied().unwrap_or(0),
                    now,
                    watch_every,
                ) {
                    Probe::TooSoon
                } else {
                    Probe::Answer(ask(&http, probe_url).await, down_after)
                };
                readings.push((
                    Target {
                        key,
                        name: name.to_string(),
                        link: stack
                            .as_deref()
                            .map(homelab_core::notify::page::stack)
                            .unwrap_or_else(|| homelab_core::notify::page::HOST.into()),
                        stack,
                    },
                    outcome,
                ));
            }
        }
    }

    // ask-kp11be-2: one timed ping a minute feeds the Host page's
    // reachability history (the Ping command is the cheapest host read).
    let ping_due = link_error.is_none() && now - watched.lock().await.last_ping_at >= PING_EVERY_S;
    let ping = if ping_due {
        let t = std::time::Instant::now();
        let r = host
            .ask_traced(homelab_proto::Command::Ping, Duration::from_secs(15), None)
            .await;
        let ms = t.elapsed().as_millis() as u64;
        Some((now, r.ok().filter(|r| r.ok).map(|_| ms)))
    } else {
        None
    };

    let mut news = Vec::new();
    {
        let mut w = watched.lock().await;
        // fix-194: forget a target only after a COMPLETE round — the line up
        // and the tile list read. A round that could not enumerate the stacks
        // and tiles (a host restart, a failed `Tiles` read) says nothing about
        // them; dropping their state there wiped `down_told`, so the recovery
        // that followed was never told and the centre kept "Proxmox does not
        // answer" open forever (8 Downs, no Up, on 2026-10-02).
        if complete_round {
            let keep: Vec<String> = readings.iter().map(|(t, _)| t.key.clone()).collect();
            w.seen.retain(|k, _| keep.contains(k));
        }
        let w = &mut *w;
        for (target, outcome) in &readings {
            let seen = w.seen.entry(target.key.clone()).or_default();
            match outcome {
                Probe::TooSoon => {}
                Probe::Deploying => {
                    step_deploying(seen, now);
                    w.history.record(&target.key, now, Sample::Deploying);
                    w.history_dirty = true;
                }
                Probe::Answer(answer, down_after_s) => {
                    // The history counts every lost reading, not only the
                    // ones that outlast the five minutes before a Down notice.
                    w.history.record(
                        &target.key,
                        now,
                        if answer.is_ok() {
                            Sample::Up
                        } else {
                            Sample::Down
                        },
                    );
                    w.history_dirty = true;
                    if let Some(c) = step(seen, now, answer.clone(), *down_after_s) {
                        news.push((target.clone(), c));
                    }
                }
            }
        }
        if let Some((at, ms)) = ping {
            w.history.record_ping("ping:host", at, ms);
            w.last_ping_at = at;
            w.history_dirty = true;
        }
        w.targets = readings.into_iter().map(|(t, _)| t).collect();
    }
    for (t, c) in news {
        let (kind, title, body, level) = match &c {
            Change::Down { why, .. } => (
                Kind::Down,
                format!("{} does not answer", t.name),
                format!("for more than five minutes: {}", why),
                Level::Critical,
            ),
            Change::Up { was_down_s } => (
                Kind::Up,
                format!("{} answers again", t.name),
                format!("after {} min without an answer", (was_down_s + 59) / 60),
                Level::Info,
            ),
        };
        let mut d = Draft::new(kind, &format!("watch-{}", t.key), &title, &body);
        d.stack = t.stack.clone();
        d.detail = Detail {
            level,
            since: match c {
                Change::Down { since, .. } => Some(since),
                Change::Up { .. } => None,
            },
            consequence: matches!(c, Change::Down { .. })
                .then(|| "whoever uses it now gets no answer".to_string()),
            remedy: matches!(c, Change::Down { .. })
                .then(|| "open its page, look at its logs and restart it from there".to_string()),
            link: Some(t.link.clone()),
            source: Some("watch".into()),
            key: Some(t.key.clone()),
            ..Default::default()
        };
        center.notify(d).await;
    }
}
