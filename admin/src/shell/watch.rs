//! replace-kuma (Kenny, 2026-09-30): the dashboard measures every minute
//! what Uptime Kuma measured. Each tile's own backend address, asked
//! directly (its `probe`, resolved by the client at deploy time,
//! `core::ops::tiles::TileView::probe`); each stack's container, as the
//! host last read it; and the host itself. Five minutes without an answer
//! is a Down notice (urgent: pushed at once), and the return an Up notice.
//! The host watches this dashboard in turn.
//!
//! tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30):
//! `HOMELAB_ADMIN_WATCH_VIA` (a Traefik plain entrypoint address, asked with
//! a forged Host header) is gone — measuring a tile through the gateway is
//! exactly what fix-89 (traefik-lan-host-header-bypass) closed the door on,
//! and reopening it for the watch was never sound. A tile with a `probe` is
//! watched, straight to that address; a tile with none is not, the same as
//! one with no reading.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::actions::{Actions, HostPort, JobState};
use super::actions_notify::NotifyCenter;
use super::host_link::Shared;
use crate::core::notify::{Detail, Draft, Kind, Level};
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
            format!("no answer: {}", e.without_url())
        }),
    }
}

pub fn spawn(
    host: Arc<dyn HostPort>,
    shared: Shared,
    center: Arc<NotifyCenter>,
    watched: Watched,
    actions: Actions,
) {
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
            round(&host, &shared, &center, &watched, &actions).await;
        }
    });
}

/// `pub` (rather than the module-private a scheduled call needs) so a test
/// can run one round directly instead of waiting on `EVERY`.
pub async fn round(
    host: &Arc<dyn HostPort>,
    shared: &Shared,
    center: &Arc<NotifyCenter>,
    watched: &Watched,
    actions: &Actions,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
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
    // (tile-watch, owner decision "Afgeleid uit de tegels", 2026-09-30 —
    // fix-89 closed the Traefik-Host-header door, and a measurement through
    // it never reopens it). Only a tile that carries a `probe` (the client
    // resolved one, at deploy time) is watched; one with none is silently
    // skipped, same as one with no reading.
    if let Some(a) = tiles_answer {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
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

    let mut news = Vec::new();
    {
        let mut w = watched.lock().await;
        let keep: Vec<String> = readings.iter().map(|(t, _)| t.key.clone()).collect();
        w.seen.retain(|k, _| keep.contains(k));
        for (target, outcome) in &readings {
            let seen = w.seen.entry(target.key.clone()).or_default();
            match outcome {
                Probe::TooSoon => {}
                Probe::Deploying => step_deploying(seen, now),
                Probe::Answer(answer, down_after_s) => {
                    if let Some(c) = step(seen, now, answer.clone(), *down_after_s) {
                        news.push((target.clone(), c));
                    }
                }
            }
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
