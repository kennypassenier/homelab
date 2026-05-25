

# Tier 1: CLIENT (Desktop Control Center)

The CLIENT is a local desktop application that acts as the primary control center, replacing the old `client.sh` and related bash scripts ([1], [2]).

## 1. Premium UI/UX (Ratatui)
- [x] **Hyper-Modern Interface:** The terminal application is built using `ratatui` and must look visually stunning. 
- [x] **Styling & Feedback:** Uses a centralized styling module with dynamic colors (Cyan/Magenta accents, dark grey backgrounds), rounded borders (`BorderType::Rounded`), and animated spinners for loading states (e.g., waiting for API triggers or Git pushes).

## 2. Feature Parity & Scaffolding
- [x] **Dynamic Scaffolding:** Replaces `create-new-stack.sh` and `create-new-app.sh` ([3], [4]). Dynamically generates `docker-compose.yml` templates using Askama/Tera.
- [x] **Standard Boilerplate:** Generated templates automatically include Traefik labels (replacing Nginx Proxy Manager), Promtail configurations for log shipping ([5]), and Watchtower auto-update labels (`com.centurylinklabs.watchtower.enable=true`) ([6]).
- [x] **MAC Address Generator:** To prevent DHCP conflicts ([7]), the client generates safe, random Locally Administered MAC addresses for new LXCs.
- [x] **Idempotent SSH Management:** Replaces `add-ssh.sh` ([8]). Securely parses the local `~/.ssh/config` file and adds or updates SSH aliases for new LXCs without duplicating entries or corrupting the file ([9]).

## 3. Security, Validation & GitOps
- [x] **Pre-Flight Linting:** Parses and validates all YAML configurations using `serde_yaml` locally before allowing a `git push`.
- [x] **Automated Git Lifecycle:** Automatically stages, commits (with auto-generated commit messages), and pushes newly scaffolded stacks/apps to the `main` branch.
- [ ] **Blast Radius Protection:** Replaces `remove-app.sh` and `remove-stack.sh` ([10], [11]). When deleting an app or stack, a stark red floating modal with a 3D drop-shadow appears ([12]). The user must type the exact name of the app/stack to confirm ([13], [14]). Once confirmed, the client deletes the folder from Git, commits, and pushes to trigger automatic Garbage Collection.

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

| Old Bash Script         | CLIENT Rust Feature/Module                | Status |
|------------------------|-------------------------------------------|--------|
| client.sh              | Main TUI, tab navigation, all workflows   | [x]    |
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
