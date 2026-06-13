# CLIENT Features (Current)

Last updated: 2026-06-12

## Scope

- Ratatui TUI for all interactive flows.
- Stack/app scaffolding, activation/deactivation, deploy/update queueing.
- Backup/restore/patch orchestration surfaces.
- Structured client logfmt-style event emission for critical operations.
- CLIENT keeps the full session log stream because HOST and LXC now bound their own replay histories.
- Live deploy telemetry streamed from a single persistent LXC daemon WebSocket path during sync actions (no secondary replay stream).
- Persistent websocket supervision for HOST plus all deploy-enabled stacks, with auto-reconnect for stale/no-signal streams.
- Sync dispatch is now gated on HOST `lxc_ready` signals so CLIENT never sends LXC sync requests before HOST bootstrap finishes.
- Client-side provisioning dispatch debounce suppresses duplicate HOST provision requests during reconnect/key-repeat storms.
- Logs tab ingestion suppresses identical consecutive LXC log lines within a short window to reduce repeated warning noise.
- CLIENT control-plane actions now use websocket RPC on LXC `/api/logs/ws` for sync, restore, heartbeat, and remote command execution (with HTTP fallback kept for compatibility).
- When initial sync dispatch fails because LXC is still bootstrapping (connection refused/transport errors), CLIENT now queues an automatic retry and re-dispatches sync immediately after that stack websocket connects.
- CLIENT now attaches one-shot latch pull context (`PAT` / `KEY` / `REPO` / `project` / optional `env` / `sparse`) to HOST and LXC update requests and to LXC sync requests.
- Session heartbeat pulses to LXC daemons while CLIENT is running, used to suppress unnecessary failsafe sync windows.
- Session heartbeat pulses to HOST now use websocket RPC (`client_heartbeat`) with HTTP `POST /api/heartbeat` fallback.
- Live HOST connectivity polling via HOST metrics API (`GET /api/metrics`) with runtime node/LXC status in the Host Management tab.
- Logs tab source focus mode (Shift+f) to isolate one source without dropping global log ingestion.
- CLIENT detects `daemon_version=` markers from HOST/LXC websocket logs and emits explicit version-detected/version-changed log lines.
- Update tab cards now render per-target metadata: detected daemon version plus last manual update outcome/timestamp for HOST and each LXC stack.
- Top tab bar keeps the glitch treatment while other sections render stable titles; selected tab uses a filled highlight style for clearer focus.
- Update tab cards include richer action context (trigger key/target and live state `idle|updating`) alongside version and last-result telemetry.
- Update tab now shows latest available HOST release tag (GitHub `host-daemon-v*`) with refresh timestamp, plus LXC update channel visibility (image/tag target used by self-update).
- CLIENT now auto-loads environment from `config/.env` (or `CLIENT_ENV_FILE` override).

## Implemented Highlights

- Add/delete stack and add/delete app flows.
- Core app management.
- Deploy selected and batch deploy/update of active stacks.
- Fail-closed pre-sync and filesystem-layout validation gates.
- Transaction ledger for add_stack and delete_stack phases.
- Reusable operation progress modal used by backup/restore/patch actions.
- GPU compose wiring toggles per selected app (g/G) and host hint writes to lxc-compose.
- Stack creation wizard now captures provisioning defaults (CPU 1-8, memory in 512 MiB steps, root disk GiB) and writes them into stack `lxc-compose.yml`.
- Stack creation wizard now captures boot policy defaults (autostart + boot order) and writes them into stack `lxc-compose.yml`.
- Stack creation wizard now requires VMID `101..354` and deterministically derives reserved IPv4 as `STACK_IP_PREFIX + (vmid - 100)` (default prefix `10.10.10.`).
- Stack creation wizard now includes a per-stack Promtail toggle before final review; stack scaffold creates watchtower + traefik always and only adds promtail when selected.
- After stack creation, CLIENT now auto-opens the app creation wizard for that stack.
- App creation wizard flow is now: app name -> optional Traefik subdomain (empty disables Traefik labels) -> review -> create, then loops back to app name so multiple apps can be added without leaving the modal.
- Stack config editor allows stack-level editing of deploy state, resources, hostname, MAC address, IP mode, and reserved IPv4 from the Scaffolding tab.
- Stack config editor allows stack-level editing of autostart and boot order policy.
- Stack config editor can sync stack-owned DHCP reservations to OPNsense Kea using the stack's deterministic MAC address and reserved IPv4 intent.
- App rows now expose a real config editor for Git-managed app metadata, starting with Docker image updates.
- New stack defaults explicitly set `deploy.enabled=false` to keep manual activation as the safe default.
- Latch clone orchestration module can perform offer/create/apply credential sync through local + LXC command execution.
- Remote command execution now prefers websocket RPC over LXC `/api/logs/ws`, with HTTP `/api/exec` fallback for compatibility.
- LXC naming standardization supports canonical `vmid-app-<stack>` hostnames while preserving legacy alias compatibility.

## Notes

- CLIENT remains GitOps-first and commits generated changes through the existing Git helper path.
- HOST-only operations (for example real GPU passthrough on Proxmox) are represented as CLIENT orchestration intent, not direct local host mutation.
- DHCP automation only mutates reservations proven to be homelab stack-owned; unrelated/manual reservations are treated as hard conflicts.
- Host Management data now comes from the HOST daemon metrics API (`GET /api/metrics`), including runtime LXC rows and uptime telemetry; the tab no longer relies on synthetic host-state rows.
