# Use Case: Post-Deploy Actions

**Tier:** LXC (health probes + rollback) + CLIENT (result display)  
**Status:** Specification — not yet implemented  

---

## 1. Overview

After every `docker compose up -d` call during a sync cycle, the LXC daemon performs a post-deploy validation phase. This phase:
1. Waits for containers to reach a healthy or running state.
2. Detects crash-looping containers within a 10-second observation window.
3. Triggers automatic rollback to the previous known-good image if a container crashes.
4. Sends an alert (Ntfy or Discord) if a deployment fails or a rollback occurs outside a CLIENT-triggered session.
5. Emits structured logfmt events that are forwarded to the CLIENT SSE stream.

---

## 2. Health Check Observation Window

After `docker compose up -d`, the LXC daemon observes each newly started or updated container for **10 seconds**:

```rust
let observation_deadline = Instant::now() + Duration::from_secs(10);
while Instant::now() < observation_deadline {
    for container in &updated_containers {
        let state = docker.inspect_container(&container.id).await?;
        match state.status {
            ContainerStatus::Exited | ContainerStatus::Dead => {
                // Container crashed — trigger rollback
                rollback(container, &previous_image_id).await?;
                return Err(DeployError::CrashDetected(container.name.clone()));
            }
            ContainerStatus::Running if state.health == Some(HealthStatus::Healthy) => {
                // Container is healthy — mark as verified
            }
            _ => {} // Still starting — continue observing
        }
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
}
```

---

## 3. Container States After Deploy

| Observed State | Action |
|---|---|
| `running` (no healthcheck defined) | Accepted as healthy after 10s observation without crash |
| `running` + `health: healthy` | Accepted immediately |
| `running` + `health: starting` | Wait up to 30s additional grace period for health to resolve |
| `running` + `health: unhealthy` | Log `level=warn`; do NOT rollback (unhealthy ≠ crashed); alert sent |
| `exited` or `dead` within 10s | Rollback triggered immediately |
| `restarting` (crash loop detected) | Rollback triggered after first detected restart |

---

## 4. Automatic Rollback

When a crash is detected within the observation window:

### 4a. Image ID Tracking

Before running `docker compose pull`, the LXC daemon records the currently running image IDs:

```rust
let previous_images: HashMap<String, String> = docker
    .list_containers(None)
    .await?
    .iter()
    .map(|c| (c.names[0].clone(), c.image_id.clone()))
    .collect();
```

### 4b. Rollback Execution

```rust
async fn rollback(container: &Container, previous_image_id: &str) -> Result<()> {
    emit_log(Level::Warn, format!("rolling back {} to {}", container.name, previous_image_id));
    
    // Stop the crashed container
    docker.stop_container(&container.id, Some(5)).await?;
    
    // Update docker-compose.yml in memory to pin to previous image ID
    // (Only for this rollback cycle — Git state is not modified)
    let rollback_compose = pin_image_in_compose(&container.compose_file, previous_image_id)?;
    
    // Start the rollback compose
    run_compose_up(&rollback_compose).await?;
    
    emit_log(Level::Info, format!("rollback complete for {}", container.name));
    Ok(())
}
```

**Important:** Rollback only affects the running state. The `docker-compose.yml` in Git is **not** modified. On the next sync cycle, the LXC daemon will attempt the new image again. If it crashes again, it rolls back again. This creates a natural protection loop.

**Logfmt events:**
```
ts=<ISO8601> level=warn component=lxc stack=<stack_name> app=<app> msg="crash detected; initiating rollback" image=<new_id>
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app> msg="rollback complete" image=<prev_id>
```

---

## 5. Watchtower Lifecycle Pre-Check Hooks

For apps with Watchtower lifecycle labels, post-deploy validation is extended with a pre-check script that runs **before** Watchtower updates the container (not after `docker compose up`):

```yaml
labels:
  com.centurylinklabs.watchtower.lifecycle.pre-check: /check-streams.sh
```

The script (`/check-streams.sh`) is injected into the container image or mounted as a volume. It returns exit code `75` to signal "skip this update cycle" (Watchtower's convention for deferring updates).

Example for Jellyfin:
```bash
#!/bin/bash
# Cancel update if active streams exist
active=$(curl -s "http://localhost:8096/Sessions?api_key=${JELLYFIN_API_KEY}" | \
         jq '[.[] | select(.NowPlayingItem != null)] | length')
[[ "$active" -gt 0 ]] && exit 75 || exit 0
```

The LXC daemon does not execute these scripts directly; they are executed by Watchtower during its own update cycle. The LXC daemon is responsible only for deploying the container with the correct labels.

---

## 6. Webhook Alerts for Unattended Failures

When a deployment failure or rollback occurs **outside** a CLIENT-triggered sync session (i.e., during the 30-minute fallback cron cycle), the LXC daemon sends an HTTP alert:

```rust
// POST to Ntfy or Discord webhook
let alert = Alert {
    title: format!("Deployment Failed — {}/{}", stack_name, app_name),
    body: format!("Container {} crashed after update. Rolled back to {}. Last logs:\n{}", 
                  app_name, previous_image_id, last_10_log_lines),
    priority: AlertPriority::High,
};
send_alert(&alert, &config.alert_webhook_url).await?;
```

**Alert configuration** is stored in the LXC daemon's runtime environment (injected by ephemeral secrets):
- `ALERT_WEBHOOK_URL` — Ntfy topic URL or Discord webhook URL.
- `ALERT_PROVIDER` — `ntfy` or `discord`.

If `ALERT_WEBHOOK_URL` is not set, alert sending is skipped silently (not a failure condition).

**Logfmt event:**
```
ts=<ISO8601> level=info component=lxc stack=<stack_name> app=<app> msg="webhook alert sent" provider=ntfy
```

---

## 7. Post-Deploy Results Surfaced to CLIENT

When a sync is CLIENT-triggered (via `POST /api/sync`), post-deploy results are streamed back via SSE in real time. After all containers are validated, the LXC daemon sends a final summary event:

```
ts=<ISO8601> level=info component=lxc stack=<stack_name> msg="post-deploy validation complete"
  apps_healthy=<N> apps_unhealthy=<M> apps_rolled_back=<K>
```

The CLIENT modal displays:
- Green indicator for each healthy app.
- Amber indicator for unhealthy (running but health check failing) apps.
- Red indicator for rolled-back apps, with the previous and attempted image IDs shown.

---

## 8. Related Use Cases

| Use Case File | Relationship |
|---|---|
| `pre-sync-hooks.md` | Runs before this phase; if `setup.sh` fails, post-deploy never runs |
| `transactional-actions.md` | Rollback strategy described here in detail |
| `error-handling-fail-closed.md` | Crash-loop and deploy failure handling |
| `update-active-stacks.md` | Triggers the sync cycle that ends with this post-deploy phase |
| `tui-deployment-modal-progress.md` | CLIENT displays post-deploy results streamed via SSE |
