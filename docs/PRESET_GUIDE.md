# Preset guide — add or change an app without touching code

Presets are **data**: a directory under `presets/` in this repo. The wizard
reads them at startup, so adding a new app to the catalog is: create a
directory, write two small files, done. No recompile, no code review, no
restart of anything on the host.

## The 30-second version: add a new app

Say you want Stirling-PDF in the catalog:

```bash
mkdir -p presets/stirling/stirling
```

`presets/stirling/preset.yml`:
```yaml
description: "PDF toolbox"
ram_mb: 1024
```

`presets/stirling/stirling/docker-compose.yml`:
```yaml
services:
  stirling:
    image: frooodle/s-pdf:latest
    container_name: stirling
    restart: unless-stopped
    volumes:
      - /appdata/__STACK__/stirling-config:/configs
    ports:
      - "8080:8080"
    labels:
      - com.homelab.backup.pause=true
      - com.homelab.update.policy=manual
    networks:
      - __STACK___net

networks:
  __STACK___net:
    external: true
    name: __STACK___net
```

Open the TUI, press `N` — "stirling" is in the list. That's the whole flow.

## Layout

```
presets/
  _core/                 # apps injected into EVERY new stack
    promtail/            #   (log shipping to Loki — D8)
      docker-compose.yml
      promtail-config.yml
  syncthing/             # one preset
    preset.yml           #   metadata (description, resources, overrides)
    syncthing/           #   one directory PER APP, holding its files
      docker-compose.yml
  jellyfin/
    preset.yml
    jellyfin/docker-compose.yml
  custom/
    preset.yml           # no app dirs = empty stack
```

Rules:
- A preset is any directory with a `preset.yml`. Directories starting with
  `_` are reserved (`_core` = the always-injected apps).
- Every **subdirectory** of a preset is an app; **every file** in it is
  copied into the new stack with placeholder substitution. Extra config
  files (like promtail's) ride along automatically.
- A preset may have **multiple app dirs** — a "media" preset with jellyfin +
  sonarr + radarr is just three subdirectories.
- The manifest's `apps:` list is derived from the app directory names —
  that's what the host starts as `/opt/<stack>/<app>`.
- "custom" always sorts last in the wizard; everything else alphabetically.

## preset.yml fields

```yaml
description: "Shown in the wizard"   # required (well: strongly advised)
ram_mb: 1024        # wizard default; user can still change it
cores: 4            # optional — omit to use StackDefaults (2)
disk_gb: 64         # optional — omit to use StackDefaults (32)
features: "nesting=1,keyctl=1,fuse=1"  # optional LXC features override
unprivileged: false # optional — ONLY for presets that truly need privileged
```

Everything the preset does not set comes from `StackDefaults` (client
config): network conventions, swap formula, boot order, protection flag,
which core apps get injected.

## Placeholders

These literal tokens are replaced in **every** template file at scaffold
time:

| Token | Becomes | Example |
|---|---|---|
| `__STACK__` | the stack name the user typed | `vault-sync` |
| `__VMID__` | the chosen vmid | `109` |
| `__HOSTNAME__` | `<vmid>-app-<stack>` | `109-app-vault-sync` |
| `__IP__` | the derived IP (no CIDR) | `10.10.10.9` |

Anything else with underscores (promtail's `__path__`, compose `${ENV_VAR}`)
is left untouched — only these four exact tokens are substituted.

## House conventions (keep these in every compose)

- **Config under `/appdata/__STACK__/<app>-config`** — that's the host bind
  mount that survives container recreation and is what restic backs up.
  A named docker volume would silently miss the backups.
  **You only write it here** — the scaffolder scans the compose files for
  `/appdata/` binds and generates the manifest `storage:` entries from them
  (host dir creation, ownership, LXC mount). One source of truth; nothing
  to keep in sync.
- **Labels**:
  - `com.homelab.backup.pause=true` — stop this container during the
    snapshot (only needed for apps whose data must be quiesced).
  - `com.homelab.update.policy=manual` — the nightly run skips it; set
    `auto` for apps you trust to update unattended.
- **Network `__STACK___net`**, external — the deploy pipeline creates it.
- Frozen in-container paths for media apps (`/data/18TB`, `/data/12TB`,
  `/config`, `/downloads`) — the *arr databases store absolute paths.

## Secrets

Never put credentials in a preset. Put a `.env` file next to the compose in
the **stack** directory after scaffolding (`stacks/<name>/<app>/.env`) — the
deploy sends it over the TLS line into the host vault
(`/var/lib/homelab/secrets/`), outside git. Presets are committed to git;
stacks' `.env` files are gitignored.

## Your own Rust services (G9)

Your Rust repos and the homelab meet at one interface: a Docker image on
GHCR. The bridge lives in `templates/rust-service/`:

1. Copy `Dockerfile` (swap in your binary name) and `release-image.yml`
   into your Rust repo. Every `vX.Y.Z` tag then publishes
   `ghcr.io/<user>/<repo>:<version>` and `:latest` next to the release.
   The package is linked to the repo and takes its visibility, so a public
   repo yields one the host can pull anonymously — nothing to flip. (This
   step used to say the opposite; corrected 2026-08-28 after Kenny checked.)
2. Copy `presets/rust-service/` to `presets/<yourname>/`, point the
   `myservice` compose at your image, keep or drop the bundled RabbitMQ.
   `RABBIT_USER`/`RABBIT_PASS` go in BOTH apps' `.env` via the secrets
   vault (RabbitMQ's built-in guest user is localhost-only).
3. Wizard → deploy. From then on: tag a release in the app repo → CI
   builds the image → the nightly run updates it with automatic rollback
   (`com.homelab.update.policy=auto`), or `homelab update stacks/<name>`
   immediately.

The example preset deploys as-is only after you edit the image line — it
points at a placeholder on purpose.

## Changing an existing preset

Edit the files; the next wizard run uses them. Already-scaffolded stacks are
**copies** — they do not change retroactively (that's a feature: a preset
edit can't silently reconfigure a running service). To apply an improved
preset to an existing stack, edit the stack's own files (or re-scaffold
under a new name) and `homelab deploy`.

## Core apps (`_core/`)

Every directory under `presets/_core/` is copied into **every** new stack.
Today that is promtail. To exempt one stack: delete the app dir from the
scaffolded stack + remove it from the manifest's `apps:` list. If a preset
ships its own version of a core app (same dir name), the preset's version
wins.

## Fallback behaviour

No `presets/` directory (bare checkout, tests)? The wizard falls back to a
built-in synthetic catalog (same names) that generates a generic compose.
Disk presets always take precedence — if you see a generic compose where you
expected your template, the client's working directory is wrong (run
`homelab` from the repo root).

## Converting a vendor docker-compose.yml

Apps usually ship a ready-made `docker-compose.yml` in their docs. Don't
convert it by hand: paste **docs/LLM_COMPOSE_CONVERSION.md** into any LLM,
followed by the vendor file — it produces a rule-conformant preset plus a
change list to review. That document is self-contained on purpose.

## Checklist for a new preset

1. `preset.yml` with description + `ram_mb`.
2. One dir per app with `docker-compose.yml` (+ any config files).
3. Config volume under `/appdata/__STACK__/…`, both labels, the
   `__STACK___net` network block.
4. `homelab tui` → `N` → pick it → deploy to a test vmid (108 while it's
   the test container) → verify → destroy.
5. Commit the preset directory.
