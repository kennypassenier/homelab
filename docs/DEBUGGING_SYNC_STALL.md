# Debugging Sync Stall After HOST Readiness

## Problem Statement

Fresh deployments hang after "GitOps checker started" with no latch pull execution, despite:
- HOST bootstrap completing successfully
- LXC daemon online and WebSocket connected
- Manual latch pull in container works when run with explicit credentials

## Root Cause Hypothesis

CLIENT is not sending latch credentials to LXC because:
1. CLIENT environment lacks `LATCH_PAT`, `LATCH_KEY`, `LATCH_SECRETS_REPO` env vars
2. `~/.latch/config.toml` does not exist or lacks these values
3. CLIENT sends sync request with `latch: null` to LXC
4. LXC receives the sync but skips latch pull (no credentials provided)
5. Without `.env` files, docker compose operations fail or stall

## Comprehensive Logging Added

### CLIENT Logging (client-app/src/main.rs)

**Function: `request_lxc_sync_ws()`** - Shows what latch payload CLIENT is building

```
[CLIENT-DEBUG] No latch credentials loaded — sending null to LXC
   → Means: LATCH_PAT/KEY/REPO env vars missing AND ~/.latch/config.toml not found
   → ACTION: Set env vars or create ~/.latch/config.toml

[CLIENT-DEBUG] Latch credentials loaded — sending to LXC: {...}
   → Means: Credentials found from env vars or config file
   → Will include: pat, key, secrets_repo, project, env, sparse flag
```

**Function: `load_latch_pull_context()` in client-app/src/latch.rs** - Shows credential scan results

```
[LATCH-DEBUG] Credential scan: pat=[set/missing] key=[set/missing] repo=[missing/value] project=[missing/value] env=[missing/value]
   → Shows exactly which credentials were found and which are missing
   → If all missing: "All credentials missing, returning None"
```

### LXC Logging (lxc-daemon/src/api.rs)

**Function: WebSocket sync request handler** - Shows what credentials LXC received

```
[ws-sync] Sync triggered — latch: pat=[present/empty] key=[present/empty] repo=kennypassenier/secrets project=homelab env=dev sparse=true
   → Shows: LXC received sync request WITH credentials
   → Action taken: Will proceed to latch pull execution

[ws-sync] Sync triggered via WebSocket RPC — NO latch credentials in request!
   → Shows: LXC received sync request but no latch field
   → Action taken: Will skip latch pull and proceed to docker compose directly
   → PROBLEM: Without .env files, services likely won't start
```

### LXC Logging (lxc-daemon/src/gitops.rs)

**Function: `perform_sync()`** - Step-by-step sync pipeline with credential logging

```
[sync] ============ Sync cycle started ============
   → Entry marker: sync was triggered and starting

[sync] Step 3: about to process latch pull...
   → Reached latch phase

[sync] [latch] stack=cloudflared credentials present, invoking run_latch_pull...
   → Credentials found: about to run latch pull
   → Expected output: "Step 4: running docker compose pull per app"
   → If stalled here: latch pull is hanging or failing

[latch] stack=cloudflared command completed with exit=0
   → SUCCESS: latch pull completed
   → Next: docker compose pull and up will execute

[latch] stack=cloudflared command FAILED
   → FAILURE: latch command exited with error
   → See stderr lines below for specific error
   → Example errors:
      - "unexpected argument '--PAT'"  → Wrong latch binary version
      - "github token invalid"         → Invalid GITHUB_PAT
      - "secrets repo not found"       → Invalid REPO or KEY permissions
```

## Diagnostic Procedure

### Step 1: Deploy with Enhanced Logging

Rebuild and deploy lxc-daemon:

```bash
cd /home/kenny/Projects/homelab/lxc-daemon
cargo build --release
# Copy binary to HOST and provision container
```

### Step 2: Trigger Deployment

Create and provision a stack (e.g., cloudflared).

### Step 3: Capture Logs

**CLIENT side:** Look for `[CLIENT-DEBUG]` and `[LATCH-DEBUG]` lines
```bash
# Check CLIENT stdout/stderr during sync
journalctl -u client-app -f  # if systemd-managed
# OR check terminal where CLIENT runs
```

**LXC side:** Look for `[ws-sync]`, `[sync]`, and `[latch]` lines
```bash
# SSH to LXC container
pct enter <CONTAINER_ID>
# View daemon logs (assuming systemd or journalctl available)
journalctl -u lxc-daemon -f
# OR check if daemon writes to a log file
# OR use `docker logs lxc-daemon` if running in container
```

### Step 4: Analyze Results

**Scenario A: CLIENT has no credentials**
```
[LATCH-DEBUG] Credential scan: pat=[missing] key=[missing] repo=[missing] ...
[LATCH-DEBUG] All credentials missing, returning None
[CLIENT-DEBUG] No latch credentials loaded — sending null to LXC
[ws-sync] Sync triggered via WebSocket RPC — NO latch credentials in request!
```
**Fix:** Set env vars or create `~/.latch/config.toml` (see below)

**Scenario B: CLIENT has credentials, LXC receives them, latch succeeds**
```
[LATCH-DEBUG] Credential scan: pat=[set] key=[set] repo=kennypassenier/secrets ...
[CLIENT-DEBUG] Latch credentials loaded — sending to LXC: {...}
[ws-sync] Sync triggered — latch: pat=[present] key=[present] repo=kennypassenier/secrets ...
[sync] [latch] stack=cloudflared credentials present, invoking run_latch_pull...
[latch] stack=cloudflared command completed with exit=0
```
**Status:** ✅ Deployment should proceed to docker compose

**Scenario C: CLIENT has credentials, but latch pull fails**
```
[ws-sync] Sync triggered — latch: pat=[present] key=[present] ...
[sync] [latch] stack=cloudflared credentials present, invoking run_latch_pull...
[latch] stack=cloudflared command FAILED
ERROR: github token invalid or insufficient permissions
```
**Fix:** Verify LATCH_PAT token is valid and has access to LATCH_SECRETS_REPO

## Setting Up Latch Credentials for CLIENT

### Option 1: Environment Variables (Recommended for Testing)

```bash
export LATCH_PAT="github_pat_..."          # GitHub Personal Access Token
export LATCH_KEY="c49217b5..."            # Secrets repo encryption key
export LATCH_SECRETS_REPO="kennypassenier/secrets"
export LATCH_PROJECT="homelab"            # Optional: defaults to project folder name
export LATCH_ENV="dev"                    # Optional: environment to target
```

Then run CLIENT:
```bash
./CLIENT
```

### Option 2: Configuration File (Recommended for Persistent Setup)

Create `~/.latch/config.toml`:

```toml
[latch]
secrets_repo = "kennypassenier/secrets"
project = "homelab"
default_env = "dev"

# pat and key can be set here, but env vars take precedence
# pat = "github_pat_..."
# key = "c49217b5..."
```

Note: `pat` and `key` should preferably be set as env vars for security. If set in config, ensure file is not tracked in git and has `chmod 600`.

### Option 3: Hybrid (Env Vars Override Config)

```bash
# config.toml has repo, project, env
# env vars provide pat and key (more secure)
export LATCH_PAT="github_pat_..."
export LATCH_KEY="c49217b5..."
./CLIENT
```

## Expected Log Flow

When deployment works correctly:

```
CLIENT: [LATCH-DEBUG] Credential scan: pat=[set] key=[set] repo=... project=... env=...
CLIENT: [CLIENT-DEBUG] Latch credentials loaded — sending to LXC: {...}
LXC:    [ws-sync] Sync triggered — latch: pat=[present] key=[present] repo=... project=... env=... sparse=...
LXC:    [sync] ============ Sync cycle started ============
LXC:    [sync] Step 3: about to process latch pull...
LXC:    [sync] [latch] stack=cloudflared credentials present, invoking run_latch_pull...
LXC:    [latch] stack=cloudflared command completed with exit=0
LXC:    [sync] Step 4: running docker compose pull per app...
LXC:    [sync] Step 5: running docker compose up...
LXC:    ✓ Sync completed successfully
```

## Manual Verification (In LXC Container)

If you need to verify latch is working manually:

```bash
# SSH to LXC or run in container
cd /opt/gitops

# Export credentials (same as CLIENT would send)
export LATCH_PAT="github_pat_..."
export LATCH_KEY="c49217b5..."
export LATCH_PROJECT="homelab"
export LATCH_REPO="kennypassenier/secrets"
export LATCH_ENV="dev"

# Run latch pull
latch pull --env dev --sparse --PAT "$LATCH_PAT" --KEY "$LATCH_KEY" --REPO "$LATCH_REPO" --project "$LATCH_PROJECT"

# Check if .env files were created
ls -la stacks/cloudflared/*/

# Try docker compose up
cd stacks/cloudflared && docker compose up -d --remove-orphans
```

If this works manually but CLIENT sync stalls, the issue is in CLIENT credential loading (scenarios above apply).

## Next Steps

1. **Rebuild and deploy** with enhanced logging enabled
2. **Capture logs** during deployment showing all `[DEBUG]`, `[ws-sync]`, `[sync]`, and `[latch]` lines
3. **Share logs** showing the exact point where sync stalls
4. **Apply fix** based on diagnostic scenario (A, B, or C above)
5. **Retry deployment** to verify sync completes

---

**Related Files:**
- CLIENT sync dispatch: [client-app/src/main.rs](../../client-app/src/main.rs#L1264-L1280)
- CLIENT latch loader: [client-app/src/latch.rs](../../client-app/src/latch.rs#L1555-L1590)
- LXC sync handler: [lxc-daemon/src/api.rs](../../lxc-daemon/src/api.rs#L348-L370)
- LXC sync pipeline: [lxc-daemon/src/gitops.rs](../../lxc-daemon/src/gitops.rs#L50-L150)
