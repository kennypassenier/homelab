# Tier 1: CLIENT (Desktop TUI)

The CLIENT is a local desktop application that acts as the primary control center, fully replacing the old `client.sh` and all legacy management scripts. All interactive management, scaffolding, and deployment logic is now implemented in Rust using a premium Ratatui TUI. **client.sh is deprecated and must not be used.**

## 1. Premium UI/UX (Ratatui)
- [x] **Hyper-Modern Interface:** The terminal application is built using `ratatui` and must look visually stunning. 
- [x] **Styling & Feedback:** Uses a centralized styling module with dynamic colors (Cyan/Magenta accents, dark grey backgrounds), rounded borders (`BorderType::Rounded`), and animated spinners for loading states (e.g., waiting for API triggers or Git pushes).


## 2. Feature Parity & Advanced Scaffolding
- [x] **Dynamic Scaffolding:** Replaces `create-new-stack.sh` and `create-new-app.sh` ([3], [4]). The Rust CLIENT's Scaffolding module is now strictly responsible for automatically creating all required bind-mount directories on the Proxmox host via API/SSH before a stack is deployed. **pre-sync.sh no longer handles directory creation.**
- [x] **Standard Boilerplate:** Generated templates automatically include:
	- Traefik labels for dynamic routing (replacing Nginx Proxy Manager)
	- Promtail configurations for log shipping ([5])
	- Watchtower auto-update labels (`com.centurylinklabs.watchtower.enable=true`) ([6])
- [x] **MAC Address Generator:** To prevent DHCP conflicts ([7]), the client generates safe, random Locally Administered MAC addresses for new LXCs.
- [x] **Idempotent SSH Management:** Replaces `add-ssh.sh` ([8]). Securely parses the local `~/.ssh/config` file and adds or updates SSH aliases for new LXCs without duplicating entries or corrupting the file ([9]).


### Advanced Scaffolding Requirements
- [ ] **Traefik Labels:** docker-compose.yml generation must support dynamic Traefik routing labels for all web services, fully replacing Nginx Proxy Manager.
- [ ] **Watchtower Hooks:** Support for Watchtower lifecycle labels (e.g., `com.centurylinklabs.watchtower.lifecycle.pre-check`) to prevent updates during active usage (e.g., active Jellyfin streams).
- [ ] **Custom Healthchecks & Dependencies:** Support for `depends_on` with `condition: service_healthy` to enforce correct startup order (e.g., VPN/qBittorrent).
- [ ] **Permissions:** Support for setting `user`, `group`, and `cap_add` (capabilities) for hardware and network access in generated templates.
- [ ] **VPN Network Namespaces:** Support for injecting `network_mode: service:<vpn>` for VPN kill-switch routing in generated templates.
- [ ] **Automated Restarts:** Ensure standard Docker Compose healthchecks and Watchtower restart policies are included in all generated templates.
- [ ] **App Creation: Multiselect Defaults:** After Docker usage is confirmed, show a popup multiselect for default containers (e.g., Watchtower, Promtail), with Watchtower and Promtail pre-selected. If selected, auto-generate their service in the stack if not present.
- [ ] **Stack Actions: Conditional Add:** In stack actions, show "Add Watchtower" and "Add Promtail" only if they do not already exist in the stack.
- [ ] **All Prompts as Popups:** All prompts, confirmations, and error messages are implemented as popup modals using Ratatui conventions (not Bash style).


## 3. Security, Validation & GitOps
- [x] **Pre-Flight Linting:** Parses and validates all YAML configurations using `serde_yaml` locally before allowing a `git push`.
- [ ] **Manual GitOps Sync:** No automatic push after destructive actions. A "Sync" or "Save" action is available in the UI to stage, commit, and push all pending changes when the user chooses.
- [ ] **Blast Radius Protection:** Replaces `remove-app.sh` and `remove-stack.sh` ([10], [11]). When deleting an app or stack, a stark red floating modal with a 3D drop-shadow appears ([12]). The user must type the exact name of the app/stack to confirm ([13], [14]). Once confirmed, the client deletes the folder from Git, but does not push until the user triggers sync.

## 4. API & Telemetry
- [x] **HTTP Push API:** Instead of waiting for a 5-minute cron job ([15]), the CLIENT sends an HTTP POST request (secured with a Bearer token) directly to the LXC daemon to trigger an immediate deployment.
- [ ] **Live SSE Telemetry:** Establishes a Server-Sent Events (SSE) connection to the LXC daemon. Deployment logs are streamed live to the bottom of the desktop UI.

## 5. Updates & Maintenance
- [x] The CLIENT application is built and tested locally via `cargo test` and `cargo build`. GitHub Actions strictly runs unit tests (e.g., testing MAC generators and idempotent parsers) when the `client-app/` directory is updated.

---

**Legend:**
- [x] = Complete
- [ ] = Not yet implemented or not fully integrated in TUI

---


## Implementation Details & Mapping

| Legacy Script/Feature   | CLIENT Rust Feature/Module                | Status |
|------------------------|-------------------------------------------|--------|
| client.sh              | Main TUI, tab navigation, all workflows   | [x]    |
| pre-sync.sh (dir create)| Directory creation now handled by Rust CLIENT Scaffolding | [x]    |
| create-new-stack.sh    | Scaffolding: stack creation, templates    | [x]    |
| create-new-app.sh      | Scaffolding: app creation, templates      | [x]    |
| remove-app.sh          | Blast Radius modal, app deletion          | [ ]    |
| remove-stack.sh        | Blast Radius modal, stack deletion        | [ ]    |
| add-ssh.sh             | SSH config management (POSIX only)        | [x]    |

---

## References

1. 5-min GitOps cron job: node-sync.sh
2. client.sh (legacy entrypoint)
3. scripts/client/
4. scripts/client/create-new-stack.sh
5. scripts/client/create-new-app.sh
6. Promtail: log shipping
7. Watchtower: auto-update
8. MAC address conflicts
9. scripts/client/add-ssh.sh
10. SSH config idempotency
11. scripts/client/remove-app.sh
12. scripts/client/remove-stack.sh
13. Red modal confirmation
14. Exact name confirmation
15. GitOps garbage collection

See `refactor/phase1.md` and `refactor/refactor-features.md` for full requirements.

# Tab Requirements (Detailed)

## Dashboard Tab
- **Live SSE Telemetry:** Real-time stream of deployment logs from the LXC daemon via Server-Sent Events (SSE).
- **Manual Deployment Triggers:** Interface to trigger the HTTP Push API for immediate deployment (secured with Bearer token).
- **Visual Feedback:** Animated spinners for loading states and dynamic Cyan/Magenta color palette.

## Scaffolding Tab
- **Dynamic App & Stack Creation:** Interface to scaffold new stacks and apps, generating docker-compose.yml with Traefik, Watchtower, and Promtail boilerplate.
- **Advanced Configurations:** Support for custom healthchecks, permissions (user/group/capabilities), and VPN network namespaces (e.g., network_mode: service:gluetun).
- **MAC Address Generator:** Generate safe, random Locally Administered MAC addresses for new LXCs.
- **Pre-Flight Linting & GitOps:** YAML validation (serde_yaml), auto-stage, commit, and push to main branch.
- **Blast Radius Protection:** Deletion triggers a red floating modal with 3D drop-shadow, requiring exact name confirmation and Git commit for removal.

## Host Management Tab
- **Idempotent SSH Management:** Manage local access to Proxmox containers by parsing and updating ~/.ssh/config safely and in-place, without duplicates or corruption.

---
