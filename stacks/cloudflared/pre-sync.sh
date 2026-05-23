

#!/usr/bin/env bash
export $(cat /proc/1/environ | tr '\0' '\n' | grep '^INFISICAL_' | xargs)
set -euo pipefail

# Generate .env for cloudflared
infisical export --env=prod --path=cloudflared/cloudflared/.env > /appdata/cloudflared/cloudflared/.env
# Generate .env for promtail from shared/promtail
infisical export --env=prod --path=shared/promtail/.env > /appdata/cloudflared/promtail/.env
