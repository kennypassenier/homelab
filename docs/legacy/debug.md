# Debug Runbook

Last updated: 2026-06-12

This runbook is copy/paste-first. Run commands from the right machine context.

## 1) CLIENT Debug (local workstation)

### 1.1 Build artifact compatibility check

```bash
cd ~/Projects/homelab
make build-lxc
readelf -V ./apps/LXC 2>/dev/null | grep GLIBC | sort -Vu
```

Expected: highest GLIBC symbol should be compatible with Debian 12 containers (for example `GLIBC_2.34`).

### 1.2 Confirm stack deploy state before sending any provision request

```bash
cd ~/Projects/homelab
awk '/deploy:/,/^[^ ]/' stacks/cloudflared/lxc-compose.yml
```

If you want it hard-disabled:

```bash
cd ~/Projects/homelab
sed -n '1,120p' stacks/cloudflared/lxc-compose.yml
```

Confirm this line exists under `deploy`:

```text
enabled: false
```

### 1.3 Stream HOST logs from CLIENT machine

```bash
curl -fsSL http://10.10.5.250:8080/api/health
```

If auth is enabled:

```bash
TOKEN="$(grep '^LXC_API_TOKEN=' ~/Projects/homelab/config/.env | cut -d '=' -f2-)"
curl -fsSL -H "Authorization: Bearer ${TOKEN}" http://10.10.5.250:8080/api/metrics | jq .
```

## 2) HOST Debug (Proxmox)

### 2.1 Verify HOST service health and recent logs

```bash
systemctl status host-daemon --no-pager -l
journalctl -u host-daemon -n 120 --no-pager
journalctl -u host-daemon -f
```

### 2.2 Ensure HOST does not auto-provision by default

```bash
grep -E '^(HOST_AUTO_PROVISION_ENABLED|HOST_ENV_FILE|GITOPS_REPO)=' /root/homelab/config/.env || true
```

Recommended values:

```text
HOST_AUTO_PROVISION_ENABLED=0
HOST_ENV_FILE=/root/homelab/config/.env
GITOPS_REPO=/root/homelab
```

Then restart HOST:

```bash
systemctl restart host-daemon
systemctl status host-daemon --no-pager -l
```

### 2.3 Check that latch credentials are present on HOST

```bash
grep -E '^(LATCH_PAT|LATCH_KEY|LATCH_SECRETS_REPO)=' /root/homelab/config/.env
```

If missing, add them in `/root/homelab/config/.env` and restart HOST:

```bash
systemctl restart host-daemon
```

### 2.4 Prune obsolete local files safely (without reinstalling everything)

Preview what would be removed first:

```bash
cd /root/homelab
git clean -nd
```

Preview ignored/local artifact cleanup too:

```bash
cd /root/homelab
git clean -ndX
```

If the preview looks correct, apply cleanup:

```bash
cd /root/homelab
git clean -fd
git clean -fdX
```

Recommended env canonicalization after cleanup:

```bash
mkdir -p /root/homelab/config
test -f /root/homelab/config/.env || cp /root/homelab/config/.env.example /root/homelab/config/.env
grep -q '^HOST_ENV_FILE=/root/homelab/config/.env$' /root/homelab/config/.env || echo 'HOST_ENV_FILE=/root/homelab/config/.env' >> /root/homelab/config/.env
rm -f /root/homelab/host-daemon/.env
```

### 2.5 Validate hardened HOST self-update path

```bash
curl -fsSL -X POST http://127.0.0.1:8080/api/update
sleep 5
journalctl -u host-daemon -n 120 --no-pager
```

Look for update logs that include backup + watchdog behavior.

## 3) LXC Debug (container 109)

### 3.1 One-shot diagnostic for service, logs, port, docker

```bash
pct exec 109 -- bash -lc '
  echo "=== Service status ==="
  systemctl status lxc-daemon --no-pager -l

  echo "=== Log file (last 80) ==="
  tail -n 80 /var/log/lxc-daemon.log 2>/dev/null || true

  echo "=== Journal (last 80) ==="
  journalctl -u lxc-daemon -n 80 --no-pager 2>&1

  echo "=== GLIBC requirement in installed daemon ==="
  readelf -V /usr/local/bin/lxc-daemon 2>/dev/null | grep GLIBC | sort -Vu || true

  echo "=== Port 8080 check ==="
  ss -tlnp | grep 8080 || echo "NOTHING on port 8080"

  echo "=== Docker status ==="
  systemctl is-active docker && docker ps 2>&1 | head -5 || echo "Docker not ready"
'
```

## 4) Manual LXC Daemon Install (from HOST local artifact)

Use this when bootstrap installed a stale/broken daemon and you want an immediate manual repair.

### 4.1 Build a compatible artifact on CLIENT and sync repo to HOST

```bash
cd ~/Projects/homelab
make build-lxc
readelf -V ./apps/LXC 2>/dev/null | grep GLIBC | sort -Vu
```

### 4.2 On HOST, push that artifact into the container and restart service

```bash
pct push 109 /root/homelab/apps/LXC /usr/local/bin/lxc-daemon
pct exec 109 -- bash -lc 'chmod +x /usr/local/bin/lxc-daemon && systemctl daemon-reload && systemctl restart lxc-daemon && sleep 2 && systemctl status lxc-daemon --no-pager -l'
```

### 4.3 Validate runtime/API after manual install

```bash
pct exec 109 -- bash -lc 'systemctl is-active lxc-daemon && ss -tlnp | grep 8080 && curl -fsSL http://127.0.0.1:8080/api/health'
```

## 5) Collect complete evidence bundle

Run this on HOST and paste the full output when debugging:

```bash
echo '=== HOST ==='
systemctl status host-daemon --no-pager -l
journalctl -u host-daemon -n 120 --no-pager

echo '=== LXC 109 ==='
pct exec 109 -- bash -lc '
  systemctl status lxc-daemon --no-pager -l
  journalctl -u lxc-daemon -n 120 --no-pager 2>&1
  tail -n 120 /var/log/lxc-daemon.log 2>/dev/null || true
  readelf -V /usr/local/bin/lxc-daemon 2>/dev/null | grep GLIBC | sort -Vu || true
  ss -tlnp | grep 8080 || true
'
```
