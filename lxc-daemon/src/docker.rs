use bollard::Docker;
use bollard::container::ListContainersOptions;
use std::sync::{Arc, Mutex};
use crate::app::{AppState, ContainerInfo, LogLevel};

pub async fn run_poller(state: Arc<Mutex<AppState>>) {
    let docker = match Docker::connect_with_unix_defaults() {
        Ok(d) => d,
        Err(e) => {
            let mut s = state.lock().unwrap();
            s.add_log(LogLevel::Error, format!("Docker connect failed: {}", e));
            return;
        }
    };

    {
        let mut s = state.lock().unwrap();
        s.add_log(LogLevel::Info, "Connected to Docker daemon".to_string());
    }

    loop {
        poll_containers(&docker, state.clone()).await;
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

async fn poll_containers(docker: &Docker, state: Arc<Mutex<AppState>>) {
    let options: ListContainersOptions<String> = ListContainersOptions {
        all: true,
        ..Default::default()
    };

    match docker.list_containers(Some(options)).await {
        Ok(containers) => {
            let infos: Vec<ContainerInfo> = containers
                .iter()
                .map(|c| {
                    let name = c
                        .names
                        .as_ref()
                        .and_then(|n| n.first())
                        .map(|n| n.trim_start_matches('/').to_string())
                        .unwrap_or_default();

                    let image = c.image.clone().unwrap_or_default();
                    let image_short = image
                        .split('/')
                        .last()
                        .unwrap_or(&image)
                        .split(':')
                        .next()
                        .unwrap_or(&image)
                        .to_string();

                    let state_str = c.state.clone().unwrap_or_default();
                    let status = if state_str == "running" {
                        "\u{25cf} UP".to_string()
                    } else {
                        "\u{25cb} DN".to_string()
                    };

                    let ports = c
                        .ports
                        .as_ref()
                        .map(|ps| {
                            let strs: Vec<String> = ps
                                .iter()
                                .filter_map(|p| p.public_port.map(|pp| format!(":{}", pp)))
                                .collect::<std::collections::HashSet<_>>()
                                .into_iter()
                                .collect();
                            if strs.is_empty() {
                                "(internal)".to_string()
                            } else {
                                strs.join(" ")
                            }
                        })
                        .unwrap_or_else(|| "(internal)".to_string());

                    let uptime = c.status.clone().unwrap_or_default();

                    ContainerInfo {
                        id: c.id.clone().unwrap_or_default(),
                        name,
                        image: image_short,
                        status,
                        ports,
                        uptime,
                    }
                })
                .collect();

            let mut s = state.lock().unwrap();
            s.containers = infos;
        }
        Err(e) => {
            let mut s = state.lock().unwrap();
            s.add_log(LogLevel::Error, format!("Docker poll error: {}", e));
        }
    }
}
