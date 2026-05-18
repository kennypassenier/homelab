# Stack: Paperless

> Document management stack. Paperless-NGX with PostgreSQL, Redis, and an AI assistant for automatic document tagging.

## Overview

Paperless-NGX is a document management system that scans, OCRs, and indexes documents. The stack consists of multiple interdependent services that must all be on the same Docker network.

## Apps

| App | Purpose | Port |
|---|---|---|
| Webserver (paperless-ngx) | Document management UI + OCR | 8000 |
| DB (PostgreSQL 16) | Primary database | — |
| Broker (Redis 7) | Task queue | — |
| AI Assistant (paperless-ai) | Automatic document tagging via LLM | — |
| [Watchtower](app-watchtower.md) | Automatic image updates | — |
| [Promtail](app-promtail.md) | Log shipping to Loki | — |

## `pre-sync.sh`

Creates `paperless_network` idempotently before any compose project starts.

## Network

All apps join `paperless_network` (external Docker bridge):

```yaml
networks:
  paperless_network:
    name: paperless_network
    external: true
```

## Services

### webserver (paperless-ngx)

- Image: `ghcr.io/paperless-ngx/paperless-ngx:latest`
- Port: `8000`
- Depends on: `db`, `broker`
- Volumes: `data`, `media`, `export`, `consume` all under `/appdata/paperless/`

### db (PostgreSQL 16)

- Image: `postgres:16`
- No exposed ports
- Credentials from SOPS-encrypted `.env`
- Volume: `/appdata/paperless/db`

### broker (Redis 7)

- Image: `redis:7`
- No exposed ports
- Volume: `/appdata/paperless/redis`

### ai-assistant (paperless-ai)

- Image: `clusterzx/paperless-ai:latest`
- Connects to the paperless-ngx API
- API key and LLM configuration via SOPS-encrypted `.env`
- Volume: `/appdata/paperless/ai-assistant`

## Storage

| Host path | Purpose |
|---|---|
| `/appdata/paperless/data` | Paperless internal data |
| `/appdata/paperless/media` | Archived documents |
| `/appdata/paperless/export` | Export output |
| `/appdata/paperless/consume` | Inbox — drop PDFs here |
| `/appdata/paperless/db` | PostgreSQL data |
| `/appdata/paperless/redis` | Redis data |
| `/appdata/paperless/ai-assistant` | AI assistant data |

## See also

- [app-watchtower.md](app-watchtower.md)
- [app-promtail.md](app-promtail.md)
- [secret-management.md](secret-management.md)
