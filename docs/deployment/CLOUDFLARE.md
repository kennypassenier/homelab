# The Cloudflare edge, captured

*T20. Read from the Cloudflare API on 2026-09-02 with a token Kenny supplied,
and written down here because until this file existed the entire outside face
of the house lived in one web console and nowhere else. Nothing on any of
Kenny's machines held it, so no backup could contain it: losing the account
meant reconstructing all of this from memory.*

*This is a CAPTURE, not a source of truth the orchestrator applies. Changing
Cloudflare still happens in Cloudflare. What this file buys is the ability to
answer "what was it?" — and to notice when the answer changed.*

## The whole edge, in three lines

```
CNAME *.kp-soft.dev  -> b027d052-9f97-43e5-9272-eeb336d3266e.cfargotunnel.com  (proxied)
CNAME kp-soft.dev    -> b027d052-9f97-43e5-9272-eeb336d3266e.cfargotunnel.com  (proxied)
tunnel ingress:  *.kp-soft.dev -> http://10.10.10.4:80   ; everything else -> 404
```

That is the complete list. There are exactly two DNS records, one tunnel, one
ingress rule and one catch-all.

**The consequence is worth stating plainly, because it decides how outages
look.** Every hostname under `kp-soft.dev` is one wildcard pointing at one
address: Traefik on the gateway, CT 104. Cloudflare has no per-hostname
knowledge at all — it cannot route `docs` differently from `fin`, and it
cannot fail for one name only. Which service answers is decided entirely by
Traefik, from the `Host` header, after the request is already inside the
house. So a single external check sees the tunnel fail; no number of external
checks sees one service fail.

| | |
|---|---|
| Account | `19c7db90b03ef77b410fce31ba5624bf` (Mendax1@gmail.com's Account) |
| Zone | `kp-soft.dev` — `f53290db3a400e6b3de07eda03c76267` |
| Tunnel | `kp-soft.dev-tunnel` — `b027d052-9f97-43e5-9272-eeb336d3266e`, healthy |
| WARP routing | disabled |
| originRequest overrides | none |

The tunnel connector itself runs as the `cloudflared` app in `stacks/gateway/`,
on CT 104. Its credentials live in that stack's `.env` in the host vault — the
tunnel *token* is the thing that must not be lost; everything above can be
recreated from this file.

## Access: who may reach it

Three applications, and they do not overlap the way the names suggest.

### `Homelab` — `*.kp-soft.dev`, session 730h

The catch-all, and the one that actually protects the house. One policy,
`Toegang kpsoft`, decision **allow**, matching three email addresses:

```
mendax1@gmail.com
kennypassenier@gmail.com
fabian.hernalsteen@gmail.com
```

Identity providers: Google (`mendax1@gmail.com`) and one-time PIN. A healthy
answer from outside is therefore a **302 to the Cloudflare login page**, not a
200 — which is exactly what the two external Uptime Kuma monitors accept, and
why they accept it.

The 730-hour session (a month) is why Kenny is rarely asked to log in again.

### `sp` — `sp.kp-soft.dev`, session 24h

One policy, `SuperSync Bypass`, decision **bypass**, include **everyone**.

This one deserves a second read: `sp.kp-soft.dev` is open to the internet with
no authentication at all. That is deliberate — SuperSync is a sync endpoint
that speaks to an app which cannot do a browser login — but it means the
sentence "everything behind kp-soft.dev sits behind Cloudflare Access" is not
true, and it has one exception with a name.

↳ *SuperSync = the sync service on CT 111 (`stacks/productivity`), the one
Super Productivity talks to.*

### `Kobo services` — `ha.kp-soft.dev`, session 24h

Two policies:

1. `Toegang kpsoft` — **allow**, the same three email addresses.
2. `kobo-token` — **non_identity**, service token
   `a836cb28-4c6e-440c-ac2b-b4317ee0b44c`.

The second is what lets the e-reader through: it presents a service token
instead of logging in. The token's *id* is recorded here; its secret is not,
and cannot be read back from the API — if that secret is lost, a new service
token has to be issued and the reader reconfigured.

## What a rebuild needs that is NOT here

Written down because a capture that pretends to be complete is worse than one
that names its gaps.

- **The tunnel token** — in the gateway stack's `.env`, in the host vault.
- **The `kobo-token` service-token secret** — not readable from the API by
  design. Recovery = issue a new one and reconfigure the e-reader.
- **The Google identity-provider client secret** — same: write-only.

## How this was read

```sh
# Everything below is GET-only.
curl -H "Authorization: Bearer $CF_TOKEN" \
  https://api.cloudflare.com/client/v4/accounts/$ACC/cfd_tunnel/$TUN/configurations
curl -H "Authorization: Bearer $CF_TOKEN" \
  https://api.cloudflare.com/client/v4/zones/$ZONE/dns_records
curl -H "Authorization: Bearer $CF_TOKEN" \
  https://api.cloudflare.com/client/v4/accounts/$ACC/access/apps
curl -H "Authorization: Bearer $CF_TOKEN" \
  https://api.cloudflare.com/client/v4/accounts/$ACC/access/apps/$APP/policies
```

The token Kenny supplied for this capture is not stored anywhere in this
repository, and re-reading needs a new one.
