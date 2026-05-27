# Use Case: TUI Deployment Modal Progress

**Tier:** CLIENT (Ratatui rendering) — consumes SSE streams from HOST and LXC  
**Status:** Specification — not yet implemented  

---

## 1. Overview

The deployment modal is the primary real-time feedback surface in the CLIENT TUI. It renders during any long-running operation: stack provisioning, sync, backup, restore, OS patching, and batch deployments.

The modal is implemented with [Ratatui](https://ratatui.rs/) and consumes one or more SSE streams to drive widget updates. It is the single canonical rendering specification for all operation feedback in the system.

---

## 2. Layout

```
╔═══════════════════════════════════════════════════════════════════════╗
║  Deploying Stacks (3 of 7)                              [ESC to hide] ║
╠═══════════════════════════════════════════════════════════════════════╣
║ STACKS                          DETAILS                               ║
║ ─────────────────────────────   ──────────────────────────────────── ║
║ ✓ cloudflared                   Stack: media                          ║
║ ⟳ media             ←active    Phase: LXC bootstrap exec              ║
║ ○ monitoring                    ──────────────────────────────────── ║
║ ○ paperless                     [▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░] 25%    ║
║ ○ downloader                    ──────────────────────────────────── ║
║ ○ gateway                       LOG                                   ║
║ ○ vikunja                       ts=... component=host msg="apt-get..." ║
║                                 ts=... component=host msg="Docker in.."║
║                                 ts=... component=host msg="lxc-daemon"║
║                                 ts=... component=lxc  msg="git pull"  ║
║                                 ts=... component=lxc  msg="docker com"║
╠═══════════════════════════════════════════════════════════════════════╣
║ Elapsed: 00:02:34  ETA: ~00:07:00   Completed: 1/7   Failed: 0       ║
╚═══════════════════════════════════════════════════════════════════════╝
```

### Panes

| Pane | Content |
|---|---|
| **Stacks list** (left) | All stacks in the operation with status icons |
| **Details** (top right) | Current stack name, current phase label, progress bar |
| **Log** (bottom right) | Scrolling logfmt event stream for the selected stack |
| **Status bar** (bottom) | Elapsed time, ETA, completed/failed count |

---

## 3. Stack Status Icons

| Icon | Meaning |
|---|---|
| `○` | Not started |
| `⟳` | In progress (animated spinner — cycles: ⠋ ⠙ ⠹ ⠸ ⠼ ⠴ ⠦ ⠧ ⠇ ⠏) |
| `✓` | Completed successfully |
| `✗` | Failed |
| `↻` | Rebooting (LXC kernel update restart in progress) |
| `⚠` | Completed with warnings (e.g., unhealthy container) |

Spinner animation: Ratatui tick event at 100ms intervals advances the spinner frame.

---

## 4. Progress Bar

The progress bar represents phase progress within the current operation:

| Operation | Progress Source |
|---|---|
| Stack provision | Phase count (0-9 phases from add-stack.md) |
| Restic backup | Restic JSON output `files_done / files_total` |
| OS patch | `apt` output (package count) |
| Git pull | Unknown total → indeterminate bouncing bar |
| Docker compose pull | Image pull progress events from Bollard |

For **indeterminate** operations, the bar bounces left-to-right instead of filling.

---

## 5. SSE Stream Contract

The modal subscribes to SSE streams from HOST and/or LXC daemons. Events are logfmt text, one event per SSE `data:` field:

```
data: ts=2026-05-30T03:00:00Z level=info component=host stack=media phase=bootstrap msg="installing Docker CE"
data: ts=2026-05-30T03:00:12Z level=info component=lxc stack=media msg="git pull complete" sha=a1b2c3d
data: ts=2026-05-30T03:00:14Z level=info component=lxc stack=media app=jellyfin msg="container healthy"
data: ts=2026-05-30T03:00:15Z level=info component=lxc stack=media msg="sync complete" apps_healthy=4 duration_ms=14200
```

The CLIENT parses each logfmt event and:
- Appends the raw line to the log scroll pane for the matching stack.
- Updates the progress bar if the event contains a `phase` field.
- Transitions stack status icon if `msg` matches completion/failure patterns.

**Special event tags:**
| Tag | Value | Meaning |
|---|---|---|
| `progress_total` | integer | Sets progress bar maximum |
| `progress_current` | integer | Sets progress bar current value |
| `phase` | string | Updates current phase label |
| `level=error` | — | Turns log line red; increments failed indicator |

---

## 6. Multi-Stack Batch Layout

For batch operations (deploy-active-stacks, full-backup-restore, batch OS patch), the modal manages multiple concurrent SSE connections:

```rust
struct BatchModalState {
    stacks: Vec<StackProgress>,
    selected_stack_index: usize,          // which stack's logs are shown in detail pane
    semaphore: Arc<Semaphore>,             // limits concurrency (default 1, max 3)
    start_time: Instant,
}

struct StackProgress {
    name: String,
    status: StackStatus,
    phase: String,
    progress: Option<(u64, u64)>,          // (current, total)
    log_lines: VecDeque<LogLine>,          // ring buffer, max 200 lines
    sse_stream: Option<SseStream>,
}
```

The user navigates between stacks in the left pane using `↑/↓`. The right pane updates to show the selected stack's details and logs.

---

## 7. Keyboard Bindings

| Key | Action |
|---|---|
| `↑ / ↓` | Navigate stack list |
| `Enter` | Focus selected stack details |
| `r` | Retry failed stacks (when operation is complete) |
| `ESC` | Minimize to status bar (operation continues in background) |
| `q` | Force-quit (only when operation is complete; prompts confirmation if in progress) |
| `s` | Scroll log pane |
| `f` | Filter log by level (toggle: all / warn+error / error only) |

---

## 8. Minimized State

When the user presses `ESC`, the modal minimizes to a status bar at the bottom of the main TUI:

```
⟳ Deploy  media [⠴ LXC bootstrap]  2/7 stacks  00:03:12   [Enter to expand]
```

The status bar updates every 500ms.

---

## 9. Log Scroll Pane

The log pane is a Ratatui `Paragraph` with scroll state. Lines are color-coded:

| `level` | Ratatui style |
|---|---|
| `info` | Default foreground |
| `warn` | Yellow |
| `error` | Red, bold |
| `debug` | Dark gray (only shown when debug mode active) |

Maximum 200 log lines retained per stack (ring buffer). Oldest lines are dropped when the buffer is full. The pane auto-scrolls to the bottom unless the user has manually scrolled up.

---

## 10. ETA Calculation

```rust
fn estimate_eta(completed: usize, total: usize, elapsed: Duration) -> Duration {
    if completed == 0 { return Duration::from_secs(0); } // unknown
    let rate = elapsed.as_secs_f64() / completed as f64;  // seconds per stack
    let remaining = (total - completed) as f64 * rate;
    Duration::from_secs_f64(remaining)
}
```

ETA is displayed as `~00:07:00` (approximation; prefixed with `~`). If `completed == 0` and more than 30s have elapsed, the ETA shows `unknown`.

---

## 11. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `deploy-active-stacks.md` | Primary consumer of this modal (batch deploy) |
| `full-backup-restore.md` | DR wizard uses this modal |
| `manual-backup-all.md` | Backup progress rendered here |
| `os-patching.md` | Batch OS patch progress rendered here |
| `error-warning-logging.md` | Log line color-coding based on logfmt `level` |
| `error-handling-fail-closed.md` | Red state rendering on fail-closed events |
