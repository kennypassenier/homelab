---
description: "Conventions for docker-compose.yml files in this repo (presets and stacks). Follow these when creating or editing any compose file."
applyTo: "**/docker-compose.yml"
---

The complete, authoritative conversion rules live in
`docs/LLM_COMPOSE_CONVERSION.md` — read that file. Summary:

- Persistent state: host binds under `/appdata/__STACK__/<app>-config`
  (presets) or `/appdata/<stack>/<app>-config` (stacks). Named volumes only
  for regenerable caches.
- Labels on every stateful service: `com.homelab.backup.pause=true` and
  `com.homelab.update.policy=manual` (or `auto`). Watchtower does NOT exist
  in this system — never add its labels or containers.
- Network: exactly one external network `__STACK___net` / `<stack>_net`.
- Secrets: `env_file: .env`, the file lives in the STACK dir (gitignored)
  and travels to the host vault at deploy — never in presets, never in git.
- `restart: unless-stopped`; no `version:` key; no `privileged`/
  `network_mode: host` without a human decision.
- Preset placeholders: `__STACK__`, `__VMID__`, `__HOSTNAME__`, `__IP__`.
