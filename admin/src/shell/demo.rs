//! feat-platform-10: the demo host, only in a build with the `demo-host`
//! feature (Kenny, 2026-09-28: "Only in test builds"). `make release` builds
//! without it.

use std::time::Duration;

use axum::extract::Query;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use chassis::shell::live::Live;
use homelab_proto::{Command, RpcResponse, ServerMsg};

use super::host_link::{Asked, HostAsks, Shared, now_s};
use crate::core::fleet::fleet_view;

/// The version the demo host says Hello with: the release every gate in
/// this dashboard asks for, so every page works against it.
pub const DEMO_VERSION: &str = "3.63.0";

/// fix-220 (Kenny, 2026-10-02): a made-up Prometheus and Loki, answering
/// this process's own loopback, for the charts/traffic e2e smoke — the real
/// Prometheus/Loki clients, pointed here instead of a real metrics stack, so
/// the invariants suite exercises the actual humanizing code path
/// (`homelab_core::charts::humanize`) rather than asserting on it directly.
/// Deliberately includes the raw ids Kenny saw on the live host (a firewall
/// bridge, a hwmon PCI chip id, a Proxmox cgroup id) so a chart series label
/// matching a raw-id pattern is a real, provable regression, not a claim.
/// `demo-host` only, same as [`run_demo`] — no real metrics stack is ever
/// contacted when this is in use.
pub async fn spawn_demo_metrics() -> String {
    let app = axum::Router::new()
        .route("/api/v1/query_range", get(prom_query))
        .route("/api/v1/query", get(prom_query))
        .route("/loki/api/v1/query_range", get(loki_query_range))
        .route(
            "/loki/api/v1/query",
            get(|| async {
                // fix-221: the real gateway (stacks/metrics/loki-push/nginx.conf)
                // answers this path with a bare 403 for everyone but Grafana;
                // the demo mirrors that so a regression (an instant query
                // Loki call creeping back in) fails the same way live does.
                (
                    axum::http::StatusCode::FORBIDDEN,
                    "<html><head><title>403 Forbidden</title></head><body><center>403 Forbidden</center><hr><center>nginx</center></body></html>",
                )
                    .into_response()
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("demo metrics: bind a loopback port");
    let addr = listener.local_addr().expect("demo metrics: local_addr");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    format!("http://{addr}")
}

#[derive(serde::Deserialize)]
struct PromQ {
    query: String,
}

/// One made-up `matrix`/`vector` answer, chosen by matching a fragment of
/// the query text — the same thing a real Prometheus would answer for that
/// `by (...)` clause, with one point per series (good enough for both
/// `query` and `query_range`: `panelEl` only ever reads the last point).
async fn prom_query(Query(q): Query<PromQ>) -> Response {
    axum::Json(demo_metric_body(&q.query)).into_response()
}

async fn loki_query_range(Query(q): Query<PromQ>) -> Response {
    axum::Json(demo_metric_body(&q.query)).into_response()
}

/// Deliberately raw ids (fix-220's whole-screen invariant proves the
/// dashboard never shows one of these as a series label):
/// - `fwbr117i0` / `veth117i0` — a guest's own firewall bridge and virtual
///   NIC, vmid 117 embedded by Proxmox's own naming.
/// - `0000:00:01_0_0000:01:00_0` — an hwmon chip id, the sysfs PCI path with
///   `/` and `.` turned into `_` (exactly what Kenny saw live).
/// - `lxc/117` — a Proxmox cgroup id, same vmid.
///
/// One SMART drive answers "not ok" (`sdb`) so the health table has both
/// states to show, not one flat "ok" line.
fn demo_metric_body(query: &str) -> serde_json::Value {
    let at = now_s();
    let series = |pairs: &[(&str, f64)]| -> serde_json::Value {
        let result: Vec<serde_json::Value> = pairs
            .iter()
            .map(|(label, v)| {
                serde_json::json!({
                    "metric": { metric_key(query): label },
                    "value": [at, v.to_string()],
                    "values": [[at, v.to_string()]],
                })
            })
            .collect();
        serde_json::json!({ "status": "success", "data": { "resultType": "matrix", "result": result } })
    };
    if query.contains("node_hwmon_temp_celsius") {
        return series(&[
            ("0000:00:01_0_0000:01:00_0", 46.0),
            ("0000:00:02_0_0000:02:00_0", 52.0),
        ]);
    }
    if query.contains("smart_device_health_ok") {
        return series(&[("sda", 1.0), ("sdb", 0.0)]);
    }
    if query.contains("smart_device_pending_sectors") {
        return series(&[("sda", 0.0), ("sdb", 12.0)]);
    }
    if query.contains("smart_device_reallocated_sectors") {
        return series(&[("sda", 0.0), ("sdb", 3.0)]);
    }
    if query.contains("smart_device_temperature_celsius") {
        return series(&[("sda", 34.0), ("sdb", 41.0)]);
    }
    if query.contains("smart_device_power_on_hours") {
        return series(&[("sda", 8760.0), ("sdb", 12000.0)]);
    }
    if query.contains("pve_memory_usage_bytes") {
        return series(&[("lxc/117", 2_147_483_648.0), ("lxc/118", 1_073_741_824.0)]);
    }
    if query.contains("node_network_receive_bytes_total") {
        return series(&[
            ("vmbr0", 120_000.0),
            ("fwbr117i0", 4_200.0),
            ("veth118i0", 1_800.0),
        ]);
    }
    if query.contains("node_load") {
        return series(&[("", 1.25)]);
    }
    if query.contains("RequestHost") {
        return series(&[("demo.example.org", 42.0), ("films.example.org", 7.0)]);
    }
    if query.contains("DownstreamStatus") {
        return series(&[("200", 44.0), ("404", 3.0), ("500", 1.0)]);
    }
    // CPU/memory/disk and anything else: one plain series, no legend.
    series(&[("", 12.5)])
}

/// The `by (...)` label this query groups on, read straight out of the
/// query text — the same label the real admin passes as `legend`, so the
/// made-up `metric` map always carries the field `routes.rs` asks for.
fn metric_key(query: &str) -> &'static str {
    if query.contains("smart_device") {
        "device"
    } else if query.contains("by (chip)") {
        "chip"
    } else if query.contains("by (device)") {
        "device"
    } else if query.contains("by (id)") {
        "id"
    } else if query.contains("by (name)") {
        "name"
    } else if query.contains("RequestHost") {
        "RequestHost"
    } else if query.contains("DownstreamStatus") {
        "DownstreamStatus"
    } else if query.contains("node_filesystem") {
        "mountpoint"
    } else {
        "stack"
    }
}

/// feat-platform-10: a host that lives in this process, for the browser
/// tests (`HOMELAB_ADMIN_DEMO_HOST=1`). The real line is never opened: the
/// fleet is the stacks named in `stacks` as the working copy describes
/// them, reads answer empty, and every command that would change something
/// only prints three step lines and answers "complete". Nothing leaves the
/// machine.
/// fix-231: the demo host's stand-in for fix-83's pin check — one `Noted`
/// finding per digest-pinned image in the working copy that declares an
/// upstream (`com.homelab.update.upstream`), worded exactly as
/// `homelab_core::ops::pins::evaluate_pins` writes it. Generic: whatever
/// the working copy holds. The first such image (stack, then file order)
/// is made a MAJOR jump, every other one a minor one, so the Fleet view's
/// dialog shows both. One more finding names a made-up pin on two stacks
/// that lives in no stack file (the shape of a pin kept in code), so the
/// table's "updated with a homelab release" row has something to show.
pub fn demo_stale_findings(repo: &std::path::Path, stacks: &[String]) -> Vec<serde_json::Value> {
    use homelab_core::ops::pins::pinned_version;
    let mut out = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir(repo.join("stacks"))
        .map(|d| {
            d.flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    for stack in &names {
        let texts = super::workcopy::read_texts(&repo.join("stacks").join(stack));
        for (path, text) in &texts {
            if !path.ends_with("docker-compose.yml") {
                continue;
            }
            let Ok(v) = serde_yaml::from_str::<serde_yaml::Value>(text) else {
                continue;
            };
            let Some(serde_yaml::Value::Mapping(services)) = v.get("services") else {
                continue;
            };
            for (service, s) in services {
                let (Some(service), Some(image)) =
                    (service.as_str(), s.get("image").and_then(|i| i.as_str()))
                else {
                    continue;
                };
                let Some(version) = pinned_version(image).filter(|_| image.contains('@')) else {
                    continue;
                };
                let upstream = s
                    .get("labels")
                    .and_then(|l| l.as_sequence())
                    .into_iter()
                    .flatten()
                    .filter_map(|l| l.as_str())
                    .find_map(|l| l.strip_prefix(homelab_core::ops::pins::UPSTREAM_LABEL))
                    .and_then(|l| l.strip_prefix('='))
                    .map(str::to_string);
                let Some(upstream) = upstream else { continue };
                let container = s
                    .get("container_name")
                    .and_then(|c| c.as_str())
                    .unwrap_or(service);
                let latest = demo_bump(&version, out.is_empty());
                out.push(serde_json::json!({
                    "severity": "Noted",
                    "subject": format!("{stack}/{container}"),
                    "what": format!("pinned to {version}; upstream {upstream} released {latest} on 2026-09-30"),
                    "remedy": "nothing is urgent (demo host)",
                }));
            }
        }
    }
    let two: Vec<&String> = stacks.iter().take(2).collect();
    if !two.is_empty() {
        out.push(serde_json::json!({
            "severity": "Noted",
            "subject": two.iter().map(|s| format!("{s}/demo-agent")).collect::<Vec<_>>().join(", "),
            "what": "pinned to v0.1.0; upstream github.com/example/demo-agent released v0.2.0 on 2026-09-28",
            "remedy": "nothing is urgent (demo host)",
        }));
    }
    out
}

/// A made-up newer version of `v`, keeping its own `v` prefix: the first
/// number up by one (a major jump) or the second (a minor one).
fn demo_bump(v: &str, major: bool) -> String {
    let (prefix, rest) = match v.strip_prefix('v') {
        Some(r) => ("v", r),
        None => ("", v),
    };
    let mut parts: Vec<u64> = rest
        .split('.')
        .map(|p| {
            p.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(0)
        })
        .collect();
    while parts.len() < 3 {
        parts.push(0);
    }
    if major {
        parts = vec![parts[0] + 1, 0, 0];
    } else {
        parts = vec![parts[0], parts[1] + 1, 0];
    }
    format!(
        "{prefix}{}",
        parts
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(".")
    )
}

/// fix-231: the demo host's registry — a made-up digest for any tag, the
/// same one every time, so the update dialog has a new image reference to
/// show without asking a real registry.
pub fn demo_resolve(
    _registry: &str,
    repository: &str,
    tag: &str,
) -> Result<Option<String>, String> {
    let seed: String = format!("{repository}:{tag}")
        .bytes()
        .map(|b| format!("{:02x}", b))
        .collect();
    Ok(Some(format!(
        "sha256:{}",
        seed.chars().cycle().take(64).collect::<String>()
    )))
}

/// redesign-backups (Backups redesign 3.71): the nights (0 = the newest, one day
/// apart) a demo stack wrote a snapshot on: twelve of them, except that the
/// third stack of `HOMELAB_ADMIN_DEMO_STACKS` (whichever name it has, never
/// hard-coded) missed night 3, so the coverage heatmap shows a missed night
/// and a whole-fleet night short of every stack. `BackupCalendar` and
/// `GetBackups` both read it, so the two answers agree.
fn demo_nights<'a>(stacks: &'a [String], stack: &'a str) -> impl Iterator<Item = u64> + 'a {
    let gap = stacks.len() > 2 && stacks[2] == stack;
    (0..12u64).filter(move |i| !(gap && *i == 3))
}

pub async fn run_demo(
    shared: Shared,
    live: Live,
    mut asks: HostAsks,
    repo: std::path::PathBuf,
    stacks: Vec<String>,
) {
    use homelab_proto::{AppView, FleetState, HostView, LogLevel, StackView, StepMark};
    // Rebuilt on every tick: the working copy is cloned beside this start.
    let build = || FleetState {
        host: HostView {
            name: "demo".into(),
            cpu_pct: Some(7),
            ram_pct: 40,
            disk_pct: 31,
            tls_fingerprint: String::new(),
            ram_total_mb: 65536,
            ram_used_mb: 26000,
            ram_committed_mb: 30000,
            cores_total: 16,
            load1_x100: 80,
            home_address: None,
            // fix-222 (Kenny, 2026-10-02: "is dat de 1TB SSD die erin
            // zit?"): made-up but realistic — a 1 TB SSD (the whole-disk
            // figure a `blockdev --getsize64` read would give), with root
            // and local-lvm as two of its partitions.
            disk_detail: Some(homelab_proto::HostDiskDetail {
                root_lv_size_gb: 96.0,
                root_disk_device: "/dev/sda".into(),
                root_disk_total_gb: 931.5,
                thin_pool_size_gb: 780.0,
                top_dirs: vec![
                    ("/var".into(), 42.0),
                    ("/usr".into(), 18.0),
                    ("/home".into(), 6.0),
                    ("/opt".into(), 3.0),
                ],
                measured_at: now_s(),
            }),
        },
        stacks: stacks
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let m = homelab_client::spec::build_manifest(&repo.join("stacks").join(name)).ok();
                StackView {
                    name: name.clone(),
                    vmid: m.as_ref().map(|m| m.vmid).unwrap_or(900 + i as u16),
                    hostname: m
                        .as_ref()
                        .map(|m| m.hostname.clone())
                        .unwrap_or_else(|| name.clone()),
                    apps: m
                        .map(|m| m.apps)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|a| AppView {
                            name: a,
                            running: true,
                            restarts: 0,
                        })
                        .collect(),
                    drift: false,
                    applied_hash: String::new(),
                    env_sealed: false,
                    online: true,
                    enabled: true,
                    usage: None,
                    applied_source: None,
                    component_digests: Default::default(),
                    native: false,
                }
            })
            .collect(),
        status_measured_at: Some(now_s()),
    };
    {
        let mut s = shared.write().await;
        s.fleet = Some(fleet_view(&build(), now_s()));
        s.host_version = Some(DEMO_VERSION.into());
        s.host_build = Some("demo host in homelab-admin".into());
        s.link_error = None;
    }
    tracing::warn!(
        "HOMELAB_ADMIN_DEMO_HOST is set: a demo host answers; no real host is contacted"
    );
    let _ = live.publish("link", &serde_json::json!({ "up": true, "host_version": DEMO_VERSION, "host_build": "demo host in homelab-admin" }));
    let mut next: u64 = now_s() << 20;
    // redesign-3.71 secrets: what this demo host was asked to reveal or
    // copy, as the real host records it in history.jsonl (never a value),
    // so the screen tests can see Activity name it.
    let mut history: Vec<homelab_core::history::HistoryEntry> = Vec::new();
    let mut tick = tokio::time::interval(Duration::from_secs(5));
    loop {
        tokio::select! {
            Some(Asked { command, reply, sent }) = asks.rx.recv() => {
                let id = next;
                next += 1;
                if let Some(s) = sent {
                    let _ = s.send(id);
                }
                let answer = move |message: String| RpcResponse { id, ok: true, message, deferred: None };
                // redesign-3.71 secrets: a made-up value at once (the real
                // host reads its vault, no job), recorded as the real host
                // records it.
                if let Command::RevealSecret { stack, secret, audit } = &command {
                    let purpose = audit.as_ref().map(|a| a.purpose).unwrap_or_default();
                    history.push(homelab_core::ops::secrets::reveal_history(
                        now_s(),
                        stack,
                        secret,
                        purpose,
                        audit.as_ref().map(|a| a.by.clone()),
                        Some(id),
                        None,
                    ));
                    let rel = homelab_core::ops::secrets::latch_rel_path(stack, secret);
                    let _ = reply.send(Ok(answer(format!("DEMO_VALUE=made-up-for-{}", rel.replace('/', "-")))));
                    continue;
                }
                if command.is_read_only() {
                    let body = match command {
                        Command::History { since, limit } => serde_json::json!({
                            "entries": homelab_core::history::select(history.clone(), since, limit),
                        }).to_string(),
                        Command::Incidents { .. } => serde_json::json!({ "incidents": [] }).to_string(),
                        // One made-up notice, so the notification centre has
                        // an expandable row for the screen tests to click.
                        Command::Notices { .. } => serde_json::json!({ "notices": [{
                            "seq": 1, "at": 1_790_000_000u64, "since": 1_790_000_000u64,
                            "op": "deploy-films", "label": "deploy films", "ok": false,
                            "stack": "films", "title": "demo host: deploy films failed",
                            "what": "a made-up failure so the centre has a row",
                            "consequence": "nothing; this is the demo host",
                            "remedy": "nothing; this is the demo host",
                            "page": "/stack/films", "urgent": false,
                            "routed": "centre", "push": "",
                        }], "last_seq": 1 }).to_string(),
                        Command::CurrentOp => serde_json::to_string(&homelab_proto::CurrentOpView::default()).unwrap_or_default(),
                        // The TUI parity pages' reads, in the shapes the real
                        // host answers (empty, or plainly made up and said so).
                        Command::Today { .. } => serde_json::json!({ "items": [{
                            "level": "Attention", "source": "check",
                            "what": "demo host: one made-up item so the list has a row",
                            "remedy": "nothing; this is the demo host",
                        }], "unread": [] }).to_string(),
                        // fix-231: the stale-image rows the Fleet view
                        // offers Update on, made up from the working copy.
                        Command::FleetCheck { .. } => serde_json::json!({
                            "passes": true,
                            "findings": demo_stale_findings(&repo, &stacks),
                        }).to_string(),
                        Command::ListManualChecks { .. } => serde_json::json!({ "now": now_s(), "checks": [] }).to_string(),
                        Command::ListTemplates => "clonable golden templates (fast):\nclone:996  debian-13-homelab-v4\n\nOS templates (full bootstrap):\n  local:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst\n".into(),
                        Command::GetApplied { .. } => "[]".into(),
                        // fix-207: the demo host's stand-in for "what pve
                        // actually enforces" — made up, but made up the way
                        // Kenny's own fleet looks: the repository's own
                        // declaration for most stacks, with the first stack
                        // (sorted) that declares no firewall (or declares
                        // one switched off) shown as enforced anyway and
                        // flagged as not matching the repository, so the
                        // invariants smoke can pin that the dashboard shows
                        // live state over a stale repository rather than
                        // the other way round.
                        Command::GetFirewallLive { stack_files } => {
                            let declared_on = |name: &str| {
                                homelab_client::spec::build_manifest(
                                    &repo.join("stacks").join(name),
                                )
                                .ok()
                                .and_then(|m| m.firewall)
                                .is_some_and(|f| f.enabled)
                            };
                            let mut off_first: Vec<&str> = stack_files
                                .iter()
                                .map(|(s, _)| s.as_str())
                                .filter(|s| !declared_on(s))
                                .collect();
                            off_first.sort();
                            let override_stack = off_first.first().copied();
                            let statuses: std::collections::BTreeMap<_, _> = stack_files
                                .iter()
                                .map(|(name, _vmid)| {
                                    let on = declared_on(name);
                                    let (enforced, matches_repo) =
                                        if Some(name.as_str()) == override_stack {
                                            (true, false)
                                        } else {
                                            (on, true)
                                        };
                                    (
                                        name.clone(),
                                        serde_json::json!({ "enforced": enforced, "matches_repo": matches_repo }),
                                    )
                                })
                                .collect();
                            serde_json::json!({ "statuses": statuses }).to_string()
                        }
                        Command::Ping => "pong (demo host)".into(),
                        Command::GetHostConfig => serde_json::to_string(&homelab_proto::HostConfigFile {
                            path: "/etc/homelab/host.toml (demo host)".into(),
                            ..Default::default()
                        }).unwrap_or_default(),
                        // fix-202 (invariants: the backup calendar must show
                        // partial data at once and never hang on a stack
                        // with nothing to read): every requested stack
                        // answers at once — the demo host's own LAST stack
                        // (whichever name `HOMELAB_ADMIN_DEMO_STACKS` gives
                        // it, never hard-coded) stands in for "keeps no
                        // data at all" (`no_backup`), every other one for an
                        // ordinary read with a few nights of snapshots.
                        Command::BackupCalendar { stacks: req, .. } => {
                            let names: Vec<String> = if req.is_empty() { stacks.clone() } else { req };
                            let now = now_s();
                            let mut by_stack = serde_json::Map::new();
                            let mut measured_at = serde_json::Map::new();
                            let mut no_backup = Vec::new();
                            for name in &names {
                                if stacks.last() == Some(name) && stacks.len() > 1 {
                                    no_backup.push(name.clone());
                                    measured_at.insert(name.clone(), serde_json::json!(now));
                                    continue;
                                }
                                let nights: Vec<u64> = demo_nights(&stacks, name)
                                    .map(|i| now - i * 86_400 - 3600)
                                    .collect();
                                by_stack.insert(name.clone(), serde_json::json!(nights));
                                measured_at.insert(name.clone(), serde_json::json!(now));
                            }
                            serde_json::json!({
                                "stacks": by_stack,
                                "measured_at": measured_at,
                                "skipped": [],
                                "no_backup": no_backup,
                                "reasons": {},
                            }).to_string()
                        }
                        // fix-216 (invariants: the restore dialog's own
                        // snapshot picker needs real snapshots to show):
                        // one restic repository per declared app (compose)
                        // or native unit, each with a few nights of
                        // snapshots at distinct times so the picker's
                        // newest-first order and "N days ago" are provable
                        // without a real restic repository.
                        Command::GetBackups { stack, .. } => {
                            let m = homelab_client::spec::build_manifest(
                                &repo.join("stacks").join(&stack),
                            )
                            .ok();
                            let native = m.as_ref().is_some_and(|m| !m.natives.is_empty());
                            let owners: Vec<String> = m
                                .map(|m| if native { m.natives } else { m.apps })
                                .unwrap_or_default();
                            let now = now_s();
                            let last = owners.len().saturating_sub(1);
                            let at = stacks.iter().position(|s| *s == stack);
                            let repos: Vec<serde_json::Value> = owners
                                .iter()
                                .enumerate()
                                .map(|(oi, owner)| {
                                    let snaps: Vec<serde_json::Value> = demo_nights(&stacks, &stack)
                                        .map(|i| {
                                            let t = now
                                                - (oi as u64) * 1_800
                                                - i * 86_400
                                                - 3_600;
                                            serde_json::json!({
                                                "id": format!("demo{oi}{i:02}snapshotidfortest"),
                                                "short_id": format!("demo{oi}{i:02}"),
                                                "time": t,
                                                "run": t,
                                            })
                                        })
                                        .collect();
                                    // redesign-backups (Backups redesign 3.71): a size for
                                    // every repository but a several-app
                                    // stack's last (the real host has none
                                    // when `restic stats` failed: "—"), and
                                    // a drill verdict on two: the second
                                    // stack's first repository passed, the
                                    // third stack's last one failed.
                                    let size = (last == 0 || oi < last)
                                        .then(|| 48_234_496_u64 * (oi as u64 + 1));
                                    let drill = match (at, oi) {
                                        (Some(1), 0) => Some(serde_json::json!({
                                            "last_attempt": now - 2 * 86_400,
                                            "last_pass": now - 2 * 86_400,
                                            "last_error": null,
                                        })),
                                        (Some(2), o) if o == last && last > 0 => Some(serde_json::json!({
                                            "last_attempt": now - 86_400,
                                            "last_pass": 0,
                                            "last_error": "restored 0 files (demo)",
                                        })),
                                        _ => None,
                                    };
                                    let mut repo = serde_json::json!({
                                        "owner": owner,
                                        "newest_snapshot": snaps.first(),
                                        "snapshot_count": snaps.len(),
                                        "snapshots": snaps,
                                        "measured_at": now,
                                    });
                                    if let Some(b) = size {
                                        repo["size_bytes"] = serde_json::json!(b);
                                    }
                                    if let Some(d) = drill {
                                        repo["drill"] = d;
                                    }
                                    repo
                                })
                                .collect();
                            serde_json::json!({ "native": native, "repos": repos }).to_string()
                        }
                        // fix-257 (design review, 2026-10-03): the real
                        // host answers TokenList with a list
                        // (`host/src/main.rs`, `Vec<TokenView>`); this demo
                        // fell through to `{}`, which Settings could not
                        // read ("invalid type: map, expected a sequence").
                        Command::TokenList => serde_json::to_string(&[
                            homelab_proto::TokenView {
                                name: "legacy".into(),
                                scope: homelab_proto::Scope::All,
                            },
                            homelab_proto::TokenView {
                                name: "wsl".into(),
                                scope: homelab_proto::Scope::Operate,
                            },
                        ])
                        .unwrap_or_default(),
                        _ => "{}".into(),
                    };
                    let _ = reply.send(Ok(answer(body)));
                    continue;
                }
                let events = asks.events.clone();
                let name = command.name().to_string();
                tokio::spawn(async move {
                    let op = format!("{}-demo", name.split('_').next().unwrap_or("op"));
                    for step in ["prepare", "apply", "verify"] {
                        for finished in [false, true] {
                            let _ = events.send(ServerMsg::Log {
                                level: LogLevel::Info,
                                source: "DEMO".into(),
                                msg: format!("[{op}] {step} {}", if finished { "done" } else { "started" }),
                                req: Some(id),
                                ts: Some(now_s()),
                                step: Some(StepMark { op: op.clone(), step: step.into(), finished, changed: finished, skipped: false }),
                                plan: None,
                                by: Some("admin".into()),
                            });
                            tokio::time::sleep(Duration::from_millis(600)).await;
                        }
                    }
                    let _ = reply.send(Ok(answer(format!("{name} complete (demo host: nothing ran)"))));
                });
            }
            _ = tick.tick() => {
                let f = fleet_view(&build(), now_s());
                shared.write().await.fleet = Some(f.clone());
                let _ = live.publish("fleet", &serde_json::json!({ "changed": false, "fleet": f }));
            }
        }
    }
}
