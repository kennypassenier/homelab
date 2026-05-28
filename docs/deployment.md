# Deployment Guide

Last updated: 2026-05-28

This document describes the current deployment order, required credentials, environment files, and verification steps for bringing up the homelab stack safely.

## 1. Deployment Model

The system has three runtime tiers:

- CLIENT: interactive Linux desktop TUI and GitOps authoring surface
- HOST: Proxmox-side daemon and operational scripts
- LXC: per-stack daemon/runtime inside each container

Deployment is GitOps-first:

- change Git-managed config in this repository
- push to the repo
- let HOST/LXC services consume those changes
- do not patch running LXCs manually unless you are in a break-glass recovery scenario

## 2. Required External Services

You need these services/accounts before deployment:

- GitHub repository hosting this repo
- GitHub Container Registry if you will publish/pull the LXC daemon image
- Proxmox VE host with unprivileged LXCs enabled
- Linux client workstation for CLIENT
- Docker / Docker Compose inside each LXC
- Secret source for per-app `.env` material

Current stack hooks in this repo already assume Infisical CLI exports in several `pre-sync.sh` files. If you keep that model, Infisical access must exist before first sync.

## 3. GitHub Setup

### Releases and Workflows

Current workflows:

- `.github/workflows/host-release.yml`
- `.github/workflows/lxc-image.yml`

Current behavior:

- HOST release assets are built from tag pushes matching `host-daemon-v*`
- LXC image publishing runs on main-branch changes under `lxc-daemon/` or on manual dispatch
- GitHub Actions can publish both without a custom PAT because the workflows use `GITHUB_TOKEN`

### Optional Tokens

You only need extra GitHub tokens for private-repo/private-package scenarios.

1. `HOST_UPDATE_TOKEN`
Purpose: lets HOST self-update against private GitHub Releases.
Permissions:
- `Contents: Read`
- `Metadata: Read`

2. `GITOPS_REPO_TOKEN`
Purpose: lets the LXC daemon clone a private GitOps repo over HTTPS.
Permissions:
- `Contents: Read`
- `Metadata: Read`

3. GHCR pull token
Purpose: only needed if the published LXC image is private.
Permissions:
- `Packages: Read`
- `Metadata: Read`

You do not need a PAT for the current workflows to create releases or push GHCR images in GitHub Actions.

## 4. Environment Files

Create these files from the examples in the repo:

- `client-app/.env.example`
- `host-daemon/.env.example`
- `lxc-daemon/.env.example`

Use them as service `EnvironmentFile=` inputs or shell exports for local testing.

### CLIENT variables

CLIENT currently cares about:

- `LXC_API_TOKEN`
- `LXC_API_IP`
- `OPNSENSE_BASE_URL` (for example `https://10.10.5.1`)
- `OPNSENSE_API_KEY`
- `OPNSENSE_API_SECRET`
- optional `OPNSENSE_TLS_INSECURE=true` for lab-only self-signed HTTPS
- optional per-stack overrides like `LXC_MEDIA_IP`

### HOST variables

HOST currently cares about:

- `HOST_UPDATE_REPO`
- `HOST_UPDATE_ASSET`
- optional `HOST_UPDATE_TOKEN`
- optional `LXC_<STACK>_IP` values used during backup orchestration

### LXC variables

LXC currently cares about:

- `STACK_NAME`
- `STACK_IP`
- `GITOPS_REPO_URL`
- optional `GITOPS_REPO_TOKEN`

## 5. Stack Secret Files

Per-app runtime secrets should not be committed.

Current expected model:

- stack `pre-sync.sh` scripts export app `.env` files into `/appdata/<stack>/<app>/.env`
- those files are created on the LXC side before compose up
- secret values come from your external secret source, not Git

Before first deployment, verify that every stack hook can populate the required `.env` targets it references.

## 6. Bring-Up Order

Use this order.

1. Prepare GitHub.
- Confirm workflows exist.
- Confirm release tags and GHCR namespace conventions.
- If the repo or packages are private, create the optional read tokens above.

2. Prepare the client workstation.
- Install Rust toolchain if building locally.
- Ensure Git SSH access works.
- Create CLIENT env file from `client-app/.env.example`.

3. Prepare the Proxmox host.
- Install/start the HOST daemon.
- Create HOST env file from `host-daemon/.env.example`.
- Ensure the systemd service name is `host-daemon.service` if you want self-update restarts to work unchanged.
- Ensure `/opt/appdata` and backup storage roots exist.

4. Prepare each LXC.
- Install Docker / Docker Compose / git.
- Install/start the LXC daemon.
- Create env file from `lxc-daemon/.env.example`.
- Set `STACK_NAME` per container.
- Set `GITOPS_REPO_URL` to this repo.
- If private, also set `GITOPS_REPO_TOKEN`.

5. Prepare secrets.
- Verify every stack `pre-sync.sh` can authenticate to the secret backend.
- Verify destination directories under `/appdata/<stack>/<app>/` exist or are created by your stack workflow.

6. Publish artifacts.
- Create a `host-daemon-vX.Y.Z` tag to publish a HOST release asset.
- Push `lxc-daemon/` changes or run the LXC image workflow manually.

7. Activate stacks.
- Use CLIENT to create/edit stack config.
- If you want DHCP reservation automation, set `network.reserved_ipv4` in the stack config editor and export the OPNsense variables above on CLIENT.
- Keep `deploy.enabled=false` until a stack is ready.
- Activate stacks explicitly from CLIENT when ready.

8. Trigger first sync.
- From CLIENT, deploy selected stack(s).
- Confirm LXC sparse checkout initializes correctly.
- Confirm setup hook, compose pull, and compose up complete.

## 7. Verification Checklist

After deployment, verify:

- CLIENT `cargo check` passes locally
- CLIENT can open the app config editor and save image changes through GitOps
- CLIENT can stream live LXC deploy logs during a sync
- HOST and LXC services are running
- HOST self-update can read releases
- LXC sparse checkout contains only `stacks/<stack_name>/`
- stack `lxc-compose.yml` files contain expected `deploy`, `network`, and `resources` blocks
- stack `lxc-compose.yml` files contain expected `deploy`, `network`, `boot`, and `resources` blocks
- OPNsense reservations are present for any stack using `dhcp-reserved` plus `network.reserved_ipv4`
- per-app `.env` files exist under `/appdata/...`
- Docker containers start successfully inside each LXC
- backup orchestration can reach `http://<lxc_ip>:8080`

## 8. Known Remaining Gaps

These features are still tracked as pending use-cases before full end-to-end feature completion:

- host storage operations
- host hardware operations
- backup policy enforcement service
- restore execution backend
