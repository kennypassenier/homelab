# Fase 2: De LXC Daemon (Tier 3)
**Doel:** De GitOps engine die in elke LXC draait als een Docker-container. Beheert automatische deployments, enforceert atomaire transacties voor mounts, en communiceert met de Docker API voor fail-safe rollbacks.

## Stap 2.1: Project Set-up & Basis API
**Copilot Prompt:**
`I am building a Rust-based GitOps Daemon that runs inside an LXC container (Tier 3).
Task 1: Initialize a new cargo project named lxc-daemon.
Task 2: Add crates: ratatui, crossterm, tokio, axum, bollard, git2, anyhow, color-eyre, and fs2.
Task 3: Write a main.rs that sets up a Ratatui terminal backend using crossterm, WITH a color-eyre panic hook that calls disable_raw_mode().
Task 4: Implement a basic TUI loop that shows a Dashboard tab with dummy stats and a log viewer pane.`

## Stap 2.1b: Premium UI/UX & Styling Requirements
**Copilot Prompt:**
`CRITICAL UI/UX INSTRUCTIONS: Just like the Client application, the Ratatui interface for this LXC Daemon MUST be hyper-modern, highly polished, and visually stunning. 
Task 1: Implement a centralized styling module with vibrant but professional colors (e.g., Cyan/Magenta accents, dark grey backgrounds).
Task 2: All Layout blocks must use rounded borders (BorderType::Rounded) with appropriate padding. 
Task 3: Implement visual feedback: active tabs must be highlighted, background sync states must use animated spinners in the UI, and error logs must stand out in high-contrast Red.
Task 4: Modals must render as floating, centered pop-ups with a shadow effect.`


## Stap 2.2: File-Locking & De HTTP Push-API
**Copilot Prompt:**
`We need to implement a concurrency lock and an HTTP API server using axum.
Task 1: Create a SyncManager struct that manages the deployment state. 
Task 2: Implement a method acquire_lock() using the fs2 crate on a temporary file (e.g., /var/lock/lxc-sync.lock). If the lock is held, immediately return HTTP 429 Too Many Requests.
Task 3: Set up an axum router with a POST /api/sync endpoint, protected by a Bearer token.
Task 4: The endpoint must trigger the sync process (spawning a background Tokio task) and return an SSE stream (axum::response::sse::Event). The background task sends deployment logs to this stream.`

## Stap 2.3: GitOps Sparse Checkout Logic
**Copilot Prompt:**
`We need to implement the GitOps synchronization logic using git2.
Task 1: Create a function sync_git_repo(repo_url: &str, target_dir: &str, stack_name: &str) -> Result<(), anyhow::Error>.
Task 2: Perform a clone or fetch, then a hard reset to origin/main, discarding local changes.
Task 3: CRITICAL RULE: Implement a Git Sparse-Checkout restricted strictly to the stacks/{stack_name}/ directory.
Task 4: Log each step to a shared channel broadcasted via the SSE stream.`

## Stap 2.4: Atomaire Blast Radius Directory Validatie
**Copilot Prompt:**
`We need to implement a strict atomic pre-flight check before starting any Docker containers.
Task 1: Create a function validate_storage_mounts(base_dir: &str) -> Result<(), anyhow::Error>.
Task 2: The architecture dictates a 2-folder structure: {base_dir}/docker/ (code from Git) and {base_dir}/config/ (persistent data bind-mounted from Proxmox).
Task 3: Verify that {base_dir}/config/ exists and is writable. 
Task 4: Implement a Linux-specific check (using std::os::unix::fs::MetadataExt) to verify that the st_dev (device ID) of {base_dir}/config/ is DIFFERENT from the st_dev of {base_dir}/. If they are the same, the bind-mount failed.
Task 5: If the mount check fails, return a critical error that aborts the deployment.`

## Stap 2.5: Ephemeral Secrets Container (Fail-Closed)
**Copilot Prompt:**
`Before deploying apps, we must securely fetch secrets using an ephemeral SECRETS container.
Task 1: Create a function fetch_secrets_ephemeral(stack_name: &str) -> Result<(), anyhow::Error> using bollard.
Task 2: Start a temporary Docker container (image: your-registry/secrets-cli:latest) with a bind-mount to {base_dir}/config/.
Task 3: Wait for the container to exit. If the exit code is NOT 0, return a critical error to abort the entire deployment (Fail-Closed). Do not proceed to start any stack containers.`

## Stap 2.6: Docker API Integratie & Rollbacks
**Copilot Prompt:**
`We need to implement Docker lifecycle management using bollard.
Task 1: Create a DockerEngine struct connecting to /var/run/docker.sock.
Task 2: Write deploy_stack(compose_path: &str) that reads GitOps YAML.
Task 3: Use bollard to pull images, stop existing containers, and start new ones.
Task 4: CRITICAL: Implement rollback logic. If the start command fails or a container crashes within 10 seconds, automatically attempt to restart the previously running image IDs and return an Error.
Task 5: Send events to the SSE stream.`

## Stap 2.6b: Pre-Deploy Hooks & Webhook Alerting
**Copilot Prompt:**
`We need to implement Pre-Deploy hooks and Webhook alerting for rollbacks.
Task 1: Before triggering the bollard deployment, check if a file named 'hooks/setup.sh' exists in the target stack directory. If it exists, execute it using tokio::process::Command. This replaces the old bash 'pre-sync.sh' logic for creating external Docker networks.
Task 2: If the execution of the hook fails, abort the deployment and return an Error.
Task 3: Implement an alerting mechanism. If a deployment fails and triggers the rollback logic from Step 2.6, use 'reqwest' to send an HTTP POST request to a Webhook URL (e.g., Discord or Ntfy) loaded from the system environment variables. Inform the user about the exact stack that failed and triggered a rollback.`


## Stap 2.7: Garbage Collection (Git = God)
**Copilot Prompt:**
`We need to implement safe Garbage Collection.
Task 1: Create perform_garbage_collection(current_git_apps: Vec<String>, deployed_apps: Vec<String>, force_delete: bool).
Task 2: Detect orphaned apps (running in Docker but missing in Git).
Task 3: Stop and remove orphan containers using bollard.
Task 4: CRITICAL: Do NOT delete the persistent data in ../config/ UNLESS force_delete is true (passed from Client API). If false, just log a warning.`

## Stap 2.8: Genereer Documentatie
**Prompt 1 (Menselijke Wiki):**
`Write docs/wiki/lxc-daemon.md. Explain how it runs as a Docker container, the HTTP API, the 2-folder structure validation (/docker vs /config), the ephemeral secrets container, and the Git=God Garbage Collection.`
**Prompt 2 (LLM Context):**
`Write docs/LLM_CONTEXT_LXC_DAEMON.md for AI Agents. State boundaries (NEVER touches Proxmox host, uses bollard). Detail concurrency rules (fs2 lock), SSE telemetry, and rollback conditions.`
