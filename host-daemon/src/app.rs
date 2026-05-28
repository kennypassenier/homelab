#[derive(Debug, Clone)]
pub struct LxcNode {
    pub id: u32,
    pub name: String,
    /// Resolved IP address used for HTTP API calls (pause/resume).
    /// Falls back to the `LXC_<STACKNAME_UPPER>_IP` env var, then the default.
    pub ip: String,
    pub status: String,
    pub cpu: f64,
    pub ram: Option<String>,
}

impl LxcNode {
    fn resolve_ip(stack_name: &str, default: &str) -> String {
        let key = format!("LXC_{}_IP", stack_name.replace('-', "_").to_uppercase());
        std::env::var(&key).unwrap_or_else(|_| default.to_string())
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct BackupStack {
    pub repo: String,
    pub schedule: String,
    pub last_run: String,
    pub duration: String,
    pub size_raw: String,
    pub size_dedup: String,
    pub snapshots: u32,
}

pub struct App {
    pub tab: usize,
    /// Live status lines from the backup orchestrator thread.
    pub backup_status: Vec<String>,
    /// True while a backup run is in progress.
    pub backup_running: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            tab: 0,
            backup_status: Vec::new(),
            backup_running: false,
        }
    }

    pub fn lxc_nodes(&self) -> Vec<LxcNode> {
        vec![
            LxcNode {
                id: 101,
                name: "lxc-cloudflared".to_string(),
                ip: LxcNode::resolve_ip("cloudflared", "10.0.1.101"),
                status: "RUN".to_string(),
                cpu: 3.0,
                ram: Some("128/512 MB".to_string()),
            },
            LxcNode {
                id: 102,
                name: "lxc-downloader".to_string(),
                ip: LxcNode::resolve_ip("downloader", "10.0.1.102"),
                status: "RUN".to_string(),
                cpu: 8.0,
                ram: Some("210/512 MB".to_string()),
            },
            LxcNode {
                id: 103,
                name: "lxc-gateway".to_string(),
                ip: LxcNode::resolve_ip("gateway", "10.0.1.103"),
                status: "RUN".to_string(),
                cpu: 22.0,
                ram: Some("380/1024 MB".to_string()),
            },
            LxcNode {
                id: 104,
                name: "lxc-media".to_string(),
                ip: LxcNode::resolve_ip("media", "10.0.1.104"),
                status: "STOP".to_string(),
                cpu: 0.0,
                ram: Some("--/1024 MB".to_string()),
            },
            LxcNode {
                id: 105,
                name: "lxc-monitoring".to_string(),
                ip: LxcNode::resolve_ip("monitoring", "10.0.1.105"),
                status: "RUN".to_string(),
                cpu: 11.0,
                ram: Some("290/512 MB".to_string()),
            },
            LxcNode {
                id: 106,
                name: "lxc-paperless".to_string(),
                ip: LxcNode::resolve_ip("paperless", "10.0.1.106"),
                status: "RUN".to_string(),
                cpu: 17.0,
                ram: Some("640/1024 MB".to_string()),
            },
        ]
    }

    pub fn backup_stack(&self) -> BackupStack {
        BackupStack {
            repo: "/mnt/backup/restic".to_string(),
            schedule: "daily @ 03:00".to_string(),
            last_run: "2026-05-26 03:00".to_string(),
            duration: "4m 32s".to_string(),
            size_raw: "87.4 GB (raw)".to_string(),
            size_dedup: "14.2 GB (dedup, 83.7% savings)".to_string(),
            snapshots: 42,
        }
    }
}
