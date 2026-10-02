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

# fix-207: the topology/firewall/dependencies pages read the working copy
# (homelab_admin::shell::workcopy::WorkingCopy), not the demo host's made-up
# fleet — so a smoke that pins those pages needs a real (if tiny) working
# copy to clone. A local path is a supported remote (arch-edit-txn, "a
# local path works too (tests)"), so this builds a one-commit fixture repo
# from three real stack manifests — one firewall on, one off, one with no
# firewall block at all — and points HOMELAB_ADMIN_GIT_REMOTE at it. Only
# `lxc-compose.yml` is copied (never a stack's `.env`), so no secret ever
# enters this throwaway repo.
fixture_repo="$workdir/fixture-repo"
mkdir -p "$fixture_repo/stacks/admin" "$fixture_repo/stacks/kp-soft" "$fixture_repo/stacks/gateway"
cp "$root/stacks/admin/lxc-compose.yml" "$fixture_repo/stacks/admin/lxc-compose.yml"
cp "$root/stacks/kp-soft/lxc-compose.yml" "$fixture_repo/stacks/kp-soft/lxc-compose.yml"
cp "$root/stacks/gateway/lxc-compose.yml" "$fixture_repo/stacks/gateway/lxc-compose.yml"
git init -q -b main "$fixture_repo"
git -C "$fixture_repo" -c user.email=invariants@example.com -c user.name=invariants \
  add -A
git -C "$fixture_repo" -c user.email=invariants@example.com -c user.name=invariants \
  commit -q -m "fixture: three stacks for the invariants smoke"

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
# The binary lands in cargo's target directory, which a global
# ~/.cargo/config.toml (or CARGO_TARGET_DIR) may move out of the repository.
target_dir=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')

echo "invariants: starting the demo host on $listen"
HOMELAB_ADMIN_TOKEN="$token" \
HOMELAB_ADMIN_SECRET_KEY="$secret_key" \
HOMELAB_ADMIN_PUBLIC_URL="https://localhost:$port" \
HOMELAB_ADMIN_DEMO_HOST=1 \
HOMELAB_ADMIN_DEMO_STACKS="admin,kp-soft,gateway" \
HOMELAB_ADMIN_DATA_DIR="$workdir/admin-data" \
HOMELAB_ADMIN_GIT_REMOTE="$fixture_repo" \
HOMELAB_ADMIN_GIT_BRANCH="main" \
  "$target_dir"/debug/homelab-admin \
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
[ -d node_modules/playwright ] || PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1 npm ci --no-audit --no-fund
INVARIANTS_BASE_URL="$base_url" INVARIANTS_TOKEN="$token" \
  node --test test-e2e/invariants.e2e.js
