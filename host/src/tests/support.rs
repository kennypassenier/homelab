//! Helpers shared by the host tests: loopback servers, handlers and small fixtures.

use crate::*;

pub(crate) fn parsed(text: &str) -> (serde_json::Value, RpcRequest) {
    let value: serde_json::Value = serde_json::from_str(text).unwrap();
    let req: RpcRequest = serde_json::from_value(value.clone()).unwrap();
    (value, req)
}

/// Load a config from `raw` through a file of its own. Tests run as
/// parallel threads of one process, so neither a shared path nor
/// `HOMELAB_CONFIG` may be used (rust-code-hygiene, 2026-09-27: one test
/// set that variable and wrote a fixed /tmp path while others ran).
pub(crate) fn config_from_text(raw: &str) -> Config {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "homelab-host-test-{}-{}.toml",
        std::process::id(),
        N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::write(&path, raw).unwrap();
    let cfg = load_config_from(path.display().to_string());
    let _ = std::fs::remove_file(&path);
    cfg
}

/// The daemon's shared state around `config`, as `main` builds it.
pub(crate) fn test_state(config: Config) -> AppState {
    let (log_tx, _) = broadcast::channel(64);
    AppState::new(config, log_tx)
}

/// A session over a real socket on a free local port, with `handler` in
/// place of `handle_rpc`. Returns the address to connect to.
pub(crate) async fn serve_on_loopback<H, Fut>(handler: H) -> SocketAddr
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let config = config_from_text("token = \"0123456789abcdef0123\"\n");
    serve_state_on_loopback(test_state(config), handler).await
}

/// [`serve_on_loopback`] around a state the test built itself.
pub(crate) async fn serve_state_on_loopback<H, Fut>(state: AppState, handler: H) -> SocketAddr
where
    H: Fn(AppState, RpcRequest) -> Fut + Clone + Send + Sync + 'static,
    Fut: std::future::Future<Output = RpcResponse> + Send + 'static,
{
    let app =
        Router::new()
            .route(
                "/ws",
                get(
                    move |ws: WebSocketUpgrade,
                          State(st): State<AppState>,
                          headers: HeaderMap| async move {
                        // The tests that predate arch-tokens send no header
                        // and ran as the one token there was: scope all.
                        let who = identify(
                            headers.get("authorization").and_then(|v| v.to_str().ok()),
                            &st.config.token,
                            &st.config.tokens,
                        )
                        .unwrap_or(Identity {
                            name: "legacy".into(),
                            scope: homelab_proto::Scope::All,
                        });
                        ws.on_upgrade(move |s| serve_ws(s, st, who, handler))
                    },
                ),
            )
            .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    addr
}

/// Stands in for a deploy whose service check reaches a question: it asks
/// through the real `LiveAsker` and reports the answer it got. `Answer`
/// goes to the real handler, which is what delivers it.
pub(crate) async fn asking_handler(st: AppState, req: RpcRequest) -> RpcResponse {
    asking_handler_with(st, req, false).await
}

/// fix-240: [`asking_handler`] that also raises the question as a notice
/// (the test's state directory is its own).
pub(crate) async fn notifying_asking_handler(st: AppState, req: RpcRequest) -> RpcResponse {
    asking_handler_with(st, req, true).await
}

pub(crate) async fn asking_handler_with(
    st: AppState,
    req: RpcRequest,
    notify: bool,
) -> RpcResponse {
    if matches!(req.command, Rpc::Answer { .. } | Rpc::AnswerOpen { .. }) {
        return handle_rpc(&st, req).await;
    }
    let asker = LiveAsker {
        state: &st,
        timeout_s: 5,
        notify,
    };
    let q = homelab_core::ask::Question {
        op: "deploy-gateway".into(),
        step: "service checks".into(),
        what: "routes went from 29 to 28".into(),
        if_allowed: "the deploy goes on".into(),
        if_stopped: "the deploy stops".into(),
    };
    let answer = homelab_core::ask::Asker::ask(&asker, &q).await;
    RpcResponse {
        id: req.id,
        ok: answer.may_continue(),
        message: format!("{:?}", answer),
        deferred: None,
    }
}

/// A handler shaped like the real `Rpc::BackupCalendar`/`Rpc::GetBackups`
/// arms (fix-180): a cache miss kicks the slow restic read into a
/// detached background task and answers "not read yet" at once, instead
/// of awaiting it. Everything else goes to the real `handle_rpc`.
pub(crate) async fn cache_answering_handler(st: AppState, req: RpcRequest) -> RpcResponse {
    if let Rpc::BackupCalendar { .. } = req.command {
        let cache = st.snapshot_cache.clone();
        tokio::spawn(async move {
            // Stands in for a real, slow `restic snapshots` over rclone.
            let exec = SlowMockExecutor::new(Duration::from_secs(2));
            cache
                .refresh(
                    &exec,
                    &homelab_core::ops::backup::BackupCfg::default(),
                    "gateway",
                    1,
                )
                .await;
        });
        return RpcResponse {
            id: req.id,
            ok: true,
            message: "not read yet".into(),
            deferred: None,
        };
    }
    handle_rpc(&st, req).await
}

/// `MockExecutor` with every `run` delayed, so a test can stand in for a
/// restic read slow enough to matter without a real restic binary.
pub(crate) struct SlowMockExecutor {
    pub(crate) inner: homelab_core::mock::MockExecutor,
    pub(crate) delay: Duration,
}

impl SlowMockExecutor {
    pub(crate) fn new(delay: Duration) -> Self {
        Self {
            inner: homelab_core::mock::MockExecutor::new(),
            delay,
        }
    }
}

#[async_trait::async_trait]
impl homelab_core::executor::Executor for SlowMockExecutor {
    async fn run(
        &self,
        cmd: &homelab_core::executor::Cmd,
    ) -> Result<homelab_core::executor::CmdOutput, homelab_core::error::CoreError> {
        tokio::time::sleep(self.delay).await;
        self.inner.run(cmd).await
    }
    async fn read_file(&self, path: &str) -> Result<String, homelab_core::error::CoreError> {
        self.inner.read_file(path).await
    }
    async fn write_file(
        &self,
        path: &str,
        content: &str,
        mode: u32,
    ) -> Result<(), homelab_core::error::CoreError> {
        self.inner.write_file(path, content, mode).await
    }
    async fn sleep_ms(&self, ms: u64) {
        self.inner.sleep_ms(ms).await
    }
}

pub(crate) fn sha_hex(t: &str) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(t.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

/// A state with a legacy token and two scoped ones, its audit log in a
/// directory of its own.
pub(crate) fn scoped_state(tag: &str) -> (AppState, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("homelab-scope-{}-{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let raw = format!(
        "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n\
         [[tokens]]\nname = \"dash-read\"\nscope = \"read\"\nsha256 = \"{}\"\n\
         [[tokens]]\nname = \"dash-operate\"\nscope = \"operate\"\nsha256 = \"{}\"\n\
         [[tokens]]\nname = \"dash-all\"\nscope = \"all\"\nsha256 = \"{}\"\n",
        dir.display(),
        sha_hex("read-token-aaaaaaaaaaaaaaaa"),
        sha_hex("operate-token-bbbbbbbbbbbbbb"),
        sha_hex("all-token-cccccccccccccccccc"),
    );
    (test_state(config_from_text(&raw)), dir)
}

/// Opens a session with `token`, sends `commands` in order and returns
/// the replies in the order they ARRIVED, as (id, ok, message).
pub(crate) async fn session_replies(
    addr: SocketAddr,
    token: &str,
    commands: Vec<(u64, Rpc)>,
) -> Vec<(u64, bool, String)> {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
    let mut request = format!("ws://{}/ws", addr).into_client_request().unwrap();
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {}", token).parse().unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(request)
        .await
        .expect("connect");
    let (mut tx, mut rx) = ws.split();
    let n = commands.len();
    for (id, command) in commands {
        let req = RpcRequest { id, command };
        tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
            .await
            .unwrap();
    }
    let mut out = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(frame)) = rx.next().await {
            if let WsMsg::Text(t) = frame
                && let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t)
            {
                out.push((r.id, r.ok, r.message));
                if out.len() == n {
                    return;
                }
            }
        }
    })
    .await;
    out
}

/// Answers every command with "ran <name>"; a non-read command takes a
/// moment, so a read sent after it can overtake it only beside the queue.
pub(crate) async fn naming_handler(_st: AppState, req: RpcRequest) -> RpcResponse {
    if !req.command.is_read_only() {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    RpcResponse {
        id: req.id,
        ok: true,
        message: format!("ran {}", req.command.name()),
        deferred: None,
    }
}

/// Answers with the token name the request runs under.
pub(crate) async fn by_handler(_st: AppState, req: RpcRequest) -> RpcResponse {
    RpcResponse {
        id: req.id,
        ok: true,
        message: requested_by().unwrap_or_else(|| "<none>".into()),
        deferred: None,
    }
}

/// Asks a question, and while it waits floods the broadcast channel far
/// past its capacity, so a slow reader lags past the question.
pub(crate) async fn flooding_asker(st: AppState, req: RpcRequest) -> RpcResponse {
    if matches!(req.command, Rpc::Answer { .. }) {
        return handle_rpc(&st, req).await;
    }
    // Spawned before the question is sent, so on this single-threaded
    // test runtime it runs as soon as the asker waits: after the Ask is
    // in the channel, before the session's forwarder has taken it out.
    let flood = st.log_tx.clone();
    tokio::spawn(async move {
        for i in 0..500 {
            let _ = flood.send(ServerMsg::Log {
                req: None,
                step: None,
                ts: None,
                by: None,
                plan: None,
                level: homelab_proto::LogLevel::Debug,
                source: "HOST".into(),
                msg: format!("noise {}", i),
            });
        }
    });
    asking_handler(st, req).await
}

/// A state whose `state_dir` is a fresh directory of its own, holding a
/// self-update marker armed at `armed_at`.
pub(crate) fn state_with_marker(tag: &str, armed_at: u64) -> (AppState, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("homelab-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let toml = dir.join("host.toml");
    std::fs::write(
        &toml,
        format!(
            "token = \"0123456789abcdef0123\"\nstate_dir = \"{}\"\n",
            dir.display()
        ),
    )
    .unwrap();
    let marker = dir.join("selfupdate.pending");
    std::fs::write(
        &marker,
        format!("{{\"to_version\":\"9.9.9\",\"armed_at\":{}}}\n", armed_at),
    )
    .unwrap();
    (
        test_state(load_config_from(toml.to_string_lossy().into_owned())),
        marker,
    )
}

/// One Ping over a real session, answered.
pub(crate) async fn ping_over(addr: SocketAddr) {
    use tokio_tungstenite::tungstenite::Message as WsMsg;
    let (ws, _) = tokio_tungstenite::connect_async(format!("ws://{}/ws", addr))
        .await
        .expect("connect to the loopback session");
    let (mut tx, mut rx) = ws.split();
    let req = RpcRequest {
        id: 1,
        command: Rpc::Ping,
    };
    tx.send(WsMsg::Text(serde_json::to_string(&req).unwrap().into()))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while let Some(Ok(WsMsg::Text(t))) = rx.next().await {
            if let Ok(ServerMsg::RpcDone(r)) = serde_json::from_str::<ServerMsg>(&t) {
                assert!(r.ok);
                return;
            }
        }
        panic!("the connection closed before the ping was answered");
    })
    .await
    .expect("the ping was answered");
    // The acceptance runs after the answer is queued; give it its turn.
    tokio::time::sleep(Duration::from_millis(200)).await;
}

/// One plain HTTP GET against `addr`, with an optional bearer token;
/// returns the status code and the body.
pub(crate) async fn http_get(addr: SocketAddr, path: &str, token: Option<&str>) -> (u16, String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let auth = token
        .map(|t| format!("Authorization: Bearer {}\r\n", t))
        .unwrap_or_default();
    s.write_all(
        format!(
            "GET {} HTTP/1.1\r\nHost: x\r\n{}Connection: close\r\n\r\n",
            path, auth
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    let mut out = String::new();
    s.read_to_string(&mut out).await.unwrap();
    let code = out
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .unwrap_or(0);
    let body = out.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (code, body)
}

pub(crate) fn stray_clone() -> WatchedBackup {
    WatchedBackup {
        name: "opnsense-config".into(),
        rclone_path: "rclone:gdrive:homelab-backups/OPNSense-backups".into(),
        max_age_hours: 26,
    }
}

/// A `MakeWriter` into a shared buffer, to read what the journal gets.
#[derive(Clone, Default)]
pub(crate) struct Captured(pub(crate) Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
