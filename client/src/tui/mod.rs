//! The cyberpunk TUI (G1, AR6): Elm-style loop over a Backend.
//!
//! Some theme/fx helpers are carried over from the mockup and land in later
//! milestones (deploy focus window, sparklines); keep them available.
#![allow(dead_code)]

pub mod backend;
pub mod fx;
pub mod keys;
pub mod model;
pub mod theme;
pub mod view;

use std::io::stdout;
use std::time::Duration;

use crossterm::ExecutableCommand;
use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use futures_util::StreamExt;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use backend::Backend;
use model::{Model, Msg, update};

/// `repo`: the repository root the command line found (fix-101), so the TUI
/// reads the same stacks and presets from any directory.
pub async fn run(
    backend: Box<dyn Backend>,
    repo: Option<std::path::PathBuf>,
) -> std::io::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = stdout().execute(LeaveAlternateScreen);
        default_hook(info);
    }));

    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    terminal.clear()?;

    let channels = backend.start();
    let mut evt_rx = channels.evt_rx;
    let cmd_tx = channels.cmd_tx;

    let mut model = Model::new();
    // fix-101 (cli-path-vs-name-and-cwd, 2026-09-27): the repository's stacks
    // and presets, not whatever `./stacks` the TUI was started next to.
    if let Some(root) = repo {
        model.stacks_dir = root.join("stacks");
        model.presets_dir = root.join("presets");
    }
    // fix-106 (tui-not-calm, 2026-09-27): start at the effect level F2 last
    // chose; nothing saved means off.
    let fx_file = fx::fx_path();
    if let Some(level) = fx::load_fx(&fx_file) {
        model.fx = level;
    }
    let mut saved_fx = model.fx;
    model.local_stacks = crate::spec::scan_local_stacks(&model.stacks_dir);
    model.presets = crate::scaffold::scan_presets(&model.presets_dir);

    // H7: release check off-thread; the loop below folds the answer in.
    let (side_tx, mut side_rx) = tokio::sync::mpsc::channel::<Msg>(8);
    {
        let tx = side_tx.clone();
        tokio::task::spawn_blocking(move || {
            let tag = crate::release::latest_release_tag();
            let _ = tx.blocking_send(Msg::ReleaseTag(tag));
        });
    }
    let mut events = EventStream::new();
    let mut anim = tokio::time::interval(Duration::from_millis(33));
    anim.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    while !model.should_quit {
        terminal.draw(|f| view::draw(f, &model))?;
        // fix-106: F2 (or the palette) changed the level: keep it.
        if model.fx != saved_fx {
            fx::save_fx(&fx_file, model.fx);
            saved_fx = model.fx;
        }

        tokio::select! {
            maybe = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe
                    && key.kind == KeyEventKind::Press {
                        update(&mut model, Msg::Key(key));
                    }
            }
            Some(bev) = evt_rx.recv() => {
                update(&mut model, Msg::Backend(bev));
            }
            Some(side) = side_rx.recv() => {
                update(&mut model, side);
            }
            _ = anim.tick() => {
                update(&mut model, Msg::Tick);
            }
        }

        // H7: the U key requested a release update — stage it off-thread and
        // ship it through the normal command channel; progress lines arrive
        // as synthesized log events in the open focus window.
        if let Some(tag) = model.release_update_requested.take() {
            let tx = side_tx.clone();
            let ctx = cmd_tx.clone();
            tokio::spawn(async move {
                let log = |m: &str| {
                    Msg::Backend(backend::BackendEvent::Server(
                        homelab_proto::ServerMsg::Log {
                            req: None,
                            step: None,
                            ts: None,
                            by: None,
                            level: homelab_proto::LogLevel::Info,
                            source: "LOCAL".into(),
                            msg: m.to_string(),
                        },
                    ))
                };
                let _ = tx
                    .send(log(&format!("[release] downloading {} via gh…", tag)))
                    .await;
                let staged =
                    tokio::task::spawn_blocking(move || crate::release::stage_release(&tag))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                match staged {
                    Ok(binary_b64) => {
                        let _ = tx
                            .send(log("[release] signature and checksum verified — shipping over the line"))
                            .await;
                        let _ = ctx
                            .send(homelab_proto::Command::SelfUpdateHost { binary_b64 })
                            .await;
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Msg::Backend(backend::BackendEvent::Server(
                                homelab_proto::ServerMsg::RpcDone(homelab_proto::RpcResponse {
                                    id: 0,
                                    ok: false,
                                    message: format!("release staging failed: {}", e),
                                    deferred: None,
                                }),
                            )))
                            .await;
                    }
                }
            });
        }

        // T71: the I key asked for native binaries. Same shape as H7 above
        // and for the same reason — `gh` is a network fetch and the event
        // loop may not block on it. Sequential rather than concurrent: three
        // downloads racing would interleave their progress lines into one
        // window and the reader could not tell which service failed.
        if !model.native_install_requested.is_empty() {
            let wanted = std::mem::take(&mut model.native_install_requested);
            let tx = side_tx.clone();
            let ctx = cmd_tx.clone();
            tokio::spawn(async move {
                let log = |m: String| {
                    Msg::Backend(backend::BackendEvent::Server(
                        homelab_proto::ServerMsg::Log {
                            req: None,
                            step: None,
                            ts: None,
                            by: None,
                            level: homelab_proto::LogLevel::Info,
                            source: "LOCAL".into(),
                            msg: m,
                        },
                    ))
                };
                let fail = |m: String| {
                    Msg::Backend(backend::BackendEvent::Server(
                        homelab_proto::ServerMsg::RpcDone(homelab_proto::RpcResponse {
                            id: 0,
                            ok: false,
                            message: m,
                            deferred: None,
                        }),
                    ))
                };
                for (manifest, unit_file) in wanted {
                    let Some(unit_file) = unit_file else {
                        let _ = tx
                            .send(fail(format!(
                                "{}: no {}.service in the repository — a binary with nothing to \
                                 run it is not an install",
                                manifest.unit, manifest.unit
                            )))
                            .await;
                        continue;
                    };
                    let Some(repo) = manifest.release_repo.clone() else {
                        continue;
                    };
                    let asset = manifest.asset_name().to_string();
                    let tag = {
                        let r = repo.clone();
                        tokio::task::spawn_blocking(move || crate::release::latest_tag_of(&r))
                            .await
                            .ok()
                            .flatten()
                    };
                    let Some(tag) = tag else {
                        let _ = tx
                            .send(fail(format!(
                                "{}: no release found in {} (gh authenticated?)",
                                manifest.unit, repo
                            )))
                            .await;
                        continue;
                    };
                    let _ = tx
                        .send(log(format!(
                            "[install] {} :: {} {} from {}",
                            manifest.unit, asset, tag, repo
                        )))
                        .await;
                    let staged = {
                        let (r, t, a) = (repo.clone(), tag.clone(), asset.clone());
                        tokio::task::spawn_blocking(move || crate::release::stage_asset(&r, &t, &a))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    };
                    match staged {
                        Ok(binary_b64) => {
                            if let Some(why) = crate::version::too_large(binary_b64.len()) {
                                let _ = tx.send(fail(format!("{}: {}", manifest.unit, why))).await;
                                continue;
                            }
                            let _ = tx
                                .send(log(format!(
                                    "[install] {} :: checksum verified — shipping over the line",
                                    manifest.unit
                                )))
                                .await;
                            let _ = ctx
                                .send(homelab_proto::Command::InstallNative {
                                    manifest: Box::new(manifest),
                                    binary_b64,
                                    unit_file,
                                })
                                .await;
                        }
                        Err(e) => {
                            let _ = tx.send(fail(format!("{}: {}", manifest.unit, e))).await;
                        }
                    }
                }
            });
        }

        // fix-69: the drift badge's local hashes. `latch` runs for each, so
        // they are computed here, off the event loop, one after another, and
        // come back as messages; nothing they print reaches the screen.
        if !model.local_hash_requested.is_empty() {
            let wanted = std::mem::take(&mut model.local_hash_requested);
            let tx = side_tx.clone();
            tokio::spawn(async move {
                for (stack, dir) in wanted {
                    let computed =
                        tokio::task::spawn_blocking(move || crate::spec::local_intent_hash(&dir))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()));
                    let (hash, notes) = match computed {
                        Ok((h, notes)) => (Ok(h), notes),
                        Err(e) => (Err(e), Vec::new()),
                    };
                    if tx
                        .send(Msg::LocalHash { stack, hash, notes })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }

        // Flush queued commands from the pure update to the backend.
        for cmd in model.outbox.drain(..) {
            let _ = cmd_tx.send(cmd).await;
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
