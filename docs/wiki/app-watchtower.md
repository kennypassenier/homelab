# App: Watchtower

> Automatic Docker image updater. Checks for new image versions on a schedule and recreates containers that opt in via label.

## Overview

Each stack runs its own Watchtower instance. Watchtower polls for new images, pulls them, and recreates containers. Only containers with the `com.centurylinklabs.watchtower.enable=true` label are updated — all others are ignored.

## Container

| Field | Value |
|---|---|
| Image | `containrrr/watchtower:latest` |
| Schedule | Poll interval defined per stack (e.g. every 24h) |
| Flags | `--cleanup --label-enable` |

`--cleanup` removes old images after a successful update, keeping the host clean.  
`--label-enable` restricts updates to opt-in containers only.

## DOCKER_API_VERSION

All Watchtower instances set:
```yaml
environment:
  - DOCKER_API_VERSION=1.41
```
This prevents version negotiation warnings and ensures compatibility with the Docker socket on the host.

## Lifecycle Hooks

Watchtower supports pre/post-check and pre/post-update hooks via container labels. Example from Jellyfin:

```yaml
labels:
  com.centurylinklabs.watchtower.lifecycle.pre-check: "/check-streams.sh"
```

If the pre-check script exits non-zero, Watchtower skips the update for that container in this cycle. See [app-jellyfin.md](app-jellyfin.md) for the streams check implementation.

## Self-Update

Watchtower updates itself — it monitors its own image and recreates itself when a new version is available.

## Volume

Watchtower mounts the Docker socket:
```yaml
volumes:
  - /var/run/docker.sock:/var/run/docker.sock
```

## Opt-in Label

Add this label to any container that should be auto-updated:
```yaml
labels:
  com.centurylinklabs.watchtower.enable: "true"
```

## See also

- [app-jellyfin.md](app-jellyfin.md)
- [gitops-flow.md](gitops-flow.md)
