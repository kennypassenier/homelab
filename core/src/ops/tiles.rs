//! replace-homepage (Kenny, 2026-09-30: "Vervangen"): the dashboard's start
//! page. Each stack declares its tiles in its own file (`tiles:`, keyed by
//! the hostname a tile opens); the host reads each tile's optional reading
//! in the stack's container and hands the page the whole list. Nothing here
//! knows an app: a tile's words and its reading are the stack file's.

use crate::executor::Executor;
use crate::state::HostState;

/// One tile as the page shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TileView {
    pub stack: String,
    pub host: String,
    pub url: String,
    /// tile-watch (owner decision "Afgeleid uit de tegels", 2026-09-30): the
    /// plain backend address to reach this tile directly at (no Traefik
    /// hop), as the client resolved it at deploy time. Absent = the tile
    /// carries no probe and the minute watch does not ask it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub probe: Option<String>,
    /// Owner decision "default plus per tile" (2026-09-30): this tile's own
    /// watch interval and down-after, when the stack file sets one; absent
    /// = the fleet default the host answers in the `Tiles` RPC.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_every: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub down_after: Option<u64>,
    pub name: String,
    pub group: String,
    pub order: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The reading's lines; empty when the tile has none.
    pub lines: Vec<String>,
    /// Why the reading gave nothing, when it failed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Every declared tile, sorted by group (its lowest order first) and then by
/// order and name. Readings run in the stacks' containers a few at a time.
///
/// **Pass the plain executor, not the tracing one**: a reading may read a key
/// on its way to the line it prints.
pub async fn read_tiles(exec: &dyn Executor, state: &HostState) -> Vec<TileView> {
    let mut todo: Vec<(TileView, u16, Option<String>)> = Vec::new();
    for (stack, st) in &state.stacks {
        let Some(m) = st.manifest.as_ref() else {
            continue;
        };
        for (host, t) in &m.tiles {
            todo.push((
                TileView {
                    stack: stack.clone(),
                    host: host.clone(),
                    url: t
                        .url
                        .clone()
                        .unwrap_or_else(|| format!("https://{}/", host)),
                    probe: t.probe.clone(),
                    watch_every: t.watch_every,
                    down_after: t.down_after,
                    name: t.name.clone(),
                    group: t.group.clone(),
                    order: t.order,
                    description: t.description.clone(),
                    lines: Vec::new(),
                    error: None,
                },
                m.vmid,
                t.reading.clone(),
            ));
        }
    }
    let mut tiles: Vec<TileView> = crate::ops::pool::bounded(
        todo.into_iter()
            .map(|(mut view, vmid, reading)| async move {
                if let Some(cmd) = reading {
                    match exec.run(&crate::executor::attach_sh(vmid, &cmd, 20)).await {
                        Ok(out) if out.success() => {
                            view.lines = out
                                .stdout
                                .lines()
                                .map(str::trim)
                                .filter(|l| !l.is_empty())
                                .take(4)
                                .map(str::to_string)
                                .collect();
                        }
                        Ok(out) => {
                            view.error = Some(
                                format!("exit {}: {}", out.code, out.stderr.trim())
                                    .chars()
                                    .take(160)
                                    .collect(),
                            )
                        }
                        Err(e) => view.error = Some(e.to_string().chars().take(160).collect()),
                    }
                }
                view
            })
            .collect(),
        crate::ops::pool::READ_CONCURRENCY,
    )
    .await;
    sort(&mut tiles);
    tiles
}

/// tile-watch-watcher-out (found 2026-10-01): every applied stack's own ip
/// and the tile-watch ports its own tiles probe, for every stack (with or
/// without a firewall of its own) — the fleet-wide input the deploy, the fleet check
/// and the editplan preview all derive the SAME watcher OUT rule from.
/// Walks `state.stacks` the same way [`read_tiles`] does.
pub fn fleet_tile_watch_targets(state: &HostState) -> crate::firewall::FleetTileTargets {
    crate::firewall::derive_fleet_tile_targets(state.stacks.values().filter_map(|st| {
        // Every stack with probes, firewall or not: the watcher's own
        // `OUT DROP` towards the VLAN blocks a target that has no firewall
        // of its own just the same (measured 2026-10-01: CT 120 reached no
        // backend on CT 106, which has no firewall file).
        let m = st.manifest.as_ref()?;
        Some((m.network.ip.as_str(), &m.tiles))
    }))
}

/// The stack whose own ip is `source` (the dashboard/watcher itself), as
/// everything a deploy needs to re-render and write its firewall. None when
/// `source` is empty, or no applied stack's own ip matches it, or that
/// stack declares no firewall.
pub fn tile_watch_watcher(state: &HostState, source: &str) -> Option<crate::firewall::TileWatcher> {
    let source = source.trim();
    let source = source.split('/').next().unwrap_or(source);
    if source.is_empty() {
        return None;
    }
    state.stacks.values().find_map(|st| {
        let m = st.manifest.as_ref()?;
        let ip = m.network.ip.split('/').next().unwrap_or(&m.network.ip);
        if ip != source {
            return None;
        }
        Some(crate::firewall::TileWatcher {
            vmid: m.vmid,
            stack: m.stack_name.clone(),
            own_ip: m.network.ip.clone(),
            firewall: m.firewall.clone()?,
            tiles: m.tiles.clone(),
        })
    })
}

/// Groups by their lowest order, then tiles by order and name.
pub fn sort(tiles: &mut [TileView]) {
    let mut first: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for t in tiles.iter() {
        let e = first.entry(t.group.clone()).or_insert(t.order);
        *e = (*e).min(t.order);
    }
    tiles.sort_by(|a, b| {
        first[&a.group]
            .cmp(&first[&b.group])
            .then(a.group.cmp(&b.group))
            .then(a.order.cmp(&b.order))
            .then(a.name.cmp(&b.name))
    });
}
