//! Host RPC plumbing: connect, send one command, stream its output, exit by result.

use super::*;

pub(crate) async fn rpc(host: &str, token: &str, command: Command) {
    let ok = rpc_with(host, token, command).await;
    std::process::exit(if ok { 0 } else { 1 });
}

/// The version a host announces in its Hello, or None when it cannot be
/// reached or says nothing within a few seconds.
pub(crate) async fn host_version(host: &str, token: &str) -> Option<String> {
    let link = homelab_client::link::connect(
        host,
        token,
        REPO_PIN.get().and_then(|p| p.as_deref()),
        homelab_client::repo_config::built_in_pin(),
    )
    .await
    .ok()?;
    let (_, mut rx) = link.ws.split();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(Ok(Message::Text(text))) = rx.next().await {
            if let Ok(ServerMsg::Hello { version, .. }) = serde_json::from_str(&text) {
                return Some(version);
            }
        }
        None
    })
    .await
    .ok()
    .flatten()
}

/// How long `wait_for_updated_host` watches, in seconds. The old daemon may
/// finish a running operation first (up to 60 s, fix-52), then the restart
/// and a possible rollback follow.
const WAIT_S: u64 = 150;
/// How long a host that was never seen down has to keep answering before a
/// restart without a version counts as done.
const UNSEEN_RESTART_S: u64 = 10;

/// What `wait_for_updated_host` has seen so far.
struct HostWatch<'a> {
    host: &'a str,
    token: &'a str,
    expected: Option<String>,
    started: std::time::Instant,
    seen_down: bool,
    last: Option<String>,
}

impl HostWatch<'_> {
    /// host-restart-wait-1 (measured 2026-10-29 on the real host: `homelab
    /// host restart` waited its whole 150 s and failed): the daemon
    /// restarts within about 2 s, faster than a 3 s look, so "went down"
    /// was never seen. With no version to wait for, look every 300 ms.
    fn quick(&self) -> bool {
        self.expected.is_none()
    }

    /// One look at the host; `Some(result)` once the wait is decided.
    async fn look(&mut self) -> Option<bool> {
        use homelab_client::release::{AfterUpdate, after_update};
        let pause = if self.quick() { 300 } else { 3000 };
        tokio::time::sleep(std::time::Duration::from_millis(pause)).await;
        let answered = host_version(self.host, self.token).await;
        if answered.is_none() {
            self.seen_down = true;
        } else {
            self.last = answered.clone();
            // A host that has answered throughout for `UNSEEN_RESTART_S` has
            // restarted between two looks (the restart was accepted by the host).
            if self.quick()
                && !self.seen_down
                && self.started.elapsed() >= std::time::Duration::from_secs(UNSEEN_RESTART_S)
            {
                println!(
                    "{}✓ the host answers; its restart was faster than a look{}",
                    C_GREEN, C_RESET
                );
                return Some(rpc_with(self.host, self.token, Command::Ping).await);
            }
        }
        match after_update(
            self.expected.as_deref(),
            self.seen_down,
            answered.as_deref(),
        ) {
            AfterUpdate::Wait => None,
            AfterUpdate::Answered => Some(self.accepted().await),
            AfterUpdate::RolledBack(v) => {
                self.report_rolled_back(&v);
                Some(false)
            }
        }
    }

    /// The round trip that accepts the update on the host.
    async fn accepted(&self) -> bool {
        let ok = rpc_with(self.host, self.token, Command::Ping).await;
        if ok {
            println!(
                "{}✓ self-update accepted — the new host answered{}",
                C_GREEN, C_RESET
            );
        }
        ok
    }

    fn report_rolled_back(&self, came_back: &str) {
        eprintln!(
            "{}✗ the host came back as v{}, not v{}: the self-update was rolled back \
             :: `journalctl -u homelab-host` on pve says why the new binary failed{}",
            C_RED,
            came_back,
            self.expected.as_deref().unwrap_or("?"),
            C_RESET
        );
    }

    fn report_timeout(&self) {
        eprintln!(
            "{}✗ no confirmation within {} s (last answer: {}) :: the update is not accepted \
             until the new daemon answers a request; `homelab ping` retries that, \
             `journalctl -u homelab-host` on pve shows what it is doing{}",
            C_RED,
            WAIT_S,
            self.last
                .as_ref()
                .map(|v| format!("v{}", v))
                .unwrap_or_else(|| "none".into()),
            C_RESET
        );
    }
}

/// fix-121 (see REGISTER.md): after a self-update, watch the host come back.
/// `true` once the shipped version (`expected`; any restarted daemon when
/// unknown) has answered a ping, which is also what accepts the update on
/// the host; `false` when the old version came back (the rollback ran) or
/// nothing answered in time.
pub(crate) async fn wait_for_updated_host(
    host: &str,
    token: &str,
    expected: Option<String>,
) -> bool {
    println!(
        "{}… waiting up to {} s for the host to come back{}{}",
        C_DIM,
        WAIT_S,
        expected
            .as_deref()
            .map(|v| format!(" as v{}", v))
            .unwrap_or_default(),
        C_RESET
    );
    let started = std::time::Instant::now();
    let deadline = started + std::time::Duration::from_secs(WAIT_S);
    let mut watch = HostWatch {
        host,
        token,
        expected,
        started,
        seen_down: false,
        last: None,
    };
    while std::time::Instant::now() < deadline {
        if let Some(result) = watch.look().await {
            return result;
        }
    }
    watch.report_timeout();
    false
}

/// One command over the link; `true` when the host reported it done and ok.
/// T85 needed a caller that sends several commands in a row (one staged
/// binary per message, then the deploy), so the exit moved to `rpc`.
pub(crate) async fn rpc_with(host: &str, token: &str, command: Command) -> bool {
    rpc_collect(host, token, command).await.0
}

/// [`rpc_with`], also handing back the fleet snapshot a `GetState` answers
/// with (ask-8: `homelab apply` compares the stacks directory with it).
pub(crate) async fn rpc_collect(
    host: &str,
    token: &str,
    command: Command,
) -> (bool, Option<homelab_proto::FleetState>) {
    let (ok, fleet, _) = rpc_exchange(host, token, command, true).await;
    (ok, fleet)
}

/// fix-68: the host's reply handed back instead of printed, for a verb whose
/// reply is data to render (`today`).
pub(crate) async fn rpc_reply(
    host: &str,
    token: &str,
    command: Command,
) -> Option<homelab_proto::RpcResponse> {
    rpc_exchange(host, token, command, false).await.2
}
