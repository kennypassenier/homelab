//! What the exchange does for the frames that start or finish it: Hello,
//! questions and the final reply.

use super::*;
use homelab_proto::RpcResponse;

impl Exchange<'_> {
    pub(crate) fn outcome(&mut self, ok: bool, resp: Option<RpcResponse>) -> Flow {
        Flow::Return(Box::new((ok, self.fleet_seen.take(), resp)))
    }

    pub(crate) async fn on_hello(
        &mut self,
        tx: &mut Tx,
        version: String,
        proto: u32,
        build: Option<String>,
    ) -> Flow {
        self.announce_host(&version, proto, build.as_deref());
        // The client refuses to send a mutating command to an older
        // host, and says which command fixes it (the 2026-08-31
        // data_mounts incident; the rule is in `link`) — the host
        // ignoring a field it predates is a separate, still-live
        // risk this fix does not touch.
        if let Some(why) = homelab_client::link::refuse_older_host(&self.req.command, &version) {
            die(&why);
        }
        // fix-199 (replaces fix-105's refusal): the other direction
        // — a stale client — no longer refuses anything; the host's
        // own field-keeping rule makes sending safe, so this is only
        // ever a notice.
        if let Some(notice) = homelab_client::link::older_client_notice(&version) {
            eprintln!("{}! {}{}", C_YELLOW, notice, C_RESET);
        }
        if !self.sent {
            tx.send(Message::Text(
                serde_json::to_string(&self.req).unwrap().into(),
            ))
            .await
            .unwrap_or_else(|e| die(&format!("send: {}", e)));
            self.sent = true;
        }
        Flow::Next
    }

    pub(crate) fn announce_host(&self, version: &str, proto: u32, build: Option<&str>) {
        // fix-141 (expert panel 2026-09-27,
        // changes-reach-prod-without-ci): "v3.59.3" named the release
        // and a hand build alike; the build tells them apart.
        if !self.quiet {
            println!(
                "{}● HOST {} · proto {} — link up{}",
                C_GREEN,
                homelab_client::link::version_label(version, build),
                proto,
                C_RESET
            );
        }
        if self.names_builds {
            println!(
                "{}  client {}{}",
                C_DIM,
                homelab_client::link::version_label(
                    env!("CARGO_PKG_VERSION"),
                    Some(homelab_client::BUILD)
                ),
                C_RESET
            );
        }
        // Only on ping: which door was knocked on, and who said so.
        // The rest of the verbs stay quiet about it.
        if self.is_ping
            && let Some(src) = HOST_SOURCE.get()
        {
            println!("{}  via {} ({}){}", C_DIM, self.host, src, C_RESET);
        }
    }

    /// fix-66 (host-questions-unanswerable): answer a question the operation
    /// raised, pre-answered or typed, or leave it to time out.
    pub(crate) async fn on_ask(&mut self, tx: &mut Tx, q: Question) -> Flow {
        eprintln!(
            "{}? {} :: {} is waiting for a decision — {}{}",
            C_YELLOW, q.op, q.step, q.what, C_RESET
        );
        match choose_answer(&q) {
            Some(allow) => {
                let answer = RpcRequest {
                    id: self.req.id + self.answer_pending as u64 + 1,
                    command: Command::Answer {
                        id: q.id,
                        allow,
                        boot: q.boot,
                    },
                };
                let text = serde_json::to_string(&answer).unwrap();
                if tx.send(Message::Text(text.into())).await.is_ok() {
                    self.answer_pending += 1;
                }
            }
            // fix-240: a headless run cannot answer; it says how
            // to answer from elsewhere, and how long there is.
            None => eprintln!(
                "{}",
                homelab_client::answer::answer_elsewhere_hint(&q.op, q.wait_s)
            ),
        }
        Flow::Next
    }

    pub(crate) fn on_rpc_done(&mut self, resp: RpcResponse) -> Flow {
        // fix-66: an answer sent above is its own RPC and gets its
        // own RpcDone, which would otherwise be read as the whole
        // exchange's result — it arrives beside, not instead of,
        // the operation's own reply (the host runs it off the
        // queue, "beside" it as the register entry says).
        if self.answer_pending > 0 && resp.id != self.req.id {
            self.answer_pending -= 1;
            if !resp.ok {
                eprintln!(
                    "{}✗ answer not delivered: {}{}",
                    C_RED, resp.message, C_RESET
                );
            }
            return Flow::Skip;
        }
        if !self.echo_reply {
            return self.outcome(resp.ok, Some(resp));
        }
        // fix-103: a fleet check is printed group by group, each in
        // its own colour, instead of one red block.
        if resp.message.starts_with("fleet check:") {
            print_check(&resp.message, resp.ok);
            return self.outcome(resp.ok, Some(resp));
        }
        if !resp.ok {
            println!("{}✗ {}{}", C_RED, resp.message, C_RESET);
            return self.outcome(false, Some(resp));
        }
        if homelab_client::rpc_can_exit(self.awaits_payload, self.payload_seen, true) {
            println!("{}✓ {}{}", C_GREEN, resp.message, C_RESET);
            return self.outcome(true, Some(resp));
        }
        self.done = Some(true);
        Flow::Next
    }
}
