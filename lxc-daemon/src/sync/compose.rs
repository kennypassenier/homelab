// Docker compose operations and orphan garbage collection.

use std::collections::HashSet;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::app::{AppState, LogLevel};

const GITOPS_REPO: &str = "/opt/gitops";

fn capture(cmd: &mut Command) -> (i32, String, String) {
    match cmd.output() {
        Ok(o) => (
            o.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (-1, String::new(), e.to_string()),
    }
}

fn log_cmd(state: &Arc<Mutex<AppState>>, label: &str, code: i32, stdout: &str, stderr: &str) {
    let mut s = state.lock().unwrap();
    let lvl = if code == 0 {
        LogLevel::Ok
    } else {
        LogLevel::Error
    };
    s.add_log(lvl, format!("[sync][exit] {} exit={}", label, code));
    for line in stdout.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        s.add_log(LogLevel::Info, format!("[sync][stdout] {} {}", label, line));
    }
    let elv = if code == 0 {
        LogLevel::Warn
    } else {
        LogLevel::Error
    };
    for line in stderr.lines().map(str::trim_end).filter(|l| !l.is_empty()) {
        s.add_log(elv.clone(), format!("[sync][stderr] {} {}", label, line));
    }
}

async fn emit_compose_failure_diagnostics(
    state: &Arc<Mutex<AppState>>,
    app_dir: &str,
    app_name: &str,
) {
    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Error,
            format!(
                "[sync][diag] compose command failed app={} -> collecting docker compose logs + ps",
                app_name
            ),
        );
        s.add_log(
            LogLevel::Info,
            format!(
                "[sync][run] cd {} && docker compose logs --no-color --tail 500",
                app_dir
            ),
        );
    }

    let dir_logs = app_dir.to_string();
    let (logs_code, logs_out, logs_err) = tokio::task::spawn_blocking(move || {
        capture(
            Command::new("docker")
                .args(["compose", "logs", "--no-color", "--tail", "500"])
                .current_dir(&dir_logs),
        )
    })
    .await
    .unwrap_or((-1, String::new(), "spawn failed".to_string()));
    log_cmd(
        state,
        &format!("docker compose logs app={}", app_name),
        logs_code,
        &logs_out,
        &logs_err,
    );

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!("[sync][run] cd {} && docker compose ps --all", app_dir),
        );
    }

    let dir_ps = app_dir.to_string();
    let (ps_code, ps_out, ps_err) = tokio::task::spawn_blocking(move || {
        capture(
            Command::new("docker")
                .args(["compose", "ps", "--all"])
                .current_dir(&dir_ps),
        )
    })
    .await
    .unwrap_or((-1, String::new(), "spawn failed".to_string()));
    log_cmd(
        state,
        &format!("docker compose ps app={}", app_name),
        ps_code,
        &ps_out,
        &ps_err,
    );

    // Pull direct container logs as requested by operators (e.g. `docker logs vikunja`).
    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!("[sync][run] cd {} && docker compose ps -q", app_dir),
        );
    }
    let dir_ids = app_dir.to_string();
    let (ids_code, ids_out, ids_err) = tokio::task::spawn_blocking(move || {
        capture(
            Command::new("docker")
                .args(["compose", "ps", "-q"])
                .current_dir(&dir_ids),
        )
    })
    .await
    .unwrap_or((-1, String::new(), "spawn failed".to_string()));
    log_cmd(
        state,
        &format!("docker compose ps ids app={}", app_name),
        ids_code,
        &ids_out,
        &ids_err,
    );

    if ids_code == 0 {
        for container_id in ids_out.lines().map(str::trim).filter(|id| !id.is_empty()) {
            {
                let mut s = state.lock().unwrap();
                s.add_log(
                    LogLevel::Info,
                    format!("[sync][run] docker logs --tail 500 {}", container_id),
                );
            }

            let cid = container_id.to_string();
            let (log_code, log_out, log_err) = tokio::task::spawn_blocking(move || {
                capture(Command::new("docker").args(["logs", "--tail", "500", &cid]))
            })
            .await
            .unwrap_or((-1, String::new(), "spawn failed".to_string()));

            log_cmd(
                state,
                &format!("docker logs app={} container={}", app_name, container_id),
                log_code,
                &log_out,
                &log_err,
            );
        }
    }
}

fn list_app_dirs(stack_dir: &str) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(stack_dir) else {
        return vec![];
    };
    let mut dirs: Vec<String> = rd
        .filter_map(|e| {
            let e = e.ok()?;
            if e.file_type().ok()?.is_dir() {
                Some(e.path().to_string_lossy().to_string())
            } else {
                None
            }
        })
        .collect();
    dirs.sort();
    dirs
}

fn appdata_root(stack_name: &str) -> PathBuf {
    Path::new("/appdata").join(stack_name)
}

fn repo_stack_root(stack_name: &str) -> PathBuf {
    Path::new(GITOPS_REPO).join("stacks").join(stack_name)
}

fn parse_volume_source(volume: &str) -> Option<&str> {
    let mut parts = volume.split(':');
    let source = parts.next()?;
    if source.is_empty() {
        None
    } else {
        Some(source)
    }
}

fn mirror_bind_mount_source(stack_name: &str, source: &Path) -> Result<(), String> {
    if source
        .to_string_lossy()
        .starts_with(&format!("/appdata/{}/", stack_name))
    {
        let suffix = source
            .strip_prefix(Path::new("/appdata").join(stack_name))
            .map_err(|e| e.to_string())?;
        let repo_candidate = repo_stack_root(stack_name).join(suffix);

        if repo_candidate.is_file() {
            if let Some(parent) = source.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("create dir {}: {}", parent.display(), e))?;
            }
            std::fs::copy(&repo_candidate, source).map_err(|e| {
                format!(
                    "copy {} -> {} failed: {}",
                    repo_candidate.display(),
                    source.display(),
                    e
                )
            })?;
            return Ok(());
        }

        if repo_candidate.is_dir() {
            std::fs::create_dir_all(source)
                .map_err(|e| format!("create dir {}: {}", source.display(), e))?;
            return Ok(());
        }
    }

    if source.extension().is_some() {
        if let Some(parent) = source.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create dir {}: {}", parent.display(), e))?;
        }
        if !source.exists() {
            std::fs::write(source, "").map_err(|e| format!("create file {}: {}", source.display(), e))?;
        }
        return Ok(());
    }

    std::fs::create_dir_all(source).map_err(|e| format!("create dir {}: {}", source.display(), e))
}

pub async fn prepare_stack_bind_mounts(
    state: Arc<Mutex<AppState>>,
    stack_name: &str,
) -> Result<(), String> {
    let stack_root = Path::new(GITOPS_REPO).join("stacks").join(stack_name);
    let apps = list_app_dirs(stack_root.to_string_lossy().as_ref());

    for app_dir in apps {
        let compose_path = Path::new(&app_dir).join("docker-compose.yml");
        if !compose_path.exists() {
            continue;
        }

        let raw = std::fs::read_to_string(&compose_path)
            .map_err(|e| format!("read {}: {}", compose_path.display(), e))?;
        let doc: serde_yaml::Value = serde_yaml::from_str(&raw)
            .map_err(|e| format!("parse {}: {}", compose_path.display(), e))?;
        let Some(root) = doc.as_mapping() else {
            continue;
        };
        let Some(services) = root
            .get(serde_yaml::Value::String("services".to_string()))
            .and_then(|v| v.as_mapping())
        else {
            continue;
        };

        for (svc_name, svc_val) in services {
            let service_name = svc_name.as_str().unwrap_or("unknown-service");
            let Some(svc_map) = svc_val.as_mapping() else {
                continue;
            };
            let Some(volumes_val) = svc_map.get(serde_yaml::Value::String("volumes".to_string()))
            else {
                continue;
            };

            let mut volume_list = Vec::new();
            if let Some(list) = volumes_val.as_sequence() {
                for item in list {
                    if let Some(volume) = item.as_str() {
                        volume_list.push(volume.to_string());
                    }
                }
            }

            for volume in volume_list {
                let Some(source) = parse_volume_source(&volume) else {
                    continue;
                };
                if !source.starts_with("/appdata/") {
                    continue;
                }

                let source_path = Path::new(source);
                if let Err(e) = mirror_bind_mount_source(stack_name, source_path) {
                    let mut s = state.lock().unwrap();
                    s.add_log(
                        LogLevel::Warn,
                        format!(
                            "[sync][prep] stack={} service={} volume={} skipped: {}",
                            stack_name, service_name, volume, e
                        ),
                    );
                }
            }
        }
    }

    Ok(())
}

fn validate_app_env_files(compose_path: &Path, app_dir: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(compose_path)
        .map_err(|e| format!("cannot read compose file {}: {}", compose_path.display(), e))?;

    let doc: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("invalid yaml: {}", e))?;
    let Some(root) = doc.as_mapping() else {
        return Ok(());
    };
    let Some(services) = root
        .get(serde_yaml::Value::String("services".to_string()))
        .and_then(|v| v.as_mapping())
    else {
        return Ok(());
    };

    let mut missing: Vec<String> = Vec::new();

    for (svc_name, svc_val) in services {
        let service_name = svc_name.as_str().unwrap_or("unknown-service");
        let Some(svc_map) = svc_val.as_mapping() else {
            continue;
        };
        let Some(env_file_val) = svc_map.get(serde_yaml::Value::String("env_file".to_string()))
        else {
            continue;
        };

        let mut env_paths: Vec<String> = Vec::new();
        if let Some(single) = env_file_val.as_str() {
            env_paths.push(single.to_string());
        } else if let Some(list) = env_file_val.as_sequence() {
            for item in list {
                if let Some(path) = item.as_str() {
                    env_paths.push(path.to_string());
                }
            }
        }

        for rel_or_abs in env_paths {
            let resolved = if Path::new(&rel_or_abs).is_absolute() {
                Path::new(&rel_or_abs).to_path_buf()
            } else {
                Path::new(app_dir).join(&rel_or_abs)
            };
            if !resolved.exists() {
                missing.push(format!(
                    "service={} env_file={} resolved={} missing",
                    service_name,
                    rel_or_abs,
                    resolved.display()
                ));
            }
        }
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join(" | "))
    }
}

fn expected_service_names(compose_path: &Path) -> Result<Vec<String>, String> {
    let content = std::fs::read_to_string(compose_path)
        .map_err(|e| format!("cannot read compose file {}: {}", compose_path.display(), e))?;
    let doc: serde_yaml::Value =
        serde_yaml::from_str(&content).map_err(|e| format!("invalid yaml: {}", e))?;
    let Some(root) = doc.as_mapping() else {
        return Ok(Vec::new());
    };
    let Some(services) = root
        .get(serde_yaml::Value::String("services".to_string()))
        .and_then(|v| v.as_mapping())
    else {
        return Ok(Vec::new());
    };

    let mut names = Vec::new();
    for (name, _) in services {
        if let Some(s) = name.as_str() {
            names.push(s.to_string());
        }
    }
    names.sort();
    Ok(names)
}

async fn verify_compose_runtime(
    state: &Arc<Mutex<AppState>>,
    app_dir: &str,
    app_name: &str,
    compose_path: &Path,
) -> Result<(), String> {
    let expected = expected_service_names(compose_path)?;

    // Allow short startup stabilization before asserting runtime state.
    tokio::time::sleep(Duration::from_secs(4)).await;

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!(
                "[sync][run] cd {} && docker compose ps --status running --services",
                app_dir
            ),
        );
    }

    let dir = app_dir.to_string();
    let (code, out, err) = tokio::task::spawn_blocking(move || {
        capture(
            Command::new("docker")
                .args(["compose", "ps", "--status", "running", "--services"])
                .current_dir(&dir),
        )
    })
    .await
    .unwrap_or((-1, String::new(), "spawn failed".to_string()));

    log_cmd(
        state,
        &format!("docker compose ps running app={}", app_name),
        code,
        &out,
        &err,
    );

    if code != 0 {
        return Err(format!(
            "docker compose ps --status running failed for app={} exit={} stderr={}",
            app_name,
            code,
            err.trim()
        ));
    }

    let running: HashSet<String> = out
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToString::to_string)
        .collect();

    if expected.is_empty() {
        return Err(format!(
            "no services defined in compose for app={} ({})",
            app_name,
            compose_path.display()
        ));
    }

    let mut missing = Vec::new();
    for svc in &expected {
        if !running.contains(svc) {
            missing.push(svc.clone());
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "runtime verification failed for app={}: missing running services [{}]",
            app_name,
            missing.join(",")
        ));
    }

    // Verify containers are not already restart-looping even if still marked running.
    for service in &expected {
        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Info,
                format!(
                    "[sync][run] cd {} && docker compose ps -q {}",
                    app_dir, service
                ),
            );
        }

        let dir = app_dir.to_string();
        let svc = service.clone();
        let (id_code, id_out, id_err) = tokio::task::spawn_blocking(move || {
            capture(
                Command::new("docker")
                    .args(["compose", "ps", "-q", &svc])
                    .current_dir(&dir),
            )
        })
        .await
        .unwrap_or((-1, String::new(), "spawn failed".to_string()));

        log_cmd(
            state,
            &format!("docker compose ps id app={} service={}", app_name, service),
            id_code,
            &id_out,
            &id_err,
        );

        if id_code != 0 {
            return Err(format!(
                "failed to resolve container id for app={} service={} stderr={}",
                app_name,
                service,
                id_err.trim()
            ));
        }

        let container_id = id_out.lines().next().unwrap_or("").trim().to_string();
        if container_id.is_empty() {
            return Err(format!(
                "empty container id for app={} service={}",
                app_name, service
            ));
        }

        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Info,
                format!(
                    "[sync][run] docker inspect --format {{.State.Status}}:{{.RestartCount}} {}",
                    container_id
                ),
            );
        }

        let cid = container_id.clone();
        let (insp_code, insp_out, insp_err) = tokio::task::spawn_blocking(move || {
            capture(Command::new("docker").args([
                "inspect",
                "--format",
                "{{.State.Status}}:{{.RestartCount}}",
                &cid,
            ]))
        })
        .await
        .unwrap_or((-1, String::new(), "spawn failed".to_string()));

        log_cmd(
            state,
            &format!(
                "docker inspect runtime app={} service={}",
                app_name, service
            ),
            insp_code,
            &insp_out,
            &insp_err,
        );

        if insp_code != 0 {
            return Err(format!(
                "docker inspect failed for app={} service={} stderr={}",
                app_name,
                service,
                insp_err.trim()
            ));
        }

        let runtime_line = insp_out.lines().next().unwrap_or("").trim().to_string();
        let mut parts = runtime_line.split(':');
        let status = parts.next().unwrap_or("");
        let restart_count = parts.next().unwrap_or("0").parse::<u64>().unwrap_or(0);

        if status != "running" || restart_count > 0 {
            return Err(format!(
                "runtime unstable for app={} service={} status={} restart_count={}",
                app_name, service, status, restart_count
            ));
        }
    }

    Ok(())
}

/// Pull images and start all app compose dirs for a stack.
pub async fn deploy_apps(state: Arc<Mutex<AppState>>, stack_name: &str) -> Result<(), String> {
    let stack_dir = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let app_dirs = list_app_dirs(&stack_dir);

    {
        let mut s = state.lock().unwrap();
        s.add_log(
            LogLevel::Info,
            format!(
                "[sync] discovered {} app directories under {}",
                app_dirs.len(),
                stack_dir
            ),
        );
    }

    let mut deployed_apps = 0usize;

    for app_dir in &app_dirs {
        let compose = format!("{}/docker-compose.yml", app_dir);
        if !Path::new(&compose).exists() {
            continue;
        }

        let app_name = Path::new(app_dir)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if let Err(e) = validate_app_env_files(Path::new(&compose), app_dir) {
            {
                let mut s = state.lock().unwrap();
                s.add_log(
                    LogLevel::Error,
                    format!(
                        "[sync][validate] app={} env_file validation failed: {}",
                        app_name, e
                    ),
                );
            }
            return Err(format!(
                "env_file validation failed for app={}: {}",
                app_name, e
            ));
        }

        // docker compose pull -q
        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Info,
                format!("[sync][run] cd {} && docker compose pull -q", app_dir),
            );
        }
        let dir = app_dir.clone();
        let an = app_name.clone();
        let (code, out, err) = tokio::task::spawn_blocking(move || {
            capture(
                Command::new("docker")
                    .args(["compose", "pull", "-q"])
                    .current_dir(&dir),
            )
        })
        .await
        .unwrap_or((-1, String::new(), "spawn failed".to_string()));
        log_cmd(
            &state,
            &format!("docker compose pull app={}", an),
            code,
            &out,
            &err,
        );
        if code != 0 {
            emit_compose_failure_diagnostics(&state, app_dir, &app_name).await;
            return Err(format!(
                "docker compose pull failed for app={} exit={} stderr={}",
                app_name,
                code,
                err.trim()
            ));
        }

        // docker compose up -d --remove-orphans
        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Info,
                format!(
                    "[sync][run] cd {} && docker compose up -d --remove-orphans",
                    app_dir
                ),
            );
        }
        let dir2 = app_dir.clone();
        let an2 = app_name.clone();
        let (code2, out2, err2) = tokio::task::spawn_blocking(move || {
            capture(
                Command::new("docker")
                    .args(["compose", "up", "-d", "--remove-orphans"])
                    .current_dir(&dir2),
            )
        })
        .await
        .unwrap_or((-1, String::new(), "spawn failed".to_string()));
        log_cmd(
            &state,
            &format!("docker compose up app={}", an2),
            code2,
            &out2,
            &err2,
        );
        if code2 != 0 {
            emit_compose_failure_diagnostics(&state, app_dir, &app_name).await;
            return Err(format!(
                "docker compose up failed for app={} exit={} stderr={}",
                app_name,
                code2,
                err2.trim()
            ));
        }

        if let Err(e) =
            verify_compose_runtime(&state, app_dir, &app_name, Path::new(&compose)).await
        {
            emit_compose_failure_diagnostics(&state, app_dir, &app_name).await;
            return Err(e);
        }

        deployed_apps += 1;
    }

    if deployed_apps == 0 {
        return Err(format!(
            "no deployable app directories found under {}/stacks/{}",
            GITOPS_REPO, stack_name
        ));
    }

    Ok(())
}

/// Stop and remove appdata dirs for apps that no longer exist in Git.
pub async fn garbage_collect(state: Arc<Mutex<AppState>>, stack_name: &str) {
    let appdata = Path::new("/appdata");
    if !appdata.exists() {
        return;
    }

    let git_stack = format!("{}/stacks/{}", GITOPS_REPO, stack_name);
    let git_apps: HashSet<String> = list_app_dirs(&git_stack)
        .into_iter()
        .filter_map(|p| {
            Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
        })
        .collect();

    let Ok(entries) = std::fs::read_dir(appdata) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let app_name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => continue,
        };
        if git_apps.contains(&app_name) {
            continue;
        }

        {
            let mut s = state.lock().unwrap();
            s.add_log(
                LogLevel::Warn,
                format!("[sync] orphan gc: {} — no longer in git", app_name),
            );
        }

        let compose = path.join("docker-compose.yml");
        if compose.exists() {
            let p2 = path.clone();
            let _ = tokio::task::spawn_blocking(move || {
                let _ = Command::new("docker")
                    .args(["compose", "down", "--remove-orphans"])
                    .current_dir(&p2)
                    .output();
            })
            .await;
        }

        let p3 = path.clone();
        let sn = stack_name.to_string();
        let an = app_name.clone();
        let result = tokio::task::spawn_blocking(move || {
            std::fs::remove_dir_all(&p3).map_err(|e| e.to_string())
        })
        .await
        .unwrap_or_else(|_| Err("spawn failed".to_string()));
        let mut s = state.lock().unwrap();
        match result {
            Ok(_) => s.add_log(
                LogLevel::Ok,
                format!("[sync] orphan gc: stack={} app={} removed", sn, an),
            ),
            Err(e) => s.add_log(
                LogLevel::Error,
                format!("[sync] orphan gc: stack={} app={} error: {}", sn, an, e),
            ),
        }
    }
}
