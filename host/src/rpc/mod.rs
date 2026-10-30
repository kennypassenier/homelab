//! The RPC dispatcher: one line per request, each handled by a function in the
//! file of its topic.

mod backups;
mod cleanup;
mod config;
mod native;
mod questions;
mod secrets;
mod snapshots;
mod stacks;
mod system;
mod tokens;
mod views;

use crate::*;

pub(crate) async fn handle_rpc(state: &AppState, req: RpcRequest) -> RpcResponse {
    let exec = RealExecutor;
    match req.command {
        Rpc::History {
            since,
            until,
            limit,
        } => system::history(state, req.id, since, until, limit).await,
        Rpc::Notices { after, limit } => system::notices(state, req.id, after, limit).await,
        Rpc::CurrentOp => system::current_op(state, req.id).await,
        Rpc::SessionOptions { .. } => RpcResponse {
            id: req.id,
            ok: true,
            message: "session options are set per session".into(),
            deferred: None,
        },
        // feat-platform-10: answered by the session loop (`serve_ws`).
        Rpc::Ui { .. } | Rpc::UiAttach | Rpc::UiReply { .. } | Rpc::UiHold { .. } => RpcResponse {
            id: req.id,
            ok: false,
            message: "UI steps are relayed by the session, not run".into(),
            deferred: None,
        },
        Rpc::Ping => RpcResponse {
            id: req.id,
            ok: true,
            message: "pong".into(),
            deferred: None,
        },
        Rpc::Status => system::status(state, req.id).await,
        Rpc::StageNativeBinary {
            stack,
            unit,
            binary_b64,
        } => native::stage_native_binary(state, req.id, stack, unit, binary_b64).await,
        Rpc::DeployStack(spec) => stacks::deploy_stack(state, req.id, spec).await,
        Rpc::DestroyStack {
            manifest,
            confirm,
            skip_backup,
        } => stacks::destroy_stack(state, req.id, manifest, confirm, skip_backup).await,
        Rpc::BackupStack(manifest) => backups::backup_stack(state, req.id, manifest).await,
        Rpc::RestoreStack {
            manifest,
            snapshot,
            confirm,
            skip_safety_copy,
            app,
            source,
        } => {
            backups::restore_stack(
                state,
                req.id,
                manifest,
                snapshot,
                confirm,
                skip_safety_copy,
                app,
                source,
            )
            .await
        }
        Rpc::UpdateStack { manifest, app } => {
            stacks::update_stack(state, req.id, manifest, app).await
        }
        Rpc::PatchFleet => stacks::patch_fleet(state, req.id).await,
        Rpc::ApplyResources(manifest) => stacks::apply_resources(state, req.id, manifest).await,
        Rpc::BackupHostMeta => backups::backup_host_meta(state, req.id).await,
        Rpc::AdoptService(m) => native::adopt_service(state, req.id, m).await,
        Rpc::InstallNative {
            manifest,
            binary_b64,
            unit_file,
        } => native::install_native(state, req.id, manifest, binary_b64, unit_file).await,
        Rpc::InstallNativeRelease {
            manifest,
            unit_file,
            tag,
        } => native::install_native_release(state, req.id, manifest, unit_file, tag).await,
        Rpc::BackupNative { stack } => native::backup_native(state, req.id, stack).await,
        Rpc::UpdateNative { stack } => native::update_native(state, req.id, stack).await,
        Rpc::RollbackNative { stack, unit } => {
            native::rollback_native(state, req.id, stack, unit).await
        }
        Rpc::ReleaseUpdateNative { stack } => {
            native::release_update_native(state, req.id, stack).await
        }
        Rpc::PruneOrphans {
            manifest,
            spec,
            confirm,
        } => cleanup::prune_orphans(state, req.id, manifest, spec, confirm).await,
        Rpc::ForgetStack { stack } => cleanup::forget_stack(state, req.id, stack).await,
        Rpc::DestroyRecorded {
            stack,
            confirm,
            skip_backup,
        } => cleanup::destroy_recorded(state, req.id, stack, confirm, skip_backup).await,
        Rpc::WipeRetired { name, confirm } => {
            cleanup::wipe_retired(state, req.id, name, confirm).await
        }
        Rpc::ApplyGuards { vmid } => cleanup::apply_guards(state, req.id, vmid).await,
        Rpc::FleetCheck {
            stack_files,
            digests,
            json,
            host_config,
        } => views::fleet_check(state, req.id, stack_files, digests, json, host_config).await,
        Rpc::Today {
            stack_files,
            digests,
            host_config,
        } => views::today(state, req.id, stack_files, digests, host_config).await,
        Rpc::Answer { id, allow, boot } => questions::answer(state, req.id, id, allow, boot).await,
        Rpc::AnswerOpen { op, allow } => questions::answer_open(state, req.id, op, allow).await,
        Rpc::ZfsReplicate => backups::zfs_replicate(state, req.id).await,
        Rpc::BackupDevices => backups::backup_devices(state, req.id).await,
        Rpc::Tiles { bare } => views::tiles(state, req.id, bare).await,
        Rpc::BackupCalendar { stacks, force } => {
            backups::backup_calendar(state, req.id, stacks, force).await
        }
        Rpc::StackRuntime { stack } => stacks::stack_runtime(state, req.id, stack).await,
        Rpc::ListManualChecks { json } => questions::list_manual_checks(state, req.id, json).await,
        Rpc::AnswerManualCheck {
            check_id: id,
            ok,
            note,
            accept_days,
        } => {
            let store = homelab_core::state::StateStore::new(&exec, &state.config.state_dir);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            // fix-65: a deliberate nok, accepted until a date.
            let until = accept_days.map(|d| now + u64::from(d) * 86_400);
            let found = record_state(&store, "manual check answer", |st| match until {
                Some(u) => homelab_core::ops::manualchecks::accept(st, &id, u, &note, now),
                None => homelab_core::ops::manualchecks::answer(st, &id, ok, &note, now),
            })
            .await
            .unwrap_or(false);
            let message = if found {
                match until {
                    Some(u) => format!(
                        "{} recorded as NOT ok, accepted until {}",
                        id,
                        homelab_core::state::ymd(u)
                    ),
                    None => format!("{} recorded as {}", id, if ok { "ok" } else { "NOT ok" }),
                }
            } else {
                format!(
                    "no manual check has id {} — run `homelab checks` for the list",
                    id
                )
            };
            RpcResponse {
                id: req.id,
                ok: found,
                message,
                deferred: None,
            }
        }
        Rpc::SetStackEnabled { stack, enabled } => {
            stacks::set_stack_enabled(state, req.id, stack, enabled).await
        }
        Rpc::SealEnv { stack } => secrets::seal_env(state, req.id, stack).await,
        Rpc::ListTemplates => stacks::list_templates(state, req.id).await,
        Rpc::BuildTemplate {
            temp_vmid,
            version,
            unprivileged,
            base_template,
        } => {
            stacks::build_template(
                state,
                req.id,
                temp_vmid,
                version,
                unprivileged,
                base_template,
            )
            .await
        }
        Rpc::ExecIn { vmid, command } => questions::exec_in(state, req.id, vmid, command).await,
        Rpc::CancelExec { request } => questions::cancel_exec(state, req.id, request).await,
        Rpc::GetApplied { stack } => views::get_applied(state, req.id, stack).await,
        Rpc::GetFirewallLive { stack_files } => {
            views::get_firewall_live(state, req.id, stack_files).await
        }
        Rpc::GetBackups {
            stack,
            force,
            source,
        } => snapshots::get_backups(state, req.id, stack, force, source).await,
        Rpc::GetRetired => views::get_retired(state, req.id).await,
        Rpc::BrowseSnapshot {
            owner,
            snapshot,
            path,
            source,
        } => snapshots::browse_snapshot(state, req.id, owner, snapshot, path, source).await,
        Rpc::ReadSnapshotFile {
            stack,
            owner,
            snapshot,
            path,
            source,
        } => {
            snapshots::read_snapshot_file(state, req.id, stack, owner, snapshot, path, source).await
        }
        Rpc::VerifyRestore {
            stack,
            app,
            snapshot,
            source,
        } => snapshots::verify_restore(state, req.id, stack, app, snapshot, source).await,
        Rpc::RestoreNative {
            stack,
            snapshot,
            confirm,
            unit,
            source,
        } => native::restore_native(state, req.id, stack, snapshot, confirm, unit, source).await,
        Rpc::RevealSecret {
            stack,
            secret,
            audit,
        } => secrets::reveal_secret(state, req.id, stack, secret, audit).await,
        Rpc::SetSecret {
            stack,
            secret,
            content,
        } => secrets::set_secret(state, req.id, stack, secret, content).await,
        Rpc::GetConfig => config::get_config(state, req.id).await,
        Rpc::SetConfig(view) => config::set_config(state, req.id, view).await,
        Rpc::GetHostConfig => config::get_host_config(state, req.id).await,
        Rpc::SetHostConfig {
            changes,
            expect_sha256,
        } => config::set_host_config(state, req.id, changes, expect_sha256).await,
        Rpc::TokenIssue { name, scope } => tokens::token_issue(state, req.id, name, scope).await,
        Rpc::TokenList => tokens::token_list(state, req.id).await,
        Rpc::TokenRevoke { name } => tokens::token_revoke(state, req.id, name).await,
        // fix-110: `config/host.toml` sent whole — `homelab host apply`, or
        // the dashboard's declarative commit of it. Same outcome shape as
        // `SetHostConfig`: the G8 keys go live immediately, the rest waits
        // for the host's next start.
        Rpc::ApplyHostConfig {
            toml: declared,
            expect_sha256,
            allow,
        } => {
            let path = state.config.config_path.clone();
            let raw = std::fs::read_to_string(&path).unwrap_or_default();
            let outcome =
                apply_host_config_whole(&raw, &declared, expect_sha256.as_deref(), &allow)
                    .and_then(|(text, saved)| {
                        write_config_file(&path, &text).map(|()| (text, saved))
                    });
            match outcome {
                Ok((text, saved)) => {
                    if let Ok(file) = toml::from_str::<FileConfig>(&text) {
                        let new_level = file
                            .log_level
                            .clone()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or_else(|| "info".to_string());
                        {
                            let mut live = state
                                .settings
                                .write()
                                .unwrap_or_else(PoisonError::into_inner);
                            live.backup_hour = file.backup_hour;
                            live.notify_webhook = file.notify_webhook;
                            live.retention = file
                                .retention
                                .unwrap_or_else(homelab_core::retention::default_tiers);
                            live.log_level = new_level.clone();
                        }
                        // fix-122: same live reload as `SetHostConfig`.
                        let _ = state.log_filter.reload(
                            tracing_subscriber::EnvFilter::try_new(&new_level)
                                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                        );
                    }
                    info!("host.toml applied from config/host.toml");
                    RpcResponse {
                        id: req.id,
                        ok: true,
                        message: serde_json::to_string(&saved).unwrap_or_default(),
                        deferred: None,
                    }
                }
                Err(e) => RpcResponse {
                    id: req.id,
                    ok: false,
                    message: e,
                    deferred: None,
                },
            }
        }
        Rpc::SelfUpdateHost { binary_b64, proof } => {
            system::self_update_host(state, req.id, binary_b64, proof).await
        }
        Rpc::RestartHost => system::restart_host(state, req.id).await,
        Rpc::Doctor { json } => system::doctor(state, req.id, json).await,
        Rpc::GetState => system::get_state(state, req.id).await,
        Rpc::IncidentShow { name } => system::incident_show(state, req.id, name).await,
        Rpc::Incidents { json } => system::incidents(state, req.id, json).await,
    }
}
