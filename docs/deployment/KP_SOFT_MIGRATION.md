# Moving kp-soft to its final address

Written 2026-08-31, the day kp-soft was deployed and the same day it was
taken back off the edge. Kenny asked for these findings to be written down so
the migration does not rediscover them.

kp-soft runs on **CT 116, 10.10.10.16:8080**, reachable only from the LAN. It
has **no gateway route at all** — deliberately, see below. The goal remains
`https://kp-soft.dev`: public, no interactive login in front of it.

## The finding that decides everything

**Every `*.kp-soft.dev` hostname sits behind Cloudflare Access, and Access
answers 401 to writes.**

The first deployment used `website.kp-soft.dev` as a development address.
Reading worked; every write failed with a 401 whose only visible text was
"cloudflare". Measured rather than reasoned:

```
PATCH https://website.kp-soft.dev/settings/theme
  → HTTP 401
    www-authenticate: Cloudflare-Access
      resource_metadata=".../.well-known/cloudflare-access-protected-resource/settings/theme"
    server: cloudflare

PATCH http://10.10.10.16:8080/settings/theme      (same request, edge skipped)
  → HTTP 419                                       Laravel's own CSRF answer
```

The app was behaving correctly throughout. Access can redirect a page
request to its login screen; it cannot redirect an XHR or a non-GET, so it
returns 401 instead. The result is a site you can read and cannot use: no
theme, no roles, no passkey.

This is why the development address was abandoned rather than kept. A name
under this domain is either behind Access — and then unusable — or excluded
from it, at which point it is as public as the apex would have been, so it
buys nothing.

## The trap that decides WHEN to migrate

**A passkey is bound to the hostname in `APP_URL`.** WebAuthn derives its
relying-party id from it, so a passkey registered while the app answers on
one hostname stops working the moment it answers on another.

Found by the kp-soft session on 2026-08-31, not by this one — it is the
reason "temporary hostname" is not a free choice here. Concretely:

- Passkeys registered on `10.10.10.16:8080` will not work on `kp-soft.dev`.
- Passkeys registered on `website.kp-soft.dev` would not have worked either.
- Therefore: **do not ask anyone to register a passkey before the final
  address is live.** Password and magic-link logins survive the move; passkeys
  do not.

There is a second reason not to test passkeys today: WebAuthn requires a
secure context, and the current address is plain http. It cannot work now
even if you wanted it to.

## What the migration actually needs

Two of the three steps are Kenny's, in the Cloudflare Zero Trust dashboard.
That dashboard is where this tunnel's ingress lives — there is no file in
this repository that routes hostnames, and the homelab has no write access
to Cloudflare.

1. **Add `kp-soft.dev` as a tunnel hostname**, pointing at the same place
   every other hostname points: Traefik on the gateway (`http://traefik:80`
   within `platform_net`).

2. **Make sure it is NOT covered by the Access application** that guards
   `*.kp-soft.dev`. Measured 2026-08-31: the apex answers 404 from Cloudflare
   and is *not* Access-guarded, while `www.kp-soft.dev` answers 302 to the
   login. That is the difference to preserve. Verify after the change with:

   ```
   curl -sI https://kp-soft.dev/ | head -1
   ```

   A `302` to `mendax1.cloudflareaccess.com` means Access caught it and the
   public site is invisible to everyone but Kenny. That is the failure mode
   to watch for, and it will not announce itself.

3. **This side**, which is one commit and one deploy:
   - restore `stacks/kp-soft/traefik-routes.yml` with
     `Host(`kp-soft.dev`)` → `http://10.10.10.16:8080`;
   - restore the `gateway_route` block in `stacks/kp-soft/lxc-compose.yml`
     (`filename: 116-app-kp-soft.yml`, `gateway_vmid: 104`);
   - change `APP_URL` to `https://kp-soft.dev` in **latch**, under
     `kp-soft/kp-soft` — not on the container. The deploy pushes the `.env`
     from latch every time, so an edit made on the container is gone at the
     next deploy;
   - update the Homepage entry in `stacks/home/homepage/services.yaml` back
     to the https name — it is currently the one deliberate `http://` link on
     that page;
   - `homelab deploy stacks/kp-soft` and `homelab deploy stacks/home`.

## What to check after the move

The handover's own list, which all passed on 2026-08-31 and should pass
again:

| Check | Expect |
|---|---|
| `/up` | 200 — the health endpoint, also what Uptime Kuma probes |
| `/`, `/puzzels`, `/sketches`, `/themas` | 200 |
| `/hangmaten` | 302 to login — the role guard, not a fault |
| A write, e.g. `PATCH /settings/theme` | anything but 401-from-cloudflare |
| The rendered HTML | no `http://` links — proxy headers arriving |
| `php artisan kpsoft:admin <mail>` | prints an **https** link on the final host |

The last two are the same proof from both sides: if `X-Forwarded-*` reaches
the container, Laravel builds https links and a magic-link mail does not
carry a downgraded URL.

## Two things that will bite a rebuild

- **The registry login is not in any manifest.** The image is private on
  GHCR, and the orchestrator has no mechanism for a registry credential — it
  pushes compose files and runs `docker compose pull`. CT 116 was logged in
  by hand once; a container rebuilt from scratch needs
  `docker login ghcr.io -u kennypassenier` with the token in latch under
  `kp-soft/registry`. Tracked as T56.

- **`APP_KEY` must survive.** It encrypts sessions and encrypted columns.
  Restoring the database without it logs everyone out and leaves those
  columns unreadable. It lives in latch under `kp-soft/kp-soft` and is
  therefore in the nightly backup — but only as long as the whole `.env` is,
  so never hand-edit that file on the container.

## Still open

- **T57** — CT 116 will be the only container strangers reach directly, on
  the same VLAN as Home Assistant and everything else. Kenny accepted that
  deliberately (K5) and asked for the OPNsense restriction to be planned
  separately rather than block a working site. It is worth doing *before* the
  apex goes live, not after.
