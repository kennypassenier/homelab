//! The cyberpunk TUI (G1, AR6): Elm-style loop over a Backend.
//!
//! Some theme/fx helpers are carried over from the mockup and land in later
//! milestones (deploy focus window, sparklines); keep them available.
#![allow(dead_code)]

pub mod backend;
pub mod fx;
pub mod model;
pub mod theme;
pub mod view;

use std::io::stdout;
use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEventKind};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use futures_util::StreamExt;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use backend::Backend;
use model::{update, Model, Msg};

pub async fn run(backend: Box<dyn Backend>) -> std::io::Result<()> {
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
    // Discover locally-deployable stacks (a ./stacks dir next to the cwd).
    model.local_stacks = crate::spec::scan_local_stacks(std::path::Path::new("stacks"));
    model.presets = crate::scaffold::scan_presets(std::path::Path::new("presets"));

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

        tokio::select! {
            maybe = events.next() => {
                if let Some(Ok(Event::Key(key))) = maybe {
                    if key.kind == KeyEventKind::Press {
                        update(&mut model, Msg::Key(key));
                    }
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
                            .send(log("[release] checksum verified — shipping over the line"))
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

        // Flush queued commands from the pure update to the backend.
        for cmd in model.outbox.drain(..) {
            let _ = cmd_tx.send(cmd).await;
        }
    }

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
