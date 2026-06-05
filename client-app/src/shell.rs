//! Shell command execution — local and remote (LXC via HTTP API).
//!
//! Provides utilities for executing commands on CLIENT desktop or remotely
//! via the LXC daemon's /api/exec endpoint.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio_tungstenite::tungstenite::Message;

use futures::{SinkExt, StreamExt};

/// Request to execute a command on LXC daemon
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecRequest {
    pub cmd: String,
    pub args: Option<Vec<String>>,
    pub stdin: Option<String>,
    pub timeout_secs: Option<u64>,
}

/// Response from command execution
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ExecResponse {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct WsExecRequest {
    kind: String,
    request_id: String,
    cmd: String,
    args: Option<Vec<String>>,
    stdin: Option<String>,
    timeout_secs: Option<u64>,
    token: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct WsExecResponse {
    kind: String,
    request_id: String,
    exit_code: Option<i32>,
    stdout: Option<String>,
    stderr: Option<String>,
    error: Option<String>,
}

impl ExecResponse {
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    pub fn combined_output(&self) -> String {
        if self.stderr.is_empty() {
            self.stdout.clone()
        } else if self.stdout.is_empty() {
            self.stderr.clone()
        } else {
            format!("{}\n---STDERR---\n{}", self.stdout, self.stderr)
        }
    }
}

/// Execute a command locally on the CLIENT desktop
pub async fn execute_local(
    cmd: &str,
    args: Vec<String>,
    stdin: Option<String>,
    timeout_secs: Option<u64>,
) -> Result<ExecResponse, Box<dyn std::error::Error>> {
    let timeout_secs = timeout_secs.unwrap_or(30);

    let future = async {
        let mut child = tokio::process::Command::new(cmd)
            .args(args)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Write stdin if provided
        if let Some(stdin_data) = stdin {
            if let Some(mut stdin_handle) = child.stdin.take() {
                stdin_handle.write_all(stdin_data.as_bytes()).await?;
            }
        }

        let output = child.wait_with_output().await?;

        Ok::<ExecResponse, Box<dyn std::error::Error>>(ExecResponse {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    };

    match tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), future).await {
        Ok(Ok(response)) => Ok(response),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(Box::from(format!(
            "Command timed out after {} seconds",
            timeout_secs
        ))),
    }
}

/// Execute a command remotely on an LXC daemon via HTTP API
pub async fn execute_remote(
    lxc_api_base: &str,
    request: ExecRequest,
) -> Result<ExecResponse, Box<dyn std::error::Error>> {
    if let Ok(response) = execute_remote_ws(lxc_api_base, request.clone()).await {
        return Ok(response);
    }

    let client = reqwest::Client::new();
    let url = format!("{}/api/exec", lxc_api_base);
    let token = std::env::var("LXC_API_TOKEN").unwrap_or_default();

    let mut req = client.post(&url).json(&request);
    if !token.is_empty() {
        req = req.bearer_auth(token);
    }
    let response = req.send().await?;

    match response.status() {
        reqwest::StatusCode::OK | reqwest::StatusCode::BAD_REQUEST => {
            let exec_response = response.json::<ExecResponse>().await?;
            Ok(exec_response)
        }
        reqwest::StatusCode::REQUEST_TIMEOUT => Err(Box::from("Remote command timed out")),
        status => Err(Box::from(format!(
            "LXC daemon returned status {}: {}",
            status,
            response.text().await.unwrap_or_default()
        ))),
    }
}

async fn execute_remote_ws(
    lxc_api_base: &str,
    request: ExecRequest,
) -> Result<ExecResponse, Box<dyn std::error::Error>> {
    let ws_base = if let Some(rest) = lxc_api_base.strip_prefix("https://") {
        format!("wss://{}", rest)
    } else if let Some(rest) = lxc_api_base.strip_prefix("http://") {
        format!("ws://{}", rest)
    } else {
        return Err("invalid lxc_api_base for websocket".into());
    };
    let ws_url = format!("{}/api/logs/ws", ws_base);

    let (mut socket, _) = tokio_tungstenite::connect_async(&ws_url).await?;
    let request_id = format!(
        "exec-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let token = std::env::var("LXC_API_TOKEN")
        .ok()
        .filter(|v| !v.is_empty());

    let frame = WsExecRequest {
        kind: "exec_request".to_string(),
        request_id: request_id.clone(),
        cmd: request.cmd,
        args: request.args,
        stdin: request.stdin,
        timeout_secs: request.timeout_secs,
        token,
    };

    socket
        .send(Message::Text(serde_json::to_string(&frame)?))
        .await?;

    loop {
        let next = tokio::time::timeout(std::time::Duration::from_secs(45), socket.next()).await;
        let message = match next {
            Ok(Some(Ok(message))) => message,
            Ok(Some(Err(err))) => return Err(err.into()),
            Ok(None) => return Err("websocket closed before exec response".into()),
            Err(_) => return Err("websocket exec timeout".into()),
        };

        match message {
            Message::Text(text) => {
                let parsed = match serde_json::from_str::<WsExecResponse>(&text) {
                    Ok(parsed) => parsed,
                    Err(_) => continue,
                };

                if parsed.kind != "exec_response" || parsed.request_id != request_id {
                    continue;
                }

                if let Some(error) = parsed.error {
                    return Err(error.into());
                }

                return Ok(ExecResponse {
                    exit_code: parsed.exit_code.unwrap_or(-1),
                    stdout: parsed.stdout.unwrap_or_default(),
                    stderr: parsed.stderr.unwrap_or_default(),
                });
            }
            Message::Close(_) => return Err("websocket closed during exec".into()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_local_success() {
        let result = execute_local("echo", vec!["hello".to_string()], None, None)
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn test_execute_local_with_stdin() {
        let result = execute_local("cat", vec![], Some("test input".to_string()), None)
            .await
            .unwrap();

        assert_eq!(result.exit_code, 0);
        assert!(result.stdout.contains("test input"));
    }

    #[tokio::test]
    async fn test_execute_local_failure() {
        let result = execute_local(
            "sh",
            vec!["-c".to_string(), "exit 42".to_string()],
            None,
            None,
        )
        .await
        .unwrap();

        assert_eq!(result.exit_code, 42);
    }
}
