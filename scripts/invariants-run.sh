#!/usr/bin/env bash
# invariants-run.sh — build the admin dashboard's demo-host build, run it
# against a throwaway config, drive it with the Playwright smoke in
# admin/web/test-e2e/invariants.e2e.js, and tear everything down again.
#
# Nothing here touches a real host, a real stack or any of Kenny's
# machines: HOMELAB_ADMIN_DEMO_HOST=1 (feat-platform-10, "Only in test
# builds") makes homelab-admin answer every read with made-up data and
# every action with three fake steps, entirely inside this one process.
#
# Called as `make invariants` directly, or from `.githooks/gate-carry.sh
# invariants` as part of `make gate`'s full run — never at commit time
# (docs/INVARIANTS.md: this suite needs a built binary and a browser,
# which a commit cannot afford).
set -euo pipefail
root="$(git rev-parse --show-toplevel)"
cd "$root"

workdir="$(mktemp -d)"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ] && kill -0 "$server_pid" 2>/dev/null; then
    kill "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  rm -rf "$workdir"
}
trap cleanup EXIT

# A free-ish high port, so two runs on one machine (a dev shell and a CI
# box, say) do not collide on 8090 (the service's own default).
port=18099
listen="127.0.0.1:$port"
base_url="http://$listen"
token="invariants-$(date +%s)-$$"
secret_key="$(head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n')"

cat > "$workdir/admin.toml" <<EOF
[admin]
host = "10.10.10.250:8443"
host_token = "0000000000000000"
access_team_domain = "example.cloudflareaccess.com"
access_aud = "e76eb5aa00000000000000000000000000000000000000000000000000000000"
dev_without_locks = true
EOF
mkdir -p "$workdir/state"

echo "invariants: building homelab-admin --features demo-host"
cargo build -p homelab-admin --features demo-host --quiet

echo "invariants: starting the demo host on $listen"
HOMELAB_ADMIN_TOKEN="$token" \
HOMELAB_ADMIN_SECRET_KEY="$secret_key" \
HOMELAB_ADMIN_PUBLIC_URL="https://localhost:$port" \
HOMELAB_ADMIN_DEMO_HOST=1 \
HOMELAB_ADMIN_DEMO_STACKS="films,notes,oldstack" \
  "$root"/target/debug/homelab-admin \
    --config "$workdir/admin.toml" \
    --state-dir "$workdir/state" \
    --listen "$listen" \
    > "$workdir/server.log" 2>&1 &
server_pid=$!

# No /healthz answer in 20 s means the binary did not come up; fail loudly
# with its own log rather than letting Playwright time out opaquely.
up=0
for _ in $(seq 1 40); do
  if curl -fsS -o /dev/null --max-time 1 "$base_url/healthz" 2>/dev/null; then
    up=1
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    break
  fi
  sleep 0.5
done
if [ "$up" != 1 ]; then
  echo "invariants: the demo host never answered /healthz — its log:" >&2
  cat "$workdir/server.log" >&2
  exit 1
fi

echo "invariants: running the Playwright smoke"
cd admin/web
[ -d node_modules ] || npm ci --no-audit --no-fund
INVARIANTS_BASE_URL="$base_url" INVARIANTS_TOKEN="$token" \
  node --test test-e2e/invariants.e2e.js
