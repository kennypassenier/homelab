//! Simulated world: fake but plausible data for every panel in the TUI.
//! Metrics random-walk, logs stream in, deploys and backups run as scripted
//! multi-step scenarios — so the mockup *feels* alive without any real infra.

use std::collections::VecDeque;

use rand::Rng;

pub const RING: usize = 60;

pub struct Ring {
    pub data: VecDeque<u64>,
    cur: f64,
}

impl Ring {
    pub fn new(start: f64) -> Self {
        let mut data = VecDeque::with_capacity(RING);
        for _ in 0..RING {
            data.push_back(start as u64);
        }
        Self { data, cur: start }
    }
    pub fn walk(&mut self, min: f64, max: f64, vol: f64) {
        let mut rng = rand::thread_rng();
        self.cur += rng.gen_range(-vol..vol);
        self.cur = self.cur.clamp(min, max);
        if self.data.len() >= RING {
            self.data.pop_front();
        }
        self.data.push_back(self.cur as u64);
    }
    pub fn last(&self) -> u64 {
        *self.data.back().unwrap_or(&0)
    }
    pub fn slice(&self) -> Vec<u64> {
        self.data.iter().copied().collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AppState {
    Running,
    Restarting,
    Stopped,
}

pub struct AppSim {
    pub name: &'static str,
    pub image: String,
    pub digest: String,
    pub state: AppState,
    pub restarts: u32,
    pub cpu: f32,
}

impl AppSim {
    fn new(name: &'static str, image: &str) -> Self {
        let mut rng = rand::thread_rng();
        Self {
            name,
            image: image.to_string(),
            digest: format!("{:08x}", rng.gen::<u32>()),
            state: AppState::Running,
            restarts: 0,
            cpu: rng.gen_range(0.5..8.0),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StackStatus {
    Online,
    Syncing,
    Degraded,
    Offline,
}

impl StackStatus {
    pub fn label(self) -> &'static str {
        match self {
            StackStatus::Online => "[ONLINE]",
            StackStatus::Syncing => "[SYNCING]",
            StackStatus::Degraded => "[DEGRADED]",
            StackStatus::Offline => "[OFFLINE]",
        }
    }
}

pub struct Stack {
    pub name: String,
    pub vmid: u16,
    pub ip: String,
    pub status: StackStatus,
    pub enabled: bool,
    pub drift: bool,
    pub sealed: bool, // .env present in HOST secrets vault
    pub apps: Vec<AppSim>,
    pub cpu: Ring,
    pub ram: Ring,
    pub ram_mb: u32,
    pub ram_limit_mb: u32,
    pub last_backup: String,
}

impl Stack {
    pub fn hostname(&self) -> String {
        format!("{}-app-{}", self.vmid, self.name)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Debug => "DBG",
            Level::Info => "INF",
            Level::Warn => "WRN",
            Level::Error => "ERR",
        }
    }
}

pub struct LogLine {
    pub time: String,
    pub source: String, // stack name or HOST/CLIENT
    pub level: Level,
    pub msg: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Running,
    Done,
}

pub struct DeployRun {
    pub stack_idx: usize,
    pub steps: Vec<(&'static str, StepState)>,
    pub current: usize,
    pub timer_ms: i64,
    pub finished: bool,
    pub log: Vec<(Level, String)>,
    sub_timer_ms: i64,
    sub_count: u32,
}

pub struct BackupRun {
    pub stack_idx: usize,
    pub progress: f64,
    pub bytes_done: f64,
}

pub struct Snapshot {
    pub id: String,
    pub stack: String,
    pub time: String,
    pub size: String,
}

pub struct GitState {
    pub branch: String,
    pub commit: String,
    pub last_msg: String,
    pub mirror_ok: bool,
    pub commits_today: u32,
}

pub struct Host {
    pub name: &'static str,
    pub cpu: Ring,
    pub ram: Ring,
    pub uptime: &'static str,
    pub temp: f64,
    pub disk_pct: u64,
    pub tls_fingerprint: &'static str,
}

pub struct World {
    pub host: Host,
    pub stacks: Vec<Stack>,
    pub logs: VecDeque<LogLine>,
    pub deploy: Option<DeployRun>,
    pub backup: Option<BackupRun>,
    pub snapshots: Vec<Snapshot>,
    pub git: GitState,
    pub clock: chrono::NaiveTime,
    log_cooldown_ms: i64,
    pub link_latency_ms: f64,
}

const DEPLOY_STEPS: &[&str] = &[
    "render manifests",
    "TLS channel :: push payload to HOST",
    "HOST :: commit to local repo",
    "pct push :: compose + env → LXC",
    "pct exec :: docker compose pull",
    "pct exec :: docker compose up -d",
    "verify :: compose ps + restart counters",
];

impl World {
    pub fn new() -> Self {
        let mk = |name: &str, vmid: u16, apps: Vec<AppSim>, ram_limit: u32| -> Stack {
            let mut rng = rand::thread_rng();
            Stack {
                name: name.to_string(),
                vmid,
                ip: format!("10.10.10.{}", vmid - 100),
                status: StackStatus::Online,
                enabled: true,
                drift: false,
                sealed: true,
                apps,
                cpu: Ring::new(rng.gen_range(5.0..30.0)),
                ram: Ring::new(rng.gen_range(20.0..60.0)),
                ram_mb: 0,
                ram_limit_mb: ram_limit,
                last_backup: "02:14".into(),
            }
        };

        let platform = mk(
            "platform",
            104,
            vec![
                AppSim::new("traefik", "traefik:v3.1"),
                AppSim::new("crowdsec", "crowdsecurity/crowdsec:latest"),
                AppSim::new("loki", "grafana/loki:3.1"),
                AppSim::new("grafana", "grafana/grafana:latest"),
                AppSim::new("uptime-kuma", "louislam/uptime-kuma:1"),
                AppSim::new("goaccess", "allinurl/goaccess:latest"),
                AppSim::new("cloudflared", "cloudflare/cloudflared:latest"),
            ],
            5120,
        );
        let downloader = mk(
            "downloader",
            105,
            vec![
                AppSim::new("gluetun", "qmcgaw/gluetun:latest"),
                AppSim::new("qbittorrent", "linuxserver/qbittorrent:latest"),
            ],
            2048,
        );
        let media = mk(
            "media",
            106,
            vec![
                AppSim::new("jellyfin", "jellyfin/jellyfin:latest"),
                AppSim::new("sonarr", "linuxserver/sonarr:latest"),
                AppSim::new("radarr", "linuxserver/radarr:latest"),
                AppSim::new("prowlarr", "linuxserver/prowlarr:latest"),
                AppSim::new("bazarr", "linuxserver/bazarr:latest"),
                AppSim::new("seerr", "fallenbagel/jellyseerr:latest"),
            ],
            8192,
        );
        let mut syncthing = mk(
            "syncthing",
            110,
            vec![AppSim::new("syncthing", "syncthing/syncthing:latest")],
            512,
        );
        syncthing.last_backup = "03:02".into();

        let snapshots = vec![
            ("platform", "today 02:14", "184 MB"),
            ("media", "today 02:19", "1.2 GB"),
            ("downloader", "today 02:22", "96 MB"),
            ("syncthing", "today 03:02", "11 MB"),
            ("platform", "yesterday 02:14", "180 MB"),
            ("media", "yesterday 02:20", "1.2 GB"),
            ("syncthing", "yesterday 03:02", "10 MB"),
        ]
        .into_iter()
        .map(|(s, t, sz)| {
            let mut rng = rand::thread_rng();
            Snapshot {
                id: format!("{:08x}", rng.gen::<u32>()),
                stack: s.into(),
                time: t.into(),
                size: sz.into(),
            }
        })
        .collect();

        Self {
            host: Host {
                name: "pve-01",
                cpu: Ring::new(18.0),
                ram: Ring::new(68.0),
                uptime: "47d 12h",
                temp: 51.0,
                disk_pct: 42,
                tls_fingerprint: "SHA256:9f2a…c41e [PINNED]",
            },
            stacks: vec![platform, downloader, media, syncthing],
            logs: VecDeque::with_capacity(600),
            deploy: None,
            backup: None,
            snapshots,
            git: GitState {
                branch: "main".into(),
                commit: "a3f9c21".into(),
                last_msg: "stacks/syncthing: bump versioning cleanup interval".into(),
                mirror_ok: true,
                commits_today: 3,
            },
            clock: chrono::Local::now().time(),
            log_cooldown_ms: 0,
            link_latency_ms: 0.8,
        }
    }

    pub fn next_free_vmid(&self) -> u16 {
        let used: Vec<u16> = self.stacks.iter().map(|s| s.vmid).collect();
        (108..355).find(|v| !used.contains(v)).unwrap_or(354)
    }

    pub fn tick(&mut self, dt_ms: i64) {
        let mut rng = rand::thread_rng();
        self.clock = chrono::Local::now().time();
        self.host.cpu.walk(3.0, 95.0, 4.0);
        self.host.ram.walk(55.0, 85.0, 1.0);
        self.host.temp = (self.host.temp + rng.gen_range(-0.4..0.4)).clamp(44.0, 62.0);
        self.link_latency_ms = (self.link_latency_ms + rng.gen_range(-0.15..0.15)).clamp(0.3, 4.0);

        for s in self.stacks.iter_mut() {
            let vol = if s.name == "media" { 6.0 } else { 3.0 };
            s.cpu.walk(1.0, 98.0, vol);
            s.ram.walk(10.0, 92.0, 1.5);
            s.ram_mb = (s.ram.last() as u32 * s.ram_limit_mb) / 100;
            for a in s.apps.iter_mut() {
                a.cpu = (a.cpu + rng.gen_range(-0.8..0.8)).clamp(0.1, 60.0);
            }
        }

        // Occasionally flip a media app into a short restart (or brief stop) so
        // the fleet shows real-looking degradation now and then.
        if rng.gen_bool(0.004) {
            if let Some(s) = self.stacks.iter_mut().find(|s| s.name == "media") {
                if let Some(a) = s.apps.iter_mut().find(|a| a.state == AppState::Running) {
                    a.state = if rng.gen_bool(0.25) {
                        AppState::Stopped
                    } else {
                        AppState::Restarting
                    };
                    a.restarts += 1;
                    if s.status == StackStatus::Online {
                        s.status = StackStatus::Degraded;
                    }
                }
            }
        } else if rng.gen_bool(0.06) {
            for s in self.stacks.iter_mut() {
                let mut recovered = false;
                for a in s.apps.iter_mut() {
                    if a.state != AppState::Running {
                        a.state = AppState::Running;
                        recovered = true;
                    }
                }
                if recovered && s.status == StackStatus::Degraded {
                    s.status = StackStatus::Online;
                }
            }
        }

        self.advance_deploy(dt_ms);
        self.advance_backup();

        self.log_cooldown_ms -= dt_ms;
        if self.log_cooldown_ms <= 0 {
            self.emit_random_log();
            self.log_cooldown_ms = rng.gen_range(180..1400);
        }
    }

    /// A plausible transcript line for the currently-running deploy step.
    fn deploy_sub_line(step: usize, stack: &Stack, count: u32) -> (Level, String) {
        let mut rng = rand::thread_rng();
        let app = stack
            .apps
            .get(count as usize % stack.apps.len().max(1))
            .map(|a| a.name)
            .unwrap_or("app");
        let image = stack
            .apps
            .get(count as usize % stack.apps.len().max(1))
            .map(|a| a.image.clone())
            .unwrap_or_default();
        match step {
            0 => (
                Level::Debug,
                [
                    format!("render :: lxc-compose.yml intent parsed ({} apps)", stack.apps.len()),
                    format!("render :: {}/docker-compose.yml validated", app),
                    "render :: env template resolved from vault (values redacted)".into(),
                ][count as usize % 3]
                    .clone(),
            ),
            1 => (
                Level::Debug,
                [
                    "tls :: session resumed · cipher TLS_AES_256_GCM_SHA384".into(),
                    format!("tls :: payload {:.1} KB → HOST · seq {}", rng.gen_range(4.0..40.0), count + 1),
                    "tls :: ack — payload integrity verified".into(),
                ][count as usize % 3]
                    .clone(),
            ),
            2 => (
                Level::Debug,
                [
                    format!("git :: stacks/{} staged", stack.name),
                    format!("git :: commit {:07x} \"deploy {}\"", rng.gen::<u32>() & 0xFFFFFFF, stack.name),
                    "git :: mirror push queued (github, non-blocking)".into(),
                ][count as usize % 3]
                    .clone(),
            ),
            3 => (
                Level::Debug,
                format!(
                    "pct push {} :: {}/docker-compose.yml + .env → /opt/{}/{}/",
                    stack.vmid, app, stack.name, app
                ),
            ),
            4 => (
                Level::Debug,
                [
                    format!("pull :: {} … digest sha256:{:012x}", image, rng.gen::<u64>()),
                    format!("pull :: {} :: layer cached, skipping", app),
                ][count as usize % 2]
                    .clone(),
            ),
            5 => (
                Level::Info,
                format!("up :: container {} started · network {}_net attached", app, stack.name),
            ),
            _ => (
                Level::Info,
                [
                    format!(
                        "verify :: compose ps → {}/{} running",
                        stack.apps.len(),
                        stack.apps.len()
                    ),
                    format!("verify :: restart counters clean for {}", app),
                    "verify :: gate PASS — no crash loops detected".into(),
                ][count as usize % 3]
                    .clone(),
            ),
        }
    }

    fn advance_deploy(&mut self, dt_ms: i64) {
        // Collect log lines to push after releasing the &mut borrow on deploy.
        let mut emitted: Vec<(String, Level, String)> = Vec::new();
        let mut clear_drift: Option<usize> = None;

        if let Some(d) = self.deploy.as_mut() {
            if d.finished {
                return;
            }

            // Sub-transcript: the running step chatters while it works.
            d.sub_timer_ms -= dt_ms;
            if d.sub_timer_ms <= 0 && d.current < d.steps.len() {
                let mut rng = rand::thread_rng();
                let stack = &self.stacks[d.stack_idx];
                let (lvl, line) = Self::deploy_sub_line(d.current, stack, d.sub_count);
                d.log.push((lvl, format!("  {}", line)));
                d.sub_count += 1;
                d.sub_timer_ms = rng.gen_range(220..520);
            }

            d.timer_ms -= dt_ms;
            if d.timer_ms <= 0 {
                let mut rng = rand::thread_rng();
                if d.current < d.steps.len() {
                    d.steps[d.current].1 = StepState::Done;
                    let stack = &self.stacks[d.stack_idx];
                    let step_name = d.steps[d.current].0;
                    d.log
                        .push((Level::Info, format!("[sync][exit] {} :: ok", step_name)));
                    emitted.push((
                        "HOST".into(),
                        Level::Info,
                        format!("deploy {} :: {} ✓", stack.name, step_name),
                    ));
                    d.current += 1;
                    d.sub_count = 0;
                    if d.current < d.steps.len() {
                        d.steps[d.current].1 = StepState::Running;
                        d.log
                            .push((Level::Info, format!("[sync][run ] {}", d.steps[d.current].0)));
                        d.timer_ms = rng.gen_range(900..2200);
                    } else {
                        d.finished = true;
                        d.log.push((
                            Level::Info,
                            "[sync] Sync complete — all gates passed".into(),
                        ));
                        clear_drift = Some(d.stack_idx);
                        emitted.push((
                            "HOST".into(),
                            Level::Info,
                            format!(
                                "deploy {} :: Sync complete — verified healthy",
                                self.stacks[d.stack_idx].name
                            ),
                        ));
                    }
                }
            }
        }

        if let Some(idx) = clear_drift {
            let s = &mut self.stacks[idx];
            s.drift = false;
            s.status = StackStatus::Online;
        }
        for (src, lvl, msg) in emitted {
            self.push_log(&src, lvl, msg);
        }
    }

    fn advance_backup(&mut self) {
        let mut done: Option<usize> = None;
        if let Some(b) = self.backup.as_mut() {
            let mut rng = rand::thread_rng();
            b.progress += rng.gen_range(0.01..0.05);
            b.bytes_done += rng.gen_range(0.5..4.0);
            if b.progress >= 1.0 {
                done = Some(b.stack_idx);
            }
        }
        if let Some(idx) = done {
            let name = self.stacks[idx].name.clone();
            let now = self.clock.format("%H:%M").to_string();
            self.stacks[idx].last_backup = now.clone();
            let mut rng = rand::thread_rng();
            self.snapshots.insert(
                0,
                Snapshot {
                    id: format!("{:08x}", rng.gen::<u32>()),
                    stack: name.clone(),
                    time: format!("today {}", now),
                    size: format!("{} MB", rng.gen_range(9..220)),
                },
            );
            self.backup = None;
            self.push_log(
                "HOST",
                Level::Info,
                format!("restic :: snapshot for {} complete, retention applied", name),
            );
        }
    }

    pub fn start_deploy(&mut self, stack_idx: usize) {
        if self.deploy.as_ref().map(|d| !d.finished).unwrap_or(false) {
            return;
        }
        let mut steps: Vec<(&'static str, StepState)> =
            DEPLOY_STEPS.iter().map(|s| (*s, StepState::Pending)).collect();
        steps[0].1 = StepState::Running;
        self.stacks[stack_idx].status = StackStatus::Syncing;
        let name = self.stacks[stack_idx].name.clone();
        self.push_log("CLIENT", Level::Info, format!("deploy requested :: {}", name));
        self.deploy = Some(DeployRun {
            stack_idx,
            steps,
            current: 0,
            timer_ms: 1400,
            finished: false,
            log: vec![(Level::Info, format!("[sync][run ] {}", DEPLOY_STEPS[0]))],
            sub_timer_ms: 250,
            sub_count: 0,
        });
    }

    pub fn start_backup(&mut self, stack_idx: usize) {
        if self.backup.is_some() {
            return;
        }
        let name = self.stacks[stack_idx].name.clone();
        self.push_log("HOST", Level::Info, format!("restic :: backup cycle start :: {}", name));
        self.backup = Some(BackupRun {
            stack_idx,
            progress: 0.0,
            bytes_done: 0.0,
        });
    }

    pub fn add_stack(&mut self, name: &str, preset_apps: &[(&'static str, &str)], ram: u32) {
        let vmid = self.next_free_vmid();
        let apps = preset_apps
            .iter()
            .map(|(n, img)| AppSim::new(n, img))
            .collect();
        let mut s = Stack {
            name: name.to_string(),
            vmid,
            ip: format!("10.10.10.{}", vmid - 100),
            status: StackStatus::Offline,
            enabled: false,
            drift: true,
            sealed: false,
            apps,
            cpu: Ring::new(0.0),
            ram: Ring::new(0.0),
            ram_mb: 0,
            ram_limit_mb: ram,
            last_backup: "—".into(),
        };
        s.status = StackStatus::Offline;
        self.push_log(
            "CLIENT",
            Level::Info,
            format!("scaffold :: stack {} created (vmid {}, deploy.enabled=false)", name, vmid),
        );
        self.git.commit = format!("{:07x}", rand::thread_rng().gen::<u32>() & 0xFFFFFFF);
        self.git.last_msg = format!("stacks/{}: initial scaffold", name);
        self.git.commits_today += 1;
        self.stacks.push(s);
    }

    pub fn remove_stack(&mut self, idx: usize) {
        if idx < self.stacks.len() {
            let name = self.stacks[idx].name.clone();
            self.stacks.remove(idx);
            self.push_log(
                "CLIENT",
                Level::Warn,
                format!("stack {} removed from repo (LXC untouched — destroy is separate)", name),
            );
        }
    }

    pub fn push_log(&mut self, source: &str, level: Level, msg: String) {
        if self.logs.len() > 500 {
            self.logs.pop_front();
        }
        self.logs.push_back(LogLine {
            time: self.clock.format("%H:%M:%S").to_string(),
            source: source.to_string(),
            level,
            msg,
        });
    }

    fn emit_random_log(&mut self) {
        let mut rng = rand::thread_rng();
        let pick = rng.gen_range(0..100);
        let (src, level, msg): (String, Level, String) = if pick < 22 {
            (
                "platform".into(),
                Level::Debug,
                format!(
                    "traefik :: 200 GET jellyfin.kp-soft.dev {}ms",
                    rng.gen_range(3..140)
                ),
            )
        } else if pick < 34 {
            (
                "media".into(),
                Level::Info,
                [
                    "jellyfin :: transcode session started (hw: vaapi)",
                    "sonarr :: rss sync complete, 0 new",
                    "radarr :: import: Movie.2026.1080p → /mnt/data/18TB",
                    "bazarr :: subtitles fetched (nl) for 1 episode",
                ][rng.gen_range(0..4)]
                    .to_string(),
            )
        } else if pick < 44 {
            (
                "downloader".into(),
                Level::Debug,
                format!(
                    "gluetun :: vpn healthy, egress {}.{}.{}.{}",
                    rng.gen_range(80..180),
                    rng.gen_range(1..250),
                    rng.gen_range(1..250),
                    rng.gen_range(1..250)
                ),
            )
        } else if pick < 56 {
            (
                "syncthing".into(),
                Level::Info,
                [
                    "syncthing :: index update from desktop (12 files)",
                    "syncthing :: folder \"obsidian-vault\" in sync",
                    "syncthing :: versioning: pruned 3 old versions",
                    "syncthing :: connected to phone (QUIC)",
                ][rng.gen_range(0..4)]
                    .to_string(),
            )
        } else if pick < 70 {
            (
                "HOST".into(),
                Level::Debug,
                format!(
                    "heartbeat :: CLIENT fresh ({}ms) — failsafe window skipped",
                    rng.gen_range(2..40)
                ),
            )
        } else if pick < 78 {
            (
                "platform".into(),
                Level::Warn,
                format!(
                    "crowdsec :: ip {}.{}.{}.{} banned (http-probing)",
                    rng.gen_range(2..250),
                    rng.gen_range(1..250),
                    rng.gen_range(1..250),
                    rng.gen_range(1..250)
                ),
            )
        } else if pick < 86 {
            (
                "HOST".into(),
                Level::Debug,
                "gitops :: repo clean, mirror push ok".into(),
            )
        } else if pick < 94 {
            (
                "platform".into(),
                Level::Debug,
                format!("loki :: ingested {} lines", rng.gen_range(40..900)),
            )
        } else if pick < 98 {
            (
                "media".into(),
                Level::Warn,
                "jellyfin :: client buffering — bitrate stepped down".into(),
            )
        } else {
            (
                "HOST".into(),
                Level::Error,
                "pct exec :: transient timeout, retried ok (1/3)".into(),
            )
        };
        self.push_log(&src, level, msg);
    }
}
