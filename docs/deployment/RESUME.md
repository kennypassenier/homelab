# Resume point — 2026-09-04, early morning

Written so a new session can pick this up without reading a chat log. Kenny's
standing instruction that day: *"ga door tot je niet meer kan, houdt er
rekening mee dat de limiet van het gesprek bereikt kan worden, dus zie dat er
niks verloren gaat."*

**Read `REGISTER.md` first** — every finding is numbered there and this file
only says what is IN FLIGHT.

## Where the project stands

- Phase 7 hardening: **22 of 23 gaps closed**, 1 deferred by Kenny (G6).
  Per-gap table in `REALIZATION_PLAN.md`.
- Host runs **v3.42.2**; repo version matches. 434 tests, CI green.
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

1. **G19 backlog — closed.** Measured 2026-09-03 under the rule F237 settled:
   107 finished rows, **0** pointing at nothing. The guard in
   `register_tests.rs` is an absolute assertion rather than a ratchet, so it
   holds the ground instead of measuring a descent. A "42 down to 18" figure
   I reported on 2026-09-02 came from a superseded matcher and was wrong
   (F272).
3. **F247** — nothing in the house sends almanac anything (zero events in 48 h).
   Kenny's call whether that is a pipeline still to build or a service to retire.

## Done 2026-09-04 · the backup asks before it quiesces (F280, F282)

Kenny was streaming at 04:17 when the nightly round ran `docker stop` on all
six media containers to take a clean snapshot and started them again thirty
seconds later. His episode skipped to the next one; every media tile on the
front page went to ECONNREFUSED.

The check that prevents exactly this had been armed for two days — on the
UPDATE path. The backup path stops the same containers every single night and
never asked. (An earlier note here blamed a dormant `stream_guards` setting;
that was wrong — F233 withdrew that mechanism on 2026-09-02 because O10
already existed and worked. The real gap was the second caller.)

The question now lives in `busy::app_busy` and both paths ask it. A backup of
a stack in use returns `CoreError::Deferred`, a third state that is neither
success nor failure: no `last_backup` timestamp is written for work that did
not happen, and H8 does not park the stack for being watched. Tomorrow it runs.

**Live-proven the same night**: `homelab backup stacks/media` while Kenny was
paused on an episode returned *"backup deferred — jellyfin is in use: Kenny is
paused on You Must Be Caspian"*, and nothing was stopped. Host v3.43.1.

## Done 2026-09-04 · Recyclarr manages the profiles (F281)

Kenny: *"Ik wil gewoon het eindresultaat … let me know als het af is."* It is
running, it has synced, and the result was read back out of both APIs rather
than out of its own log.

- Image pinned to **8.7.2**; `:latest` has never been published, which is why
  the preset could not start. Policy `manual`, because a pinned tag with an
  auto policy reports a successful update every night and changes nothing.
- It lives in the media stack (`stacks/media/recyclarr/`) and reaches the
  applications by container name. `/config` is a docker volume, not an
  `/appdata` mount: it holds only a cache, and a seventh mountpoint on CT 106
  would have renumbered the three media libraries after it.
- The API keys are in `/var/lib/homelab/secrets/media/recyclarr.env` on the
  host. Nothing in git holds them.
- **R7 is solved without renaming anything**: `name:` overrides the guide's
  profile name, so Radarr id=4 and Sonarr id=4 — the profiles 961 films and
  210 series already point at — are managed in place. The pre-change export
  stays in `captured/media-profiles/`.
- The one place the guide is contradicted: TRaSH's Sonarr WEB-1080p allows WEB
  and nothing else, which would have stopped 210 series from grabbing a Bluray
  release at all. R1 says the ceiling is Bluray-1080p, so the qualities come
  from R1 and only the scores from the guide.

If Recyclarr work resumes, the thing to know first: **`minFormatScore` is 0 on
every profile**, so a small negative score does not deprioritise a release, it
refuses it. Preferences are expressed as positive scores on what should win.

## Done 2026-09-04 · R12/R13 and the pools (D103, F283, F284)

Kenny looked at the synced profiles and read them as *"bijna alles is er
precies uitgesloopt"*. Measured: the quality list was 26 rows before and 26
after; two ticks fewer (HDTV-1080p, Remux-1080p). The work moved from five
ticks to 59 custom formats with 25 scores. `quality_sort: top` puts the
allowed qualities at the top and everything disabled below, which is what
makes it look stripped.

**Whether it is measurably better is now actually measured** — the LQ custom
format's own regexes run against his release groups: **273 of 943 films** come
from YIFY (180), RARBG (53), Tigole (15), YTS (10), LAMA (8), PSA (7), all at
−10000. Of his fifteen most common sources (544 films) **not one** scores
positively.

**R12**: the stopscore stays at 10000 — Radarr keeps looking for better
releases instead of calling every film done. **R13**: the library may grow
from 4.1 TB toward ~10 TB (median 27 MB/min today against his own 95 MB/min
preference), with the space check built alongside. 11.5 TB free on
`/data/18TB/Movies` where 942 of the 943 films live.

The check itself is `PoolFact` + `evaluate_pools` + `pool_facts_from_df`,
reading the pools each stack declares in its own `data_mounts`. Two things
worth carrying forward:

- **A percentage, not "warn under 2 TB"** — which is what R13 asked for and is
  the wrong shape once measured: HDD2TB has 1798 GB free at 1% used. The free
  space is in the message instead. Drift 80, broken 90, both configurable.
- **It logs what it measured, not only what is wrong** (F283), and that line
  paid for itself in five minutes: it showed the 18 TB pool attributed to the
  downloader alone when media mounts it too (F284). A silent check cannot be
  told apart from a check that reads nothing.

Host v3.44.2, 455 tests.

## Done 2026-09-04 · the four loose ends (N1-N4) and what N1 uncovered

Kenny's answers: N1 Onmisbaar, N2/N3/N4 Gewenst.

- **N1 · the G13 native drill passed** (F286). CT 118 from nothing: container
  from template 998, bind mount, unit file, binary staged from the GitHub
  release with its checksum, then — as designed — `NOT started — missing:
  token.env, config.toml`. With both in the vault: restored, started,
  `is-active` = active, reconcile matched on 4 points. CT 118 destroyed, its
  `/appdata` and vault entry removed. F228 closed.
- **N3 · the SMART collector rides the host-meta snapshot** (F274). Proven:
  `restic ls latest` on `host-meta-config` names all three files. Absent
  extras are skipped rather than fatal — restic refuses a whole snapshot over
  one missing source.
- **N4 · six orphaned `.bak` files removed** from CT 113, after checking the
  live `prometheus.yml` is byte-identical to the repo's.
- **N2 · all thirteen stacks deployed.** Every one "deploy complete", and not
  one container restarted — jellyfin still `Up 10 hours` afterwards. The repo
  and the machines agree.

**And the drill found a real fault (F285).** The throwaway stack declared a
native unit named `http-switchboard` — also a live service. Repositories are
named after the owning app (D25), so its backup-before-destroy wrote into the
LIVE repository and its retention then deleted that service's snapshot from
that night. Fixed by hand (`restic forget`), guarded by `conflicting_owner`
as the first step of every backup, and a correction form is open with Kenny.

Five older findings were measured rather than relayed and turned out to be
closed already: F165 (gateway repos), F179 (kyu's own backup), F59 (Loki's
logs), F186 (host.toml), F126 (promtail, gone with the fleet).

Host v3.45.0, 459 tests.

## Done 2026-09-04 · the restore drill (F293), and JobTracker is live

Kenny's form H1: one drill, two proofs. Both passed.

- **The F285 guard, live-proven.** The drill stack deliberately declared the
  app `jobtracker`, which kp-soft owns. The backup refused at `owner conflict`
  with **zero restic commands executed** — read off the transcript. The
  measurement that kept the entry in `QUEUED_MINI_ROUNDS.md` open has
  happened; that loop is closed.
- **Restore from zero.** Snapshot `32af9e22` (264 files) restored into a fresh
  CT 118, a container built around it, JobTracker 0.2.0 started on the
  restored data: healthz 200, and against the live copy **23 dossiers and 199
  files on both sides with an empty diff**. Container, data and the temporary
  repository removed.

Two new findings the drill produced:

- **F291** — `homelab backup` built the whole deploy spec, secrets included,
  and threw all but the manifest away. With the latch key detached it refused
  a backup it did not need a secret for, mid-drill. `spec::build_manifest` now
  serves backup and destroy.
- **F292 (open, T82)** — the owner guard is symmetric and knows no seniority,
  so the drill stack blocked kp-soft's own backup for six minutes. The stack
  that already owns the repository should keep it; only the newcomer refused.

JobTracker itself is live beside kp-soft on CT 116, on `job.kp-soft.dev`
behind Access, at 0.2.0 with Almanac configured through environment overrides
(D104, and the fix that followed Kenny hitting a "Not configured" error).

Host v3.47.0, 464 tests.

## Done — was: queued, needs Kenny's own go: JobTracker

The JobTracker session asked (2026-09-04) to deploy `ghcr.io/kennypassenier/
jobtracker:latest` here and add it to Homepage. Preset ready at
`~/Projects/JobTracker/deploy/homelab-preset/`. It is NOT started: a new
stack, a Cloudflare Access route and three secrets are Kenny's to approve, and
a peer session's relay is not his approval. Two secrets only he can make:
`JOBTRACKER_PASSWORD_HASH` and `JOBTRACKER_GIT_TOKEN`; the Almanac token is in
latch. Watch the uid: the container runs as 10001, and on an unprivileged LXC
the host-side chown must use the mapped uid (110001), not 10001 — measured
today via the drill, where a hand-placed root-owned file was unreadable inside
the container.

## The work in flight: four releases rolled out (2026-09-05, 04:30)

Kenny's instruction: *"Er zijn vier projecten die een nieuwe release kregen en
uitgerold moeten worden door jou: kyu, almanac, kp-soft.dev en JobTracker"*,
followed by *"ga voor de nieuwste release en/of image"*. Full account in
**D108**; three of the four are live.

| Project | Was | Now | How |
|---|---|---|---|
| kyu (CT 109, native) | 2.2.0 | **2.4.1** | `install-native` |
| almanac (CT 112, native) | 2.3.0 | **2.4.0** | the nightly round did it at 04:17 |
| JobTracker (CT 116, docker) | 0.2.0 | **0.3.0** | `update`, after F294 |
| kp-soft (CT 116, docker) | v0.2.0 | **v0.3.0** | `deploy`, after D109 |

**kp-soft landed last, along a different road (D109).** The deploy needs the
stack's `.env`, and no agent session on this machine holds the latch key —
`latch state` says `key MISSING` for all four projects and `PAT MISSING` too;
the kp-soft session measured the same from its side. Kenny's answer was to
stop working around latch: local `.env` files (gitignored via `*.env`) are the
working copy, latch stays the store, and they get pushed back into latch.

So kp-soft's env was copied byte-exact out of the host vault
(`/var/lib/homelab/secrets/kp-soft/kp-soft.env`, sha256 `2231f824…` both
sides) into `stacks/kp-soft/kp-soft/.env`.

**That first attempt had to empty `latch_secrets`**, because `build_spec`
refused an app carrying both a local `.env` and a latch entry. Kenny read that
refusal and asked the right question — *"latch beheert .env files dus als die
file er is, dan moet die toch prioriteit krijgen?"* — which opened a
mini-round on D12 and retired the refusal (**D110**). A local file now wins,
every app reports its source on every deploy, and `latch_secrets: [kp-soft]`
is back in the stack file: that line says where the secrets LIVE, the file
beside it says what is being read today.

**All thirteen latch-backed apps now have a local `.env`** (Kenny's MR2
answer), pulled from the host vault with a sha256 compared on both sides for
each, all fourteen confirmed gitignored. `homelab plan stacks/gateway` runs
without a latch key and says `[env] traefik <- local .env (latch skipped)`.

**Not done: `latch commit` without a key.** That mints a new one, which is
what cost the project's history on 2026-09-02 (D104). Handing an app back to
latch is one deletion — remove its local `.env`, change nothing else.

**F294 came out of this** and is fixed: `update` and `restore` both built the
whole deploy spec — every secret out of latch — and then sent only the
manifest. The same fault F291 closed for `backup` a day earlier, still open in
the two verbs beside it. JobTracker uses no latch secret at all and was
blocked anyway; with the fix it updated without a key.

## Live state after this session (2026-09-05, 05:40)

Host and client both **v3.48.0**. `homelab check`: 0 broken.

| What | State |
|---|---|
| kyu (CT 109) | 2.4.1, healthz ok, `kyu-backup.service` success |
| almanac (CT 112) | 2.4.0, healthz ok |
| JobTracker (CT 116) | 0.3.0, healthy, HTTP 200 |
| kp-soft (CT 116) | v0.3.0, healthy, `/up` 200 |
| Secrets | 14 local `.env` files, all gitignored; deploys name their source |
| CT 109 + CT 112 descriptions | derived, no longer hand-written (F297) |

**The `.env` files are the working copy, latch is the store.** Refresh one
from the vault with
`ssh root@10.10.5.250 'cat /var/lib/homelab/secrets/<stack>/<app>.env' > stacks/<stack>/<app>/.env`
and compare the sha256 on both sides. Handing an app back to latch is one
deletion: remove its local `.env`, change nothing else — the `latch_secrets:`
line stays, because it says where the secrets live.

Still open on latch itself: this machine has no key and no PAT for project
`stacks`, so nothing can be pushed back INTO latch yet. Not re-minted (D104).

## Waiting on Kenny (not on us)

- **`latch key backup`** now that latch 2.3.0 is out — his passphrase, his
  command. Checked 2026-09-04: the newest `.age` is from 14:12 on 2026-09-02,
  three hours older than the key activity that day.
- **The 28 manual checks** (`homelab checks`), including one new one: look at
  Radarr's 1080p profile once, now that Recyclarr writes it.
- **Correction form** for the Jellyfin stream-check duplication, queued in
  `QUEUED_MINI_ROUNDS.md`.
