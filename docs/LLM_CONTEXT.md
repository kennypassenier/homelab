# LLM Context (Current)

Last updated: 2026-06-12

## Architecture Summary

- CLIENT: sole interactive orchestrator (Ratatui).
- HOST: headless Proxmox-side execution tier.
- LXC: headless stack runtime tier.
- Communication is CLIENT->HOST and CLIENT->LXC only.

## Operational Rules

- GitOps-first: change source in repo, then deploy via orchestration.
- Prefer fail-closed behavior for pre-flight and deploy gates.
- Keep stack operations idempotent and module-driven.

## Sparse Checkout Rule

- LXC Git working copy must be restricted to stacks/<stack_name>/ only.
- Sparse scope should be re-applied during sync to avoid drift.

## GPU Passthrough Rule

- Actual passthrough is HOST responsibility.
- CLIENT can update app compose wiring and write hardware hints to lxc-compose.

## Documentation State

- Implemented use cases are stored under docs/usecases/implemented/.
- Remaining gaps are tracked under docs/usecases/pending/.
- Future ideas and roadmap candidates are tracked under docs/usecases/planned/.
- Legacy refactor phase documents were retired.

## Delivery Model Updates

- HOST self-update is release-based (version/tag check), not push-based.
- LXC daemon image delivery is automated via GHCR workflow with path-based change gating.
- Local `make build-lxc` builds the daemon inside a Debian 12 Rust container so the generated artifact stays compatible with the libc versions found in deployed LXCs.
- LXC bootstrap installs a prebuilt Debian-12-compatible `latch` binary (asset `latch-linux-x86_64-lxc.tar.gz`) pushed by HOST, then uses persistent `LATCH_PAT` / `LATCH_KEY` for headless operation; pass/keyring inside LXCs is optional.
- HOST and LXC both expose installed `latch` binary version and update status via API: `GET /api/version` (HOST) and `GET /api/secrets/keyring` (LXC) include `latch_version` and `latch_last_update_secs` fields for operational visibility.
- CLIENT stack wizard sets CPU/RAM/Disk defaults in `lxc-compose.yml` and keeps deploy disabled until explicit activation.
- CLIENT streams live deploy logs from the LXC daemon during sync actions.
- CLIENT app rows include a Git-managed config editor for Docker image updates.
- CLIENT can sync stack-owned DHCP reservations to OPNsense from `lxc-compose.yml` network intent.
- CLIENT Host Management uses HOST metrics API polling (`GET /api/metrics`, target `HOST_IP`) and displays runtime LXC status/CPU/RAM/uptime.
- LXC failsafe sync uses an inverse heartbeat policy: periodic windows run recovery only when CLIENT heartbeat is stale; windows are skipped while CLIENT is actively connected.
- CLIENT now supervises websocket workers for all deploy-enabled stacks and reconnects stale streams automatically.
- CLIENT/HOST/LXC websocket streams now exchange keepalive traffic so idle periods do not drop otherwise healthy connections.
- HOST provisioning now fail-closes stack activation: when CREATE/RECREATE/UPDATE fails for a stack, HOST writes `deploy.enabled=false` and `deploy.last_failure` into that stack `lxc-compose.yml` so retries require explicit re-enable.
- HOST provisioning now resumes partially bootstrapped existing LXCs (`RESUME_BOOTSTRAP`) when bootstrap artifacts are missing, instead of treating them as fully `OK`.
- Latch deployment to Proxmox LXCs is prebuilt-binary only: no in-container Rust toolchain/build-essential installation; HOST pushes binary + wrapper and wrapper verifies runtime compatibility before login.
- LXC websocket endpoint supports command RPC (`exec_request`/`exec_response`) in addition to log streaming.
- HOST and LXC websocket endpoints now support immediate `update_request` triggers; LXC also exposes `POST /api/update` for GHCR image pull + recreate flows.
- HOST now receives CLIENT heartbeats via websocket RPC (`client_heartbeat`) with HTTP `POST /api/heartbeat` fallback; HOST failsafe uses this API-level liveness signal.
- HOST daemon runs headless-only in deployed operation.
- CLIENT uses websocket RPC over LXC `/api/logs/ws` for sync, restore, heartbeat, and command execution, with HTTP endpoints retained as compatibility fallback.
- HOST and LXC now emit startup lifecycle logs containing `daemon_version=...`; CLIENT surfaces version detection and version-change events in Logs.
