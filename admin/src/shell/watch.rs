//! replace-kuma (Kenny, 2026-09-30): the dashboard measures every minute
//! what Uptime Kuma measured. Each tile's address, asked through the proxy
//! on the house network (`HOMELAB_ADMIN_WATCH_VIA`), not through the
//! internet's front door; each stack's container, as the host last read it;
//! and the host itself. Five minutes without an answer is a Down notice
//! (urgent: pushed at once), and the return an Up notice. The host watches
//! this dashboard in turn.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;

use super::actions::HostPort;
use super::actions_notify::NotifyCenter;
use super::host_link::Shared;
use crate::core::notify::{Detail, Draft, Kind, Level};
use crate::core::watch::{step, view, Change, Seen, Target};

pub const EVERY: Duration = Duration::from_secs(60);

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

/// The tiles as the host lists them, without their readings.
async fn tiles(host: &Arc<dyn HostPort>) -> Result<Vec<serde_json::Value>, String> {
    let r = host
        .ask_traced(
            homelab_proto::Command::Tiles { bare: true },
            Duration::from_secs(30),
            None,
        )
        .await?;
    let v: serde_json::Value = serde_json::from_str(&r.message).map_err(|e| e.to_string())?;
    Ok(v["tiles"].as_array().cloned().unwrap_or_default())
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
    via: Option<String>,
    watched: Watched,
) {
    tokio::spawn(async move {
        let mut t = tokio::time::interval(EVERY);
        loop {
            t.tick().await;
            round(&host, &shared, &center, via.as_deref(), &watched).await;
        }
    });
}

async fn round(
    host: &Arc<dyn HostPort>,
    shared: &Shared,
    center: &Arc<NotifyCenter>,
    via: Option<&str>,
    watched: &Watched,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let mut readings: Vec<(Target, Result<(), String>)> = Vec::new();

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
    readings.push((
        Target {
            key: "host".into(),
            name: "the host daemon".into(),
            stack: None,
            link: homelab_core::notify::page::HOST.into(),
        },
        match &link_error {
            None => Ok(()),
            Some(e) => Err(e.clone()),
        },
    ));

    // Each enabled stack's container, as the host last read it. Only while
    // the line is up: a host that cannot be asked says nothing about them.
    if link_error.is_none() {
        for (name, online, enabled) in stacks {
            if !enabled {
                continue;
            }
            readings.push((
                Target {
                    key: format!("stack:{}", name),
                    name: format!("container of {}", name),
                    stack: Some(name.clone()),
                    link: homelab_core::notify::page::stack(&name),
                },
                if online {
                    Ok(())
                } else {
                    Err("the container is not running".into())
                },
            ));
        }
    }

    // Each tile's address, through the proxy on the house network.
    if let (Some(via), true) = (via, link_error.is_none()) {
        if let Ok(list) = tiles(host).await {
            let via_addr: Option<SocketAddr> = tokio::net::lookup_host(via)
                .await
                .ok()
                .and_then(|mut a| a.next());
            let mut builder = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none());
            for t in &list {
                if let (Some(h), Some(a)) = (t["host"].as_str(), via_addr) {
                    builder = builder.resolve(h, a);
                }
            }
            if let Ok(http) = builder.build() {
                for t in list {
                    let (Some(url), Some(name), Some(key)) =
                        (t["url"].as_str(), t["name"].as_str(), t["host"].as_str())
                    else {
                        continue;
                    };
                    let stack = t["stack"].as_str().map(String::from);
                    // Only a tile that opens its own routed hostname: that is
                    // what the proxy answers for. Through the proxy's plain
                    // entrypoint, since TLS ends at the front door.
                    let Some(rest) = url.strip_prefix(&format!("https://{}", key)) else {
                        continue;
                    };
                    let url = format!("http://{}{}", key, rest);
                    readings.push((
                        Target {
                            key: format!("tile:{}", key),
                            name: name.to_string(),
                            link: stack
                                .as_deref()
                                .map(homelab_core::notify::page::stack)
                                .unwrap_or_else(|| homelab_core::notify::page::HOST.into()),
                            stack,
                        },
                        ask(&http, &url).await,
                    ));
                }
            }
        }
    }

    let mut news = Vec::new();
    {
        let mut w = watched.lock().await;
        let keep: Vec<String> = readings.iter().map(|(t, _)| t.key.clone()).collect();
        w.seen.retain(|k, _| keep.contains(k));
        for (target, answer) in &readings {
            let seen = w.seen.entry(target.key.clone()).or_default();
            if let Some(c) = step(seen, now, answer.clone()) {
                news.push((target.clone(), c));
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
