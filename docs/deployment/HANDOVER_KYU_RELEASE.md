# Handover to the kyu project: publish a release binary

*T72 / F168. Kenny rated this Onmisbaar on 2026-09-02 (form X5). The work is
in `~/Projects/kyu`, so it runs in a session opened there — this file is what
the homelab project owes that session: the measurement, and what the other
side already expects.*

## What was measured, and when

On 2026-09-02, while declaring where each native service's binary comes from:

```sh
$ gh release view --repo kennypassenier/kyu --json tagName,assets \
    --jq '.tagName + " :: " + ([.assets[].name]|join(","))'
v2.0.0 ::
```

Empty after the `::`. Release v2.0.0 of the kyu hub carries **no assets at
all** — no binary, no `SHA256SUMS`. The other three native services do:

| service | tag | assets |
|---|---|---|
| kyu | v2.0.0 | *(none)* |
| kyu-runner | v0.1.0 | `kyu-runner-x86_64-linux-musl`, `SHA256SUMS` |
| http-switchboard | v1.0.0 | `http-switchboard`, `SHA256SUMS` |
| almanac | v1.4.0 | `almanac`, `SHA256SUMS`, `SHA256SUMS.minisig`, `VERSION` |

## Why it matters here

Since 2026-09-02 the orchestrator can finish a container it built:
`homelab install-native` downloads a release asset with Kenny's authenticated
`gh`, verifies it against that release's `SHA256SUMS` **on the desktop**, and
ships the verified bytes over the TLS line. Three of the four services can be
installed that way. The kyu hub cannot — and it is the one the other two on
CT 109 talk to.

`stacks/kyu/lxc-compose.yml` has promised since it was written that a rebuild
goes: recreate the container, restore the data, then "the three binaries are
installed the way C7 installs them". For kyu that promise is currently false.

**Installing without a checksum is refused deliberately** and that will not be
relaxed: an unverified binary placed into a container is exactly the
hand-built step this verb exists to replace. So there is no half-fix — the
release has to carry both files.

## What the homelab side already expects

`stacks/kyu/service.yml` carries no `release_repo`, with a comment saying why.
The moment the kyu release publishes assets, that file gains two lines and
nothing else changes:

```yaml
release_repo: kennypassenier/kyu
# release_asset only if the asset name differs from the unit name "kyu"
```

The asset name may be anything; if it is not literally `kyu`, name it with
`release_asset`, the way kyu-runner does for its target-triple name.

`SHA256SUMS` is matched the standard way — one line per file,
`<64 hex>  <filename>`, with an optional `*` before the filename. The
verifier is case-insensitive on the hash and matches on the exact asset name.

## Definition of done

1. A kyu release (a new tag, or v2.0.0 re-uploaded) carries the Linux binary
   and a `SHA256SUMS` listing it.
2. `stacks/kyu/service.yml` in the homelab repo gains its `release_repo`.
3. `homelab install-native stacks/kyu` runs against a throwaway container and
   the unit comes up — not against CT 109, which is live.
4. F168 and T72 close in `docs/deployment/REGISTER.md`.

Steps 2 to 4 are homelab-side and belong to this project; step 1 is the kyu
project's, and nothing here can do it.
