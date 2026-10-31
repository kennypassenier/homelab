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
    use homelab_client::release::{AfterUpdate, after_update};
    // The old daemon may finish a running operation first (up to 60 s,
    // fix-52), then the restart and a possible rollback follow.
    const WAIT_S: u64 = 150;
    /// How long a host that was never seen down has to keep answering before a restart without a version counts as done.
    const UNSEEN_RESTART_S: u64 = 10;
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
    let mut seen_down = false;
    let mut last: Option<String> = None;
    while std::time::Instant::now() < deadline {
        // host-restart-wait-1 (measured 2026-10-29 on the real host: `homelab
        // host restart` waited its whole 150 s and failed): the daemon
        // restarts within about 2 s, faster than a 3 s look, so "went down"
        // was never seen. With no version to wait for, look every 300 ms; and
        // a host that has answered throughout for `UNSEEN_RESTART_S` has
        // restarted between two looks (the restart was accepted by the host).
        let quick = expected.is_none();
        tokio::time::sleep(std::time::Duration::from_millis(if quick {
            300
        } else {
            3000
        }))
        .await;
        let answered = host_version(host, token).await;
        if answered.is_none() {
            seen_down = true;
        } else {
            last = answered.clone();
            if quick
                && !seen_down
                && started.elapsed() >= std::time::Duration::from_secs(UNSEEN_RESTART_S)
            {
                println!(
                    "{}✓ the host answers; its restart was faster than a look{}",
                    C_GREEN, C_RESET
                );
                return rpc_with(host, token, Command::Ping).await;
            }
        }
        match after_update(expected.as_deref(), seen_down, answered.as_deref()) {
            AfterUpdate::Wait => continue,
            AfterUpdate::Answered => {
                // The round trip that accepts the update on the host.
                let ok = rpc_with(host, token, Command::Ping).await;
                if ok {
                    println!(
                        "{}✓ self-update accepted — the new host answered{}",
                        C_GREEN, C_RESET
                    );
                }
                return ok;
            }
            AfterUpdate::RolledBack(v) => {
                eprintln!(
                    "{}✗ the host came back as v{}, not v{}: the self-update was rolled back \
                     :: `journalctl -u homelab-host` on pve says why the new binary failed{}",
                    C_RED,
                    v,
                    expected.as_deref().unwrap_or("?"),
                    C_RESET
                );
                return false;
            }
        }
    }
    eprintln!(
        "{}✗ no confirmation within {} s (last answer: {}) :: the update is not accepted \
         until the new daemon answers a request; `homelab ping` retries that, \
         `journalctl -u homelab-host` on pve shows what it is doing{}",
        C_RED,
        WAIT_S,
        last.map(|v| format!("v{}", v))
            .unwrap_or_else(|| "none".into()),
        C_RESET
    );
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

pub(crate) async fn rpc_exchange(
    host: &str,
    token: &str,
    command: Command,
    echo_reply: bool,
) -> (
    bool,
    Option<homelab_proto::FleetState>,
    Option<homelab_proto::RpcResponse>,
) {
    use std::io::IsTerminal as _;
    let mut fleet_seen: Option<homelab_proto::FleetState> = None;
    // F303 (see REGISTER.md): the size guard lives HERE, where every
    // command passes, because the property is "any message can outgrow the
    // link," not "the host binary can."
    // to the property: any message can outgrow the link.
    if let Some(why) = serde_json::to_vec(&command)
        .ok()
        .and_then(|v| homelab_client::version::too_large(v.len()))
    {
        die(&why);
    }
    // Commands whose real payload arrives as a separate broadcast frame
    // (Config) may see RpcDone first — wait for the payload before exiting.
    let awaits_payload = matches!(command, Command::GetConfig | Command::GetState);
    let is_ping = matches!(command, Command::Ping);
    // A UI step prints only its own answer and notes, never the host's
    // broadcasts (log lines, transfers, the fleet snapshot, questions).
    let quiet = homelab_client::ui_cli::quiet_line(&command);
    // fix-141: ping and status also name the client's own build.
    let names_builds = matches!(command, Command::Ping | Command::Status);
    let mut payload_seen = false;
    let mut done: Option<bool> = None;
    // fix-66 (host-questions-unanswerable, 2026-09-27): the CLI could not
    // answer a question an operation started from it raised — "no answer
    // from here: run this from the TUI to decide" was the whole story.
    // Answers now run beside the host's queue (that part is built); this is
    // the client side of actually sending one. `PRE_ANSWER` (`--answer
    // allow|stop`, read once in `run`) answers every question without
    // waiting; otherwise, on a terminal, the same `[a] allow / [s] stop`
    // choice the TUI's focus window offers is asked here. Not a terminal
    // and no pre-answer: unchanged — printed, and left to time out as
    // Unattended. Each answer sent here gets its own RpcDone, which would
    // otherwise be read as the whole exchange's result; `answer_pending`
    // tells those apart from the command's own reply.
    let mut answer_pending: u32 = 0;
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
    match &link.pinned {
        Some(
            homelab_client::link::Pinned::BuiltIn(_) | homelab_client::link::Pinned::FromRepo(_),
        ) if quiet => {}
        // fix-149: nothing trusted on first use; say where the pin came from.
        Some(homelab_client::link::Pinned::BuiltIn(fp)) => eprintln!(
            "{}● pinned host certificate SHA256:{}, the one this client was built with{}",
            C_YELLOW, fp, C_RESET
        ),
        Some(homelab_client::link::Pinned::FromRepo(fp)) => eprintln!(
            "{}● pinned host certificate SHA256:{} from {}{}",
            C_YELLOW,
            fp,
            homelab_client::repo_config::REPO_FILE,
            C_RESET
        ),
        Some(homelab_client::link::Pinned::FirstUse(fp)) => {
            eprintln!(
                "{}● pinned host certificate SHA256:{}{}",
                C_YELLOW, fp, C_RESET
            );
            eprintln!(
                "{}  verify this matches the fingerprint the host printed at boot{}",
                C_DIM, C_RESET
            );
        }
        None => {}
    }
    let (mut tx, mut rx) = link.ws.split();

    // The request is deliberately NOT sent yet: it goes out only after the
    // host has said which version it is. See the Hello arm below.
    let req = RpcRequest { id: 1, command };
    let mut sent = false;

    while let Some(Ok(msg)) = rx.next().await {
        let Message::Text(text) = msg else { continue };
        let Ok(server_msg) = serde_json::from_str::<ServerMsg>(&text) else {
            continue;
        };
        match server_msg {
            // T69: the command line is not a place to answer a question —
            // there is no prompt to draw and the operator may not even be
            // watching. Print it and let the host's timeout do the rest,
            // which lands on Unattended rather than on a guess.
            ServerMsg::Ask { .. } if quiet => {}
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
                eprintln!(
                    "{}? {} :: {} is waiting for a decision — {}{}",
                    C_YELLOW, op, step, what, C_RESET
                );
                let pre_answer = PRE_ANSWER.get().copied().flatten();
                let allow = match homelab_client::answer::choose(
                    pre_answer,
                    std::io::stdin().is_terminal(),
                ) {
                    homelab_client::answer::AnswerChoice::PreAnswered(allow) => {
                        eprintln!(
                            "  --answer {} :: {}",
                            if allow { "allow" } else { "stop" },
                            if allow { &if_allowed } else { &if_stopped }
                        );
                        Some(allow)
                    }
                    homelab_client::answer::AnswerChoice::Prompt => {
                        eprintln!("    [a] allow — {}", if_allowed);
                        eprintln!("    [s] stop  — {}", if_stopped);
                        loop {
                            match homelab_client::answer::parse_prompt_answer(&read_typed("  ")) {
                                Some(allow) => break Some(allow),
                                None => eprintln!("  type 'a' or 's'"),
                            }
                        }
                    }
                    homelab_client::answer::AnswerChoice::TimesOut => None,
                };
                match allow {
                    Some(allow) => {
                        let answer = RpcRequest {
                            id: req.id + answer_pending as u64 + 1,
                            command: Command::Answer { id, allow, boot },
                        };
                        if tx
                            .send(Message::Text(
                                serde_json::to_string(&answer).unwrap().into(),
                            ))
                            .await
                            .is_ok()
                        {
                            answer_pending += 1;
                        }
                    }
                    // fix-240: a headless run cannot answer; it says how
                    // to answer from elsewhere, and how long there is.
                    None => eprintln!(
                        "{}",
                        homelab_client::answer::answer_elsewhere_hint(&op, wait_s)
                    ),
                }
            }
            ServerMsg::Hello {
                version,
                proto,
                build,
            } => {
                // fix-141 (expert panel 2026-09-27,
                // changes-reach-prod-without-ci): "v3.59.3" named the release
                // and a hand build alike; the build tells them apart.
                if !quiet {
                    println!(
                        "{}● HOST {} · proto {} — link up{}",
                        C_GREEN,
                        homelab_client::link::version_label(&version, build.as_deref()),
                        proto,
                        C_RESET
                    );
                }
                if names_builds {
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
                if is_ping && let Some(src) = HOST_SOURCE.get() {
                    println!("{}  via {} ({}){}", C_DIM, host, src, C_RESET);
                }
                // The client refuses to send a mutating command to an older
                // host, and says which command fixes it (the 2026-08-31
                // data_mounts incident; the rule is in `link`) — the host
                // ignoring a field it predates is a separate, still-live
                // risk this fix does not touch.
                if let Some(why) = homelab_client::link::refuse_older_host(&req.command, &version) {
                    die(&why);
                }
                // fix-199 (replaces fix-105's refusal): the other direction
                // — a stale client — no longer refuses anything; the host's
                // own field-keeping rule makes sending safe, so this is only
                // ever a notice.
                if let Some(notice) = homelab_client::link::older_client_notice(&version) {
                    eprintln!("{}! {}{}", C_YELLOW, notice, C_RESET);
                }
                if !sent {
                    tx.send(Message::Text(serde_json::to_string(&req).unwrap().into()))
                        .await
                        .unwrap_or_else(|e| die(&format!("send: {}", e)));
                    sent = true;
                }
            }
            ServerMsg::Log { .. } | ServerMsg::Transfer { .. } | ServerMsg::State(_) if quiet => {}
            ServerMsg::Log {
                level, source, msg, ..
            } => {
                let color = match level {
                    LogLevel::Debug => C_DIM,
                    LogLevel::Info => C_CYAN,
                    LogLevel::Warn => C_YELLOW,
                    LogLevel::Error => C_RED,
                };
                println!("{}{:<5}{} {}", color, source, C_RESET, msg);
            }
            ServerMsg::Transfer {
                label, done, total, ..
            } => {
                let total_str = total.map(|t| format!("/{}", t)).unwrap_or_default();
                println!(
                    "{}⇅ {} {}{} bytes{}",
                    C_DIM, label, done, total_str, C_RESET
                );
            }
            // fix-240: a question was answered (here, in another session,
            // in Live view or the dashboard) or its wait ran out.
            ServerMsg::AskSettled { .. } if quiet => {}
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
                payload_seen = true;
                // G8: plain-text dump for the CLI (`homelab config`).
                let hour = view
                    .backup_hour
                    .map(|h| format!("{:02}:00", h))
                    .unwrap_or_else(|| "off".into());
                println!("nightly run : {}", hour);
                println!(
                    "webhook     : {}",
                    view.notify_webhook.as_deref().unwrap_or("off")
                );
                for (i, t) in view.retention.iter().enumerate() {
                    let span = t
                        .span_days
                        .map(|d| format!("for {} days", d))
                        .unwrap_or_else(|| "forever".into());
                    println!("retention {} : every {} days {}", i + 1, t.every_days, span);
                }
            }
            ServerMsg::State(fleet) => {
                println!(
                    "{}fleet: {} stack(s) managed{}",
                    C_DIM,
                    fleet.stacks.len(),
                    C_RESET
                );
                payload_seen = true;
                fleet_seen = Some(*fleet);
            }
            ServerMsg::RpcDone(resp) => {
                // fix-66: an answer sent above is its own RPC and gets its
                // own RpcDone, which would otherwise be read as the whole
                // exchange's result — it arrives beside, not instead of,
                // the operation's own reply (the host runs it off the
                // queue, "beside" it as the register entry says).
                if answer_pending > 0 && resp.id != req.id {
                    answer_pending -= 1;
                    if !resp.ok {
                        eprintln!(
                            "{}✗ answer not delivered: {}{}",
                            C_RED, resp.message, C_RESET
                        );
                    }
                    continue;
                }
                if !echo_reply {
                    return (resp.ok, fleet_seen, Some(resp));
                }
                // fix-103: a fleet check is printed group by group, each in
                // its own colour, instead of one red block.
                if resp.message.starts_with("fleet check:") {
                    print_check(&resp.message, resp.ok);
                    return (resp.ok, fleet_seen, Some(resp));
                }
                if !resp.ok {
                    println!("{}✗ {}{}", C_RED, resp.message, C_RESET);
                    return (false, fleet_seen, Some(resp));
                }
                if homelab_client::rpc_can_exit(awaits_payload, payload_seen, true) {
                    println!("{}✓ {}{}", C_GREEN, resp.message, C_RESET);
                    return (true, fleet_seen, Some(resp));
                }
                done = Some(true);
            }
        }
        if done.is_some() && homelab_client::rpc_can_exit(awaits_payload, payload_seen, true) {
            println!("{}✓ ok{}", C_GREEN, C_RESET);
            return (true, fleet_seen, None);
        }
    }
    eprintln!(
        "{}✗ connection closed before RPC completed{}",
        C_RED, C_RESET
    );
    (false, fleet_seen, None)
}
