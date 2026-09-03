# Resume point — 2026-09-02, evening

Written so a new session can pick this up without reading a chat log. Kenny's
standing instruction that day: *"ga door tot je niet meer kan, houdt er
rekening mee dat de limiet van het gesprek bereikt kan worden, dus zie dat er
niks verloren gaat."*

**Read `REGISTER.md` first** — every finding is numbered there and this file
only says what is IN FLIGHT.

## Where the project stands

- Phase 7 hardening: **21 of 23 gaps closed**, 1 deferred by Kenny (G6), G19's
  guard built and its backlog being worked off. Per-gap table in
  `REALIZATION_PLAN.md`.
- Host runs **v3.39.2**; repo version matches. 414 tests, CI green.
- `homelab` is installed at `~/.cargo/bin/homelab` (`make install`) and reads
  `~/.config/homelab/env`, so it works from any directory with no sourcing.

## The work in flight: promtail → Grafana Alloy

**Why.** Promtail reached end of life on 2 March 2026 (F249). Eleven
containers run `grafana/promtail:3.0.0`; two native containers (CT 109 kyu,
CT 112 almanac) ship no logs at all because they run no docker and never had a
promtail sidecar (F245).

**Kenny's decisions, form C1/C2 (2026-09-02):**

| Item | Answer |
|---|---|
| C1 · what ships logs on the native containers | **Alloy on kyu and almanac** |
| C2 · the eleven compose stacks still on promtail | **Nu meteen allemaal** — migrate them all in this session |

**Order agreed with him**, and the reason: Alloy goes on the two native
containers FIRST and is verified to deliver lines into Loki under
`stack="kyu"` before eleven working promtails are touched. His "all of them"
answer was about not waiting for another decision round, not about skipping
the proof. If a migrated stack stops delivering logs and cannot be fixed
quickly, put that one stack back on promtail and report — do not continue
down the list with the first one broken.

Migration order, least consequential first, ending where Loki itself runs:

    registry · syncthing · home · uptime · kp-soft · productivity
    metrics · paperwork · downloader · media · gateway

**Status 2026-09-02 evening.** Native done and verified: kyu (CT 109),
almanac (CT 112) — both shipped nothing at all before. Compose stacks
migrated and verified in Loki: **registry, syncthing, home, uptime, kp-soft,
productivity, metrics, paperwork, downloader, media, gateway** — **all eleven done**, no promtail left in the fleet (F256)
— gateway last on purpose, Loki itself runs there.

Per stack the work is: drop `promtail` from `apps:` in `lxc-compose.yml`,
`git rm -r stacks/<name>/promtail`, `homelab deploy stacks/<name>`, then query
Loki. The deploy's own garbage collect stops and removes the promtail
container when it leaves the apps list, so `prune-orphans` has nothing to do.
Watch for a manifest whose `apps:` is a block sequence rather than a flow list
(syncthing was) — editing it wrongly leaves promtail listed with its directory
already gone.

**Verification after each stack** (this is the whole point — a shipper that
runs and delivers nothing is the fault this project keeps finding):

```sh
curl -s -G "http://10.10.10.4:3100/loki/api/v1/query_range" \
  --data-urlencode 'query={stack="<naam>"}' \
  --data-urlencode "start=$(( $(date +%s) - 600 ))000000000" \
  --data-urlencode "end=$(date +%s)000000000" --data-urlencode 'limit=1'
```

Zero streams after a migration = that stack is broken, whatever the container
says about itself.

## Fleet state after the migration, 2026-09-02 21:00

**2026-09-03: `homelab check` reports NO broken findings at all.** The
OPNsense backup works — Kenny put the privilege on the right user and four
faults on our side were fixed after it (F259). Everything left is drift from
unanswered manual checks, which is the G17 mechanism doing its job, plus one
deliberate `noted` about the registry cache.

Run the router backup on demand with `homelab backup-devices`; it also rides
the nightly round in the host-meta slot.

## Also queued, in this order

1. **G19 backlog** — down from 42 to **18**. The ratchet in
   `register_tests.rs` stops new ones and refuses to sit stale. What is left
   are real code findings that need their test named (F4, F16, F189, F191,
   F204, F207, F210, F212, F216, F225, F240, F241, F252 …) rather than the
   Phase-1 observations, which are annotated as deliberately untestable.
3. **F247** — nothing in the house sends almanac anything (zero events in 48 h).
   Kenny's call whether that is a pipeline still to build or a service to retire.

## Queued: almanac's fixture profiles (form F1, 2026-09-03)

Kenny chose to remove all three. Waits on almanac 2.0.0 landing, or on that
project confirming its tests do not read the live files. See the gate log in
`REALIZATION_PLAN.md` for the exact trigger.

## Waiting on Kenny (not on us)

- **`latch key backup`** now that latch 2.3.0 is out — his passphrase, his command.
- **Correction form** for the Jellyfin stream-check duplication, queued in
  `QUEUED_MINI_ROUNDS.md`.
