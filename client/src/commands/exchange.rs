//! One request/response exchange over the host link: what is printed for each
//! frame the host sends, and when the exchange counts as finished.

use super::*;
use homelab_proto::{FleetState, RpcResponse};

pub(crate) type Tx = futures_util::stream::SplitSink<homelab_client::link::WsStream, Message>;
pub(crate) type Outcome = (bool, Option<FleetState>, Option<RpcResponse>);

/// What the frame loop does after one frame was handled.
pub(crate) enum Flow {
    /// Check whether the exchange is complete, then read the next frame.
    Next,
    /// Read the next frame without checking (an answer's own RpcDone).
    Skip,
    /// The exchange is over.
    Return(Box<Outcome>),
}

/// The state of one exchange.
pub(crate) struct Exchange<'a> {
    pub(crate) host: &'a str,
    pub(crate) req: RpcRequest,
    pub(crate) echo_reply: bool,
    pub(crate) quiet: bool,
    pub(crate) awaits_payload: bool,
    pub(crate) is_ping: bool,
    pub(crate) names_builds: bool,
    pub(crate) payload_seen: bool,
    pub(crate) done: Option<bool>,
    pub(crate) answer_pending: u32,
    pub(crate) sent: bool,
    pub(crate) fleet_seen: Option<FleetState>,
}

impl<'a> Exchange<'a> {
    fn new(host: &'a str, command: Command, echo_reply: bool) -> Self {
        // F303 (see REGISTER.md): the size guard lives HERE, where every
        // command passes, because the property is "any message can outgrow the
        // link," not "the host binary can."
        if let Some(why) = serde_json::to_vec(&command)
            .ok()
            .and_then(|v| homelab_client::version::too_large(v.len()))
        {
            die(&why);
        }
        Exchange {
            host,
            // Commands whose real payload arrives as a separate broadcast frame
            // (Config) may see RpcDone first — wait for the payload before exiting.
            awaits_payload: matches!(command, Command::GetConfig | Command::GetState),
            is_ping: matches!(command, Command::Ping),
            // A UI step prints only its own answer and notes, never the host's
            // broadcasts (log lines, transfers, the fleet snapshot, questions).
            quiet: homelab_client::ui_cli::quiet_line(&command),
            // fix-141: ping and status also name the client's own build.
            names_builds: matches!(command, Command::Ping | Command::Status),
            // The request is deliberately NOT sent yet: it goes out only after
            // the host has said which version it is. See `on_hello`.
            req: RpcRequest { id: 1, command },
            echo_reply,
            payload_seen: false,
            done: None,
            // fix-66 (host-questions-unanswerable, 2026-09-27): each answer
            // sent gets its own RpcDone, which would otherwise be read as the
            // whole exchange's result; `answer_pending` tells those apart
            // from the command's own reply.
            answer_pending: 0,
            sent: false,
            fleet_seen: None,
        }
    }

    async fn handle(&mut self, tx: &mut Tx, msg: ServerMsg) -> Flow {
        match msg {
            ServerMsg::Ask { .. } if self.quiet => {}
            ServerMsg::Ask {
                id,
                op,
                step,
                what,
                if_allowed,
                if_stopped,
                boot,
                wait_s,
            } => {
                let q = Question {
                    id,
                    op,
                    step,
                    what,
                    if_allowed,
                    if_stopped,
                    boot,
                    wait_s,
                };
                return self.on_ask(tx, q).await;
            }
            ServerMsg::Hello {
                version,
                proto,
                build,
            } => return self.on_hello(tx, version, proto, build).await,
            ServerMsg::Log { .. } | ServerMsg::Transfer { .. } | ServerMsg::State(_)
                if self.quiet => {}
            ServerMsg::Log {
                level, source, msg, ..
            } => print_log(level, &source, &msg),
            ServerMsg::Transfer {
                label, done, total, ..
            } => print_transfer(&label, done, total),
            // fix-240: a question was answered (here, in another session,
            // in Live view or the dashboard) or its wait ran out.
            ServerMsg::AskSettled { .. } if self.quiet => {}
            ServerMsg::AskSettled { id, how, .. } => {
                eprintln!("  question {} settled: {}", id, how);
            }
            // feat-platform-10: only the attached dashboard gets UI steps.
            ServerMsg::Ui { .. } => {}
            // Live view: the dashboard holds this CLI's UI step ("paused by
            // the viewer …"); on stderr, so `--json` stays one JSON answer.
            ServerMsg::UiNote { note } => {
                eprintln!("{}● {}{}", C_YELLOW, note, C_RESET);
            }
            ServerMsg::Config(view) => {
                self.payload_seen = true;
                print_config(&view);
            }
            ServerMsg::State(fleet) => {
                println!(
                    "{}fleet: {} stack(s) managed{}",
                    C_DIM,
                    fleet.stacks.len(),
                    C_RESET
                );
                self.payload_seen = true;
                self.fleet_seen = Some(*fleet);
            }
            ServerMsg::RpcDone(resp) => return self.on_rpc_done(resp),
        }
        Flow::Next
    }

    fn finished(&self) -> bool {
        self.done.is_some()
            && homelab_client::rpc_can_exit(self.awaits_payload, self.payload_seen, true)
    }
}

/// The whole exchange: connect, wait for the host's Hello, send the command,
/// print what comes back until the command is done.
pub(crate) async fn rpc_exchange(
    host: &str,
    token: &str,
    command: Command,
    echo_reply: bool,
) -> Outcome {
    let mut ex = Exchange::new(host, command, echo_reply);
    // fix-67: the pin, the frame ceiling and the version gate live in one
    // place, shared with the TUI, whose own copy had drifted from this one.
    let link = homelab_client::link::connect(
        host,
        token,
        REPO_PIN.get().and_then(|p| p.as_deref()),
        homelab_client::repo_config::built_in_pin(),
    )
    .await
    .unwrap_or_else(|e| die(&e));
    announce_pin(&link.pinned, ex.quiet);
    let (mut tx, mut rx) = link.ws.split();
    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(server_msg) = serde_json::from_str::<ServerMsg>(&text) else {
            continue;
        };
        match ex.handle(&mut tx, server_msg).await {
            Flow::Return(outcome) => return *outcome,
            Flow::Skip => continue,
            Flow::Next => {}
        }
        if ex.finished() {
            println!("{}✓ ok{}", C_GREEN, C_RESET);
            return (true, ex.fleet_seen, None);
        }
    }
    eprintln!(
        "{}✗ connection closed before RPC completed{}",
        C_RED, C_RESET
    );
    (false, ex.fleet_seen, None)
}
