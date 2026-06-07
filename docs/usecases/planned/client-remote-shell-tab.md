# Planned: CLIENT Remote Shell Tab

**Status:** Planned  
**Priority:** Medium  
**Owner:** TBD  
**Date Created:** 2026-06-07

---

## 1. Goal

Enable interactive remote shell access to HOST or any specific LXC instance directly from the CLIENT TUI, eliminating the need for separate SSH sessions and supporting full terminal interactivity (passwords, interactive prompts, command output streaming).

**User story:**
> As an operator, I want to open a "Shell" tab in CLIENT, select a target (HOST or specific LXC), type commands, and see live output — all without opening a separate SSH terminal. If the remote asks for a password or input, I should be able to type directly into CLIENT's shell view.

---

## 2. Current State

### What Exists Today

- **LXC daemon WebSocket RPC:** `exec_request` / `exec_response` supports **one-shot command execution** with full stdout/stderr capture and exit code
- **HOST daemon WebSocket:** Supports log streaming and update RPC, but no exec capability
- **CLIENT TUI:** Has Logs, Deploy, Stack Management tabs; no Shell tab yet
- **CLIENT WebSocket supervision:** Maintains persistent websocket connections to all deploy-enabled LXCs and HOST with auto-reconnect

### What's Missing

1. **Streaming stdin during exec:** Current RPC is one-shot (send cmd, get output). Need bidirectional stdin/stdout stream.
2. **Terminal mode control:** No TTY allocation or terminal feature negotiation (echo, canonical mode, raw mode for password entry).
3. **Shell tab UI:** No TUI component for shell interaction (input field, scrollable output buffer, cursor management).
4. **HOST shell support:** HOST daemon has no exec endpoint; needs to be added.
5. **Interactive prompts:** No mechanism to handle password prompts, `[y/n]` confirmations, or other interactive CLI patterns.

---

## 3. Proposed Design

### 3.1 Architecture Overview

```
CLIENT TUI
  │
  ├─ Shell Tab (new)
  │   ├─ Target selector (dropdown/radio: HOST / LXC:vmid)
  │   ├─ Shell input field (single-line command entry or multi-line)
  │   ├─ Output buffer (scrollable, ANSI color preserved)
  │   └─ Status bar (connection state, exit code, etc.)
  │
  ├─ WebSocket to HOST (existing, extend with shell)
  │   └─ exec_stream_request
  │       ├─ stdin: "command\n"
  │       ├─ tty_mode: bool (request pseudo-TTY)
  │
  └─ WebSocket to LXC (existing, extend with shell)
      └─ exec_stream_request
          ├─ stdin: "command\n"
          ├─ tty_mode: bool (request pseudo-TTY)
```

### 3.2 Protocol Extension

**New WebSocket message types:**

```json
{
  "kind": "exec_stream_request",
  "request_id": "shell-001",
  "cmd": "/bin/bash",           // or "/bin/sh" for simpler shells
  "args": ["-i"],               // interactive shell
  "tty_mode": true,             // request PTY allocation
  "timeout_secs": null,         // no timeout for interactive shell
  "env": {
    "TERM": "xterm-256color",   // for color support
    "COLUMNS": 120,
    "LINES": 40
  }
}
```

**Response format (streaming):**

```json
{
  "kind": "exec_stream_response",
  "request_id": "shell-001",
  "stream_event": "started",    // or "data", "error", "exit"
  "exit_code": null,
  "stdout_chunk": "user@lxc:~$ ",
  "stderr_chunk": null,
  "exit_code": 0                // set on "exit" event
}
```

**Client sends stdin while session is open:**

```json
{
  "kind": "exec_stream_stdin",
  "request_id": "shell-001",
  "data": "ls -la\n"            // user input
}
```

### 3.3 Shell Tab UI (Ratatui)

```
┌─ CLIENT v0.2 ────────────────────────────────────────────────────────────┐
│ [Logs] [Deploy] [Stacks] [Shell]                                          │
├───────────────────────────────────────────────────────────────────────────┤
│ Target: [HOST ▼] (or [LXC: node-1 ▼])                    [Connect] [Exit] │
├───────────────────────────────────────────────────────────────────────────┤
│ user@host:~$ ls -la                                                       │
│ total 48                                                                   │
│ drwxr-xr-x 5 user group 4096 Jun 7 10:15 .                               │
│ drwxr-xr-x 3 root root   4096 Jun 5 14:22 ..                             │
│ -rw-r--r-- 1 user group  220 Mar 27  2025 .bash_logout                   │
│ -rw-r--r-- 1 user group 3526 Mar 27  2025 .bashrc                        │
│ user@host:~$ █                                                             │
│                                                                             │
│ Status: Connected to HOST | Exit Code: — | Ready                           │
└───────────────────────────────────────────────────────────────────────────┘
```

**Features:**
- **Target selector:** Dropdown to switch between HOST and available LXCs (populated from metrics)
- **Command input:** Single-line at bottom (Ratatui TextBox or custom widget) or multi-line for complex commands
- **Output buffer:** Scrollable text area with ANSI color support (existing Ratatui `Paragraph` can handle this)
- **Status bar:** Shows connection state, last exit code, prompt readiness
- **Auto-scroll:** Follows tail of output unless user scrolls up (standard terminal UX)
- **Ctrl+C handling:** Sends SIGINT through WebSocket to remote process

### 3.4 Backend Changes (HOST Daemon)

**New endpoint/handler:**

```rust
// In host-daemon/src/api.rs or new host-daemon/src/shell.rs

async fn handle_exec_stream(
    headers: HeaderMap,
    State(app): State<Arc<Mutex<App>>>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_shell_session(socket, app))
}

async fn handle_shell_session(mut socket: WebSocket, app: Arc<Mutex<App>>) {
    // Spawn bash/sh with pty if tty_mode requested
    // Multiplex stdin/stdout/stderr over WebSocket messages
    // Handle SIGINT, SIGTERM gracefully
}
```

**Key implementation details:**
- Use `nix::pty::openpty()` to allocate a pseudo-terminal if `tty_mode: true`
- Use `tokio::process::Command` to spawn shell with pty file descriptors
- Multiplex stdin reads from WebSocket and process output to WebSocket using `tokio::select!`
- Track session state (running, exit code, errors) to avoid zombie processes

### 3.5 Backend Changes (LXC Daemon)

**Extend existing exec_request to support streaming:**

```rust
// In lxc-daemon/src/api.rs

// New message variant
#[derive(Deserialize)]
pub struct ExecStreamRequest {
    pub cmd: String,
    pub args: Option<Vec<String>>,
    pub tty_mode: Option<bool>,
    pub timeout_secs: Option<u64>,
    pub env: Option<HashMap<String, String>>,
}

// Handle in WebSocket message router
if let Some(ExecStreamRequest) = req.get("kind").and_then(|v| v.as_str()) {
    // Spawn cmd with pty, multiplex over WebSocket
}
```

### 3.6 CLIENT Changes (TUI & WebSocket)

**New tab:**

```rust
// In client-app/src/ui.rs or new client-app/src/shell_tab.rs

#[derive(Clone)]
pub struct ShellTab {
    pub target_selector: TargetDropdown,  // HOST / LXC choices
    pub command_input: String,
    pub output_buffer: VecDeque<String>,  // ring buffer of output lines
    pub connected: bool,
    pub current_session_id: Option<String>,
}

impl ShellTab {
    pub fn send_command(&mut self, cmd: &str, ws: &mut WebSocket) {
        let req = json!({
            "kind": "exec_stream_request",
            "cmd": "/bin/bash",
            "args": ["-i"],
            "tty_mode": true,
        });
        ws.send(Message::Text(req.to_string()));
        // Then continuously read from rx channel and append to output_buffer
    }
}
```

**WebSocket handler additions:**

```rust
// In client-app/src/ws_client.rs

// Subscribe to shell stream events
if event["kind"] == "exec_stream_response" {
    shell_tab.output_buffer.push_back(event["stdout_chunk"].as_str());
    if let Some(exit_code) = event.get("exit_code") {
        shell_tab.connected = false;
        // Mark session as ended
    }
}

// Send user input to remote
if user_pressed_enter_in_shell {
    let stdin_msg = json!({
        "kind": "exec_stream_stdin",
        "request_id": shell_tab.current_session_id,
        "data": format!("{}\n", shell_tab.command_input)
    });
    ws.send(Message::Text(stdin_msg.to_string()));
    shell_tab.command_input.clear();
}
```

---

## 4. Implementation Roadmap

### Phase 1: LXC Shell (Foundation)
1. Extend LXC daemon `exec` endpoint to support streaming stdin/stdout over WebSocket
2. Add `exec_stream_request` / `exec_stream_stdin` message handlers in lxc-daemon
3. Use `nix::pty` to allocate TTY, spawn command with pty
4. Test with basic commands (`ls`, `pwd`) and interactive input (`read -p "Name: "`)
5. Validate ANSI color escape sequences pass through unchanged

**Acceptance criteria:**
- Client can send `exec_stream_request` to LXC WebSocket
- LXC spawns bash in PTY mode and streams output back
- Client receives chunks and displays them in a temporary shell view
- Password prompt in `sudo` is visible and can be typed into

### Phase 2: HOST Shell
1. Add `POST /api/shell` or extend existing exec endpoint in HOST daemon
2. Implement same PTY + streaming logic in host-daemon
3. Extend CLIENT to show HOST as a target choice

**Acceptance criteria:**
- Shell tab target selector includes "HOST"
- Can execute commands on Proxmox host directly from CLIENT

### Phase 3: CLIENT Shell Tab UI
1. Add new "Shell" tab to main TUI nav
2. Create target selector (dropdown: HOST / [LXC list])
3. Create output buffer widget (scrollable text with ANSI colors)
4. Create command input field (bottom of tab)
5. Wire up keyboard events: Enter = send, Ctrl+C = interrupt, Tab = exit shell

**Acceptance criteria:**
- Shell tab renders properly
- Can type commands and see live output
- ANSI colors (from `ls --color=auto`, etc.) are preserved
- Ctrl+C sends SIGINT to remote process
- Scrolling works correctly (up arrow scrolls history, down arrow re-follows tail)

### Phase 4: Advanced Features (Future)
- **Shell history:** Save per-target command history, autocomplete
- **Terminal modes:** Support raw mode for interactive CLI tools (`vim`, `less`, etc.)
- **Resize handling:** Send SIGWINCH when CLIENT window resizes
- **Tab support:** Multiple shell tabs open simultaneously (one per target)
- **Copy/paste:** Support selecting and copying from output buffer
- **Sessions:** Reconnect to existing shell session if websocket drops
- **Script upload:** Drag-drop to shell to transfer scripts and run them

---

## 5. Technical Feasibility

### ✅ Fully Possible

- **PTY allocation in Rust:** `nix` crate provides `openpty()`, `grantpt()`, and FD setup
- **Bidirectional streaming:** WebSocket is inherently bidirectional; just needs message format
- **ANSI passthrough:** Terminal escape codes can be preserved as-is in JSON strings
- **Existing infrastructure:** WebSocket connections already established; no new network protocols needed

### ⚠️ Challenges

- **Terminal echo management:** Need to handle canonical vs raw mode correctly; some apps expect cooked input, others raw
- **Signal handling:** SIGINT/SIGTERM need to be reliably forwarded; may require signal-sending mechanism over WebSocket
- **TTY allocation on LXC side:** Commands executed inside LXC container need to inherit the PTY cleanly; verify with `docker exec -it` equivalent
- **Concurrent sessions:** If shell session doesn't close cleanly, orphaned processes could accumulate; need robust session cleanup

### 🟢 Mitigations

- Start with interactive bash/sh only; don't try to support `vim`, `less`, etc. initially
- Use explicit session IDs and timeouts to prevent zombie sessions
- Test thoroughly with `sudo`, password prompts, and multi-line input before release

---

## 6. Open Questions

1. **Default shell:** Should we always use `/bin/bash -i` or detect user's shell from `/etc/passwd`? Start with bash for simplicity.
2. **Authentication:** When shell to HOST is requested, should CLIENT prompt for sudo password, or rely on SSH key auth? (SSH keys already set up on HOST; use those.)
3. **Working directory:** What CWD should remote shell start in? (Default to user home; allow navigation with `cd`.)
4. **Terminal size:** How should COLUMNS/LINES be calculated from CLIENT viewport? (Start with 120x40; enhance later with SIGWINCH support.)
5. **Command history:** Should we store shell history locally, or rely on remote shell's history file? (Local storage in CLIENT state, plus remote shell history.)
6. **Multiple shells:** Should users be able to open multiple shell tabs? (Phase 4; start with single active shell.)

---

## 7. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|-----------|
| Orphaned processes if session not cleaned | Medium | High | Explicit timeout + cleanup handler in daemon |
| ANSI codes break message parsing | Low | Medium | Use JSON escaping; validate with tests |
| PTY allocation fails on LXC | Low | Medium | Test before commit; fall back to non-TTY mode |
| Client TUI freezes if output is too large | Medium | Low | Ring-buffer output; don't store entire history |
| Sudo password timeout | Low | Medium | Clarify in docs; users can retry |

---

## 8. Success Metrics

- [ ] Can type `ls -la` into shell and see output within 200ms
- [ ] Can type `sudo cat /etc/shadow` and respond to password prompt interactively
- [ ] Colors from `ls --color=auto` render correctly in output
- [ ] Ctrl+C terminates a long-running command (e.g., `sleep 60`)
- [ ] Can switch targets (HOST ↔ LXC) without losing command history
- [ ] Shell session times out and closes gracefully after 30 min of inactivity
- [ ] Operator feedback: "Feels like a local terminal, but remote"

---

## 9. Related Issues / Blockers

- None currently identified; all dependencies (WebSocket, PTY, command exec) are satisfied by existing stack

---

## 10. Timeline Estimate

- **Phase 1 (LXC shell):** 2–3 days (PTY setup, message handlers, testing)
- **Phase 2 (HOST shell):** 1 day (reuse LXC logic, add endpoint)
- **Phase 3 (UI):** 2–3 days (tab layout, input field, output buffer)
- **Total:** ~5–7 days for MVP

---

## See Also

- [docs/client-features.md](../client-features.md) — CLIENT capabilities overview
- [docs/lxc-features.md](../lxc-features.md) — LXC daemon capabilities overview
- [docs/host-features.md](../host-features.md) — HOST daemon capabilities overview
- Rust crates: `nix`, `tokio::process`, `ratatui`
