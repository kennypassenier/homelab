use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::*,
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
};

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

    pub fn draw_dashboard(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Min(0),
            ])
            .split(frame.area());
        frame.render_widget(
            Paragraph::new(" >> HOST_MESH << ").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            chunks[0],
        );
        let spark =
            " CPU: ▃▄▅▃▄▅▄▃ 14%   RAM: ▄▅▄▅▄▅▄▅ 6.2/32 GB   DISK: ██████░░ 214/512 GB (42%) ";
        frame.render_widget(Paragraph::new(spark), chunks[1]);
        let sub_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(22)])
            .split(chunks[2]);
        draw_lxc_mesh_table(frame, self, sub_chunks[0]);
        draw_backup_status(frame, self.backup_stack(), sub_chunks[1]);
    }

    pub fn draw_lxc_nodes(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(frame.area());
        frame.render_widget(
            Paragraph::new(" >> LXC_CORE << ").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            chunks[0],
        );
        let sub_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(32)])
            .split(chunks[1]);
        draw_lxc_mesh_table(frame, self, sub_chunks[0]);
        draw_detail_view(frame, sub_chunks[1]);
    }

    pub fn draw_backups(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(frame.area());
        frame.render_widget(
            Paragraph::new(" >> BACKUP_ORCHESTRATOR << ").style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            chunks[0],
        );
        let bs = self.backup_stack();
        let info = format!(
            "Repo:     {}\nSchedule: {}\nStatus:   ✓ IDLE — next run in 21h 14m\nLast backup:\n    last run: {}  |  duration: {}\nTotal size:\n    raw: {}  →  dedup: {}\nSnapshots:\n    {}",
            bs.repo,
            bs.schedule,
            bs.last_run,
            bs.duration,
            bs.size_raw,
            bs.size_dedup,
            bs.snapshots
        );
        frame.render_widget(
            Paragraph::new(info).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            ),
            chunks[1],
        );
    }

    pub fn draw_storage(&self, frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new(" [ STORAGE TAB — PLACEHOLDER ] "),
            frame.area(),
        );
    }

    pub fn draw_hardware(&self, frame: &mut Frame) {
        frame.render_widget(
            Paragraph::new(" [ HARDWARE TAB — PLACEHOLDER ] "),
            frame.area(),
        );
    }
}

pub fn draw_lxc_mesh_table(frame: &mut Frame, app: &App, area: Rect) {
    let nodes = app.lxc_nodes();
    let header_style = Style::default().add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from(" STATUS ").style(header_style),
        Cell::from(" ID   CONTAINER ").style(header_style),
        Cell::from(" CPU    RAM      ").style(header_style),
    ]);
    let rows: Vec<Row> = nodes
        .iter()
        .map(|n| {
            let style = if n.status == "RUN" && n.cpu > 0.0 {
                Style::default().add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let status_str = format!(
                "{} {}",
                if n.status == "RUN" {
                    "● RUN"
                } else {
                    "○ STP"
                },
                n.status
            );
            Row::new(vec![
                Cell::from(status_str).style(style),
                Cell::from(format!(" {} {}", n.id, n.name)).style(Style::default()),
                Cell::from(format!(
                    "{:>3}%  {:?}",
                    n.cpu as u64,
                    if let Some(r) = &n.ram { r } else { "—" }
                ))
                .style(Style::default()),
            ])
        })
        .collect();
    let title = format!(" LXC_MESH :: {} NODES ", nodes.len());
    frame.render_widget(
        Table::new(
            rows,
            vec![
                Constraint::Length(7),
                Constraint::Min(16),
                Constraint::Length(14),
            ],
        )
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Double)
                .title(title.as_str())
                .style(Style::default()),
        ),
        area,
    );
}

pub fn draw_backup_status(frame: &mut Frame, bs: BackupStack, area: Rect) {
    let hints = vec![("u", "update now"), ("b", "run backup now")];
    let mut content = format!(
        "Current:  {}\nLatest:   {}  ● AVAILABLE\nChannel:  github releases\nStatus:   [IDLE]",
        bs.repo, "v1.5.0"
    );
    let hint_lines = hints
        .iter()
        .map(|(k, v)| format!("\n  [{}] {}", k, v))
        .collect::<String>();
    content.push_str(&hint_lines);

    frame.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" BACKUP_STATUS :: Restic "),
        ),
        area,
    );
}

pub fn draw_detail_view(frame: &mut Frame, area: Rect) {
    let content = r#"lxc-gateway                    
─────────────────────          
VMID:   103                     
Stack:  gateway                
Disk:   4.2/8 GB               
Cores:  2                      
RAM:    1024 MB                 
GPU:    ✗                      
TUN:    ✓                       
State:  ● RUNNING             
                                 
[ ] passthrough                "#;

    frame.render_widget(
        Paragraph::new(content).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(" DETAIL "),
        ),
        area,
    );
}
