# Instruction: convert a vendor docker-compose.yml to a homelab preset

> **How to use this document (for Kenny):** paste this entire file into any
> LLM, followed by the vendor's `docker-compose.yml` (the one from the app's
> README or docs), and say which app it is. The LLM's output is a ready-to-use
> preset directory. Review the diff it explains, then drop the files into
> `presets/` — done.

---

**You are converting a vendor-provided docker-compose.yml into a "preset" for
a private homelab orchestration system.** Follow every rule below exactly.
Where the vendor file conflicts with a rule, the rule wins — but you must
list every such change you made at the end, with one line of reasoning each,
so the human can verify nothing important was lost.

## Output format

Produce exactly two files (plus optional extra config files if the vendor
setup needs them):

**File 1 — `presets/<preset-name>/preset.yml`:**
```yaml
description: "<max ~6 words, what the app is>"
ram_mb: <sensible default for this app: 512 for small tools, 1024-2048 for
         heavier ones, 4096+ only for media/database-heavy apps>
# Only add these when the app genuinely needs more than the defaults
# (2 cores / 32 GB disk):
# cores: 4
# disk_gb: 64
```

**File 2 — `presets/<preset-name>/<app-name>/docker-compose.yml`:**
the converted compose, per the rules below.

`<preset-name>` and `<app-name>` are lowercase, digits and hyphens only.
Usually they are the same word (e.g. `mealie/mealie/`). If the vendor compose
has a database or cache sidecar (postgres, redis, mariadb), keep those
services **in the same compose file** as the main app — do not split them
into separate app directories.

## Placeholders (substituted at scaffold time — use them literally)

| Token | Meaning |
|---|---|
| `__STACK__` | the stack name the user will choose |
| `__VMID__` | the container's Proxmox vmid |
| `__HOSTNAME__` | `<vmid>-app-<stackname>` |
| `__IP__` | the container's IP address (no CIDR suffix) |

Only these four exact tokens are substituted; `${ENV_VAR}` syntax and any
other `__underscored__` strings pass through untouched.

## Conversion rules

1. **Persistent data goes under `/appdata/__STACK__/<app>-config`.**
   Replace every named volume and every relative bind (`./data`, `./config`)
   that holds STATE (settings, databases, uploads) with a host bind:
   ```yaml
   volumes:
     - /appdata/__STACK__/<app>-config:/<whatever the container expects>
   ```
   Multiple state dirs are fine: `/appdata/__STACK__/<app>-config`,
   `/appdata/__STACK__/<app>-data`, … — every path MUST start with
   `/appdata/__STACK__/`. The orchestrator scans the compose for `/appdata/`
   binds and auto-creates + mounts + backs up those host directories; a named
   docker volume would silently be excluded from backups and lost on
   container recreation. Delete the vendor's top-level `volumes:`
   declarations for the volumes you replaced.
   - CACHE-only volumes (transcode caches, tmp dirs) may stay as named
     volumes or `tmpfs` — they are deliberately not backed up. Say which
     ones you classified as cache and why.

2. **Every service gets these labels** (add to the vendor's labels, don't
   remove theirs unless rule 6 says so):
   ```yaml
   labels:
     - com.homelab.backup.pause=true
     - com.homelab.update.policy=manual
   ```
   Exception: omit `backup.pause` for stateless helpers (a flaresolverr-style
   sidecar with no volumes) — pausing them during backup is pointless.

3. **Network.** Remove the vendor's `networks:` definitions entirely and use
   exactly this on every service, with this top-level block:
   ```yaml
   services:
     <app>:
       networks:
         - __STACK___net

   networks:
     __STACK___net:
       external: true
       name: __STACK___net
   ```
   (Note: `__STACK__` + `_net` — three underscores total in the middle.)

4. **Ports stay host-published.** Keep the vendor's `ports:` mappings as-is
   (the LXC has its own IP, so there are no host port conflicts). Do NOT
   convert ports to expose-only; the reverse proxy (traefik) lives on a
   different container and reaches services by IP:port.

5. **Restart policy:** every long-running service gets
   `restart: unless-stopped`. Remove `restart: always`.

6. **Strip these if present:** `version:` key (obsolete), watchtower labels
   (`com.centurylinklabs.watchtower.*` — updates are managed by the
   orchestrator), `container_name` collisions (set `container_name` to the
   service name), healthcheck sections may stay, `depends_on` may stay,
   `privileged: true` and `network_mode: host` must be REMOVED and flagged
   loudly in your change list (they need a human decision).

7. **Secrets never go in the compose.** Move every credential, API key,
   token, VPN key, and password out of `environment:` into env-file form:
   ```yaml
   env_file:
     - .env
   ```
   and produce a **third output block** (not a file in the preset!) listing
   the `.env` template the human must fill in and place in the *stack*
   directory after scaffolding (`stacks/<name>/<app>/.env`):
   ```
   # stacks/<name>/<app>/.env — fill in and never commit
   ADMIN_TOKEN=
   DB_PASSWORD=
   ```
   Non-secret environment settings (TZ, PUID, feature flags) stay as plain
   `environment:` entries in the compose.

8. **Uid/gid and timezone.** If the image supports them, set:
   ```yaml
   environment:
     - PUID=1000
     - PGID=1000
     - TZ=Europe/Brussels
   ```

9. **Hardware/device access** (`/dev/dri` for VAAPI, `/dev/net/tun` for
   VPNs): keep the `devices:` entry in the compose, but flag it in your
   change list — the device must ALSO be passed into the LXC on the host
   side, which is a separate manual step this compose cannot do.

10. **Media/bulk-data paths.** If the app reads a media library or download
    directory, mount it from `/data/18TB/...` or `/data/12TB/...` (these
    exist inside media-related containers) and keep the vendor's default
    IN-CONTAINER path if databases will store absolute paths. Flag it: bulk
    data mounts also need a host-side LXC bind mount decision.

## Style

- Keep the compose minimal and readable; preserve vendor comments that
  explain non-obvious settings, drop marketing comments.
- Do not invent services, volumes, or env vars the vendor didn't have and
  the rules didn't require.
- Output the files in fenced code blocks with their full paths as headings.

## End with a change list

A short bullet list: every deviation from the vendor file (volume X →
appdata bind, secret Y → .env, stripped Z), plus any ⚠ flags from rules 6,
9, 10 that need a human/host-side decision.

---

## Example (input → output)

**Vendor input:**
```yaml
version: "3"
services:
  vaultwarden:
    image: vaultwarden/server:latest
    restart: always
    environment:
      - ADMIN_TOKEN=changeme123
      - SIGNUPS_ALLOWED=false
    volumes:
      - vw-data:/data
    ports:
      - 80:80
volumes:
  vw-data:
```

**Correct output:**

`presets/vaultwarden/preset.yml`
```yaml
description: "Password manager (Bitwarden server)"
ram_mb: 512
```

`presets/vaultwarden/vaultwarden/docker-compose.yml`
```yaml
services:
  vaultwarden:
    image: vaultwarden/server:latest
    container_name: vaultwarden
    restart: unless-stopped
    env_file:
      - .env
    environment:
      - SIGNUPS_ALLOWED=false
      - TZ=Europe/Brussels
    volumes:
      - /appdata/__STACK__/vaultwarden-config:/data
    ports:
      - "80:80"
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

`.env` template (goes in `stacks/<name>/vaultwarden/.env` after scaffolding,
never in the preset):
```
ADMIN_TOKEN=
```

Change list:
- named volume `vw-data` → `/appdata/__STACK__/vaultwarden-config` (state must
  live on the host for backup/recreate)
- `ADMIN_TOKEN` moved to `.env` (secret; goes to the host vault, not git)
- `restart: always` → `unless-stopped`; dropped obsolete `version:` key
- added house labels, TZ, `__STACK___net` network block
