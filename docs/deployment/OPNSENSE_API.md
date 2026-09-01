# The OPNsense API key: exactly what to create, and why

*Kenny's answer to form item X7 (2026-09-02): "ok, ik wil wel een api sleutel
geven, maar je moet dan exact zeggen hoe dat moet en welke rechten je nodig
hebt". This file is that answer, written down rather than said once.*

Two separate pieces of work need this key, and they need different
privileges. **Both are listed so the choice of how much to grant is a real
choice**, not a single take-it-or-leave-it bundle.

## What the key is used for

### 1 · H2 — static addresses, already built and switched off

`core/src/ops/kea.rs` gives a container its address by writing a DHCP
reservation instead of hard-coding one. It is complete code that has never
run, because it has no credentials. It calls exactly five endpoints:

```
GET  /api/kea/dhcpv4/search_subnet
GET  /api/kea/dhcpv4/search_reservation
POST /api/kea/dhcpv4/add_reservation
POST /api/kea/dhcpv4/set_reservation/<uuid>
POST /api/kea/service/reconfigure
```

Privilege needed: **Services: Kea DHCP** (`page-services-kea`).

The last one applies the change. Without it a reservation is written and never
takes effect, which is the "runs, reports success, wired to nothing" shape
this project keeps finding — so it is not optional.

### 2 · T57 — fencing CT 116 off from the rest of the house

↳ *CT 116 = the container running the kp-soft.dev site, the only one strangers
can reach.*

A firewall rule that blocks traffic FROM `10.10.10.16` TO the rest of
`10.10.10.0/24`, with an exception for what the site genuinely needs.

Privilege needed: **Firewall: Rules** (`page-firewall-rules`) plus
**Firewall: Apply** — a rule that is saved but never applied protects nothing,
the same trap as the reconfigure endpoint above.

## How to create it

1. OPNsense → **System → Access → Users**.
2. Either use an existing account or, better, add one named `homelab-api`
   (Kenny's call — a separate account makes it obvious in the audit log which
   changes came from the orchestrator, and revoking it does not touch a login
   Kenny uses).
3. Under that user → **Effective Privileges** → add:
   - `Services: Kea DHCP` — for H2.
   - `Firewall: Rules` and `Firewall: Apply` — for T57.
4. Same page → **API keys** → **+** → OPNsense downloads an
   `apikey.txt` containing two lines: `key=…` and `secret=…`.

Grant only the first bullet if only H2 should be enabled; only the second if
only T57 should be. The code paths are independent.

## What the orchestrator does with it

The credential file is read as a single line and handed straight to curl's
`-u`, so its content is exactly:

```
<key>:<secret>
```

It belongs at `/var/lib/homelab/secrets/opnsense.cred`, mode 0600, root-owned
— the same directory as the restic password, which is on the host-meta backup
(H10). It is never in git, never in a stack file, and never printed: the
transcript shows the curl command with `$(cat …)` unexpanded.

Then in `/etc/homelab/host.toml`:

```toml
opnsense_url = "https://10.10.10.1"
opnsense_cred_file = "/var/lib/homelab/secrets/opnsense.cred"
```

`curl -sk` — the `k` is deliberate and worth naming: OPNsense presents its own
self-signed certificate on the LAN. That is a knowingly accepted weakness on a
link that never leaves the house, not an oversight.

## What it is NOT allowed to do

The two privileges above cannot add users, change the WAN interface, touch
NAT, or reach the console. An OPNsense API user with only these two pages
cannot lock Kenny out of his own router, which is the failure worth being
careful about.
