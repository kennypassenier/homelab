# homelab-admin — architecture decisions

Phase 3 (tech choice) decided by Kenny on 2026-09-28 (form "Dashboard
techkeuze", answers 12:12). Phase 4 decisions follow below once frozen.

## Inherited, not asked

- Rust server, plain HTML + ES modules in the browser, no build step
  (Phase 0).
- `clippy --all-targets -D warnings`; failures are `Result`, never a panic.
- License `MIT OR Apache-2.0`, like the rest of the homelab workspace.
- Dependency policy of the homelab workspace: small pure-Rust crates may be
  added; anything with C bindings, its own network stack or a heavy
  transitive tree needs a mini-round.
- English UI text (Phase 0).

## Phase 3 decisions

| ID | Decision |
|---|---|
| tech-msrv | The whole workspace moves to `rust-version = "1.97"` (chassis-rs requires it). One pin; the CI MSRV job keeps reading it from `Cargo.toml`. |
| tech-charts | The eight visualisations are drawn by the dashboard's own small SVG modules, coloured with the kp-themes `--chart-*` tokens. No charting library. kp-themes keeps charts out of its scope (its `docs/FEATURES.md`); the modules may move there later. |
| tech-js-checks | Frontend checks mirror kp-themes: JSDoc types checked by `tsc` (`checkJs`, `strict`, no emit), `prettier`, `node --test` for pure modules, Playwright in Chromium and Firefox for pages. |
| tech-kp-intake | kp-themes reaches the dashboard **through chassis-rs**, which already vendors a checksummed subset of kp-themes 7.2.0 (`crates/chassis/static/kp/`, 113 files, `KP_THEMES.sha256`) and serves it under `kp/…`. That subset lacks the component modules the dashboard needs (`wizard.js`, `palette.js`, `datatable.js`, …); chassis-rs is asked to vendor them in 2.3.0. homelab keeps no second copy. (Kenny: "kp-themes zit bij in chassis-rs dus ik zie het probleem niet?") |
| tech-chassis-dep | Depend on a released chassis-rs only: `tag = "v2.3.0"`. Implementation (Phase 5 onwards) waits for that release; Phase 4 can proceed. **Released 2026-09-28** (v2.3.0 = ba7223a): pin `tag = "v2.3.0"`, features `core`, `webapp`, `live`. |
| tech-risk-class | **Recoverable, with a local-first twist** (Kenny's own answer): a small subset at commit time, including the crucial security tests; the full suite runs locally on Kenny's machine before a release, not on GitHub Actions. Fewer, larger commits: development speed matters as much as the tests. |
| tech-platforms | Linux (Garuda), Windows, and an Android smartphone; Chrome and Firefox on each. Every page must work at phone width. |
| tech-envs | Four environments, approved as listed: CT 120 (Debian 13, LAN reach to `homelab-host` 10.10.10.250:8443, own latch credential, data under `/appdata/admin/admin-config`); the development machine (WSL and Garuda: Kenny's full token, cargo, node 26, Playwright browsers in `~/.cache/ms-playwright`); GitHub CI (no LAN, no host, no credentials, mocks only); Kenny's browsers (own theme choice, reduced motion, time zone Europe/Brussels vs UTC in CI, local storage). Phase 7 tests every difference. |

### The security subset at commit time (tech-risk-class)

Named here so the gate can list it; refined in Phase 4 once the modules
exist:

- token scope enforcement (a read token cannot operate, an operate token
  cannot destroy, exec or update the host);
- the destroy confirmation compares the typed stack name;
- secrets never appear in logs, transcripts or error bodies;
- the login gate: nothing under `/app` or `/events` answers without a session;
- LAN-only binding refuses a non-private peer.

## Phase 4 — FROZEN 2026-09-28 12:40 (form "Dashboard architectuur", freeze: Akkoord); arch-exposure decided 12:44 in a follow-up round

Facts (measured 2026-09-28): deploys push content inline as `DeploySpec`
(proto/src/lib.rs:32, core/src/manifest.rs:562), secrets in its `env`,
`source` recorded but never compared (manifest.rs:602-611); the host serves
one session's requests serially, only `Answer`/`Today` beside the queue
(host/src/main.rs:4436-4460, 4540-4548); queued requests still run after a
disconnect (main.rs:4507-4512); Ask ids restart at 1 per host process
(main.rs:2763); no history beyond last-run scalars and a 4 MiB
`journal.jsonl` (main.rs:2695); no current-op or log ring (main.rs:3150);
one bearer token, no scopes (main.rs:398, 3634); scheduler only inside the
nightly window (main.rs:3759-3776); chassis serves no TLS itself and treats a
request as HTTPS only via a trusted proxy's X-Forwarded-Proto
(chassis guards.rs `is_https`, passkeys.rs:267-281, auth.rs:288); chassis
`passkeys` pulls OpenSSL through webauthn-rs; an unconfigured chassis login is
an open door (auth.rs:143-146); chassis self-update needs `SHA256SUMS.minisig`
(update.rs:444-445) and homelab releases are checksum-only
(client/src/release.rs:58-61); `latch commit` records every file missing
locally as removed (latch core/src/ops/sync.rs:73,133-163); `client::spec`
reads process env, prints notes with `eprintln!` and shells out to `latch` and
`gh` (spec.rs, release.rs:123). pve conntrack sees container-to-container
flows: `net.bridge.bridge-nf-call-iptables = 1`, 116 live 10.10.10.x→10.10.10.x
entries sampled 12:28 (e.g. .7 → .13:3100, Alloy to Loki).

### Decided in the draft, critic objections absorbed

| ID | Decision |
|---|---|
| arch-crates | Crate `admin` (binary `homelab-admin`), core/shell split (`admin::core` zero I/O). The client crate gains a default `tui` feature so `admin` reuses `link`, `tls` and `spec` without ratatui. `link` takes the pin path as a parameter. `spec` is refactored: notes are returned, not printed; the secret source (latch) and the release source are traits; the edit plan lists binary changes, so a firewall edit never ships a new release silently. |
| arch-host-link | CT 120 keeps one WSS session to `homelab-host`. The host runs every read-only command and `Audit` beside the op queue (the scope table marks read-only), so the dashboard never freezes behind a deploy. Reconnect never resends a mutating command: in-flight requests become "outcome unknown" and are resolved from History/CurrentOp. Features are gated on Hello `version`/`build` (admin and host update at different moments). |
| arch-protocol | Additive fields, no `v` bump (amended AR5): request id and timestamp on Log and step events; `CurrentOp` (holder + an in-memory ring of recent lines); JSON replies for check, doctor, incidents, manual checks, backups, snapshots; `History`; `ConfigView` for all host.toml keys. An `Answer` carries host boot id + op + step and the admin checks it against the host's open asks before forwarding. |
| arch-history | Host-owned `history.jsonl` (ops with step start/end, nightly phases, backups, drills, copies), atomic append, a torn last line skipped on read, window and size in host.toml. Ops with a start and no end (read from the AR13 journal) show as "interrupted". The backup calendar is back-filled from restic snapshots, deploys from the intent-history commits, so day one is not empty. |
| arch-traffic | The host samples `conntrack -L` on pve (interval and retention in host.toml), aggregated per (src vmid, dst vmid, port). Measured: pve sees bridged container flows. Shown as "sampled connections", not "measured traffic". |
| arch-tokens | `[[token]] {name, scope = read\|operate\|all, sha256}` in host.toml; one exhaustive `Command → scope` table in `proto` (test fails on a missing row), also marking read-only commands; the single legacy `token` = scope all until removed; refusals and all-scope commands audited with the token name. |
| arch-edit-txn | fetch + fast-forward (refuse otherwise) → validate with `homelab-core` → write → commit (only paths under `stacks/<that stack>/`) → push (on error: `git ls-remote` decides whether it landed before any local reset; no token ever in the remote URL or an error body) → deploy of that exact commit. One edit at a time. At start an unpushed commit is compared with the host's `source.commit` and offered as push, rebase or drop. |
| arch-secrets-read | Reveal is a POST; audit line on the host first, no ack → no reveal; value once with `no-store`, never over SSE or in logs; hidden after the configured seconds; latch offline → shown as stale. Secret-edit drafts are never persisted. The admin redacts known latch values from host lines before publishing them over SSE. |
| arch-login | The admin refuses to start (and fails `--check`) without login secrets; a commit-subset test covers it. SSE streams close on logout/expiry (asked of chassis-rs). |
| arch-self | The admin refuses destroy/rebuild of `stacks/admin`; deploying itself is announced as "restarts the dashboard" and reconciled after the restart. Host keys that can cut the dashboard off (`token`, `[[token]]`, `listen`, `state_dir`) are read-only in the browser. |
| arch-state | Admin data = typed JSON, `schema_version`, temp + fsync + rename, under `/appdata/admin/admin-config`; working copy at `…/repo`. |
| arch-errors | `thiserror` per layer; API errors are JSON `{what, why, fix}`. |
| arch-frontend | One ES module per page, history-API router, one store (snapshot + SSE deltas, "measured x ago"), pure view models under `node --test`, SVG chart modules, kp-themes from chassis `/static/kp/` (2.3.0 serves the whole 7.2.0 `js/` set, 41 modules), phone width. |
| arch-config | Admin: `/appdata/admin/admin-config/admin.toml` + env + `--check` (host URL, pin path, Loki, Prometheus, reveal seconds, SSE buffer, backoff, timeouts, git remote/branch, chart window, GetState poll interval). Host: the new knobs (ring size, history window/size, conntrack interval/retention, schedule tick) go into host.toml with its start-up validation. |
| arch-firewall | CT 120's firewall is `enabled: true` from the first deploy: inbound only from a configurable list of Kenny's client addresses; outbound to the host, CT 113 (Loki 3100, Prometheus 9090), GitHub and latch's remote. CT 120 is the one allowed exception to "never pve's rescue address from a container", for :8443 only. |
| arch-tests | A mock host built from the real host session loop in-process (serial worker, broadcast, Ask ids); Playwright against the real binary. Commit subset: the five security tests + login-refusal + fmt/clippy/tsc. Full suite locally before a release. |

### Decided by Kenny on the form (2026-09-28 12:40)

| ID | Decision |
|---|---|
| arch-update | **Sign homelab releases** with the ecosystem's minisign key before the dashboard's first release; the dashboard then updates through chassis self-update driven by homelab, like kyu. Kenny gives the offline key's passphrase per release. |
| arch-secrets-edit | **latch gets a single-file set operation** (mini-round asked of latch-rs); editing secrets in the dashboard waits for that latch release, revealing does not. **Delivered 2026-09-28: latch 2.6.0**, signed by Kenny and installed on WSL the same day: `printf '%s' "$new" \| latch put <file> --env <env> --project <name> [--expect <sha256>\|absent]` replaces exactly one file (stdout: sha256 of the stored plaintext), refuses when the file moved since it was read, on unpushed clone work, a never-minted key or group members; a rejected push leaves the clone as the remote is. Reading without a linked dir: `latch cat <file> --env <env> --project <name>`. |
| arch-schedule | **Schedules live in the dashboard**: any action can be scheduled; runs while CT 120 is up; a missed slot is skipped and notified, never caught up. |
| arch-push-credential | **Deploy key with write access** for CT 120; the dashboard commits only under `stacks/<stack>/`, guarded by a test. |
| arch-deploy-guard | **CLI and dashboard both refuse** a deploy when the host's last `source.commit` is not in the local history; `--force` overrides. |
| arch-host, arch-admin, arch-safety, arch-tests | Approved as listed above (Klopt). |

### ui-tables (Kenny, 2026-09-28 13:12, on the skeleton's fleet page)

"onthoudt wel dat we fancy gui willen uiteindelijk, dus een tabel moet mss een
datatable zijn (standaard zonder select/multiselect per row) en kolommen moeten
aanklikbaar zijn om te sorteren (zelfs verschillende tabellen krijgen een
sorteervolgorde)". Every table in the dashboard is the kp-themes datatable:
no row selection unless a page needs it, every column sortable by click with
a declared kind (number, text, date, or a named order such as
offline,degraded,parked,running), Shift+click adds a numbered sort key
(`data-kp-sort-multi`), and each table remembers its own sort under its own
name (`data-kp-remember`). A number column holds a bare number so it sorts as
one. Measured on the fleet page 2026-09-28: a descending sort and a second key
survive a live update and a reload.

### ui-units (Kenny, 2026-09-28 16:15, after the first real login)

"values presented to a human should always be human readable as much as
possible. In this case, MB doesn't make as much sense as GB so it should be
GB". Every size, duration and count the dashboard shows is scaled to the unit
a person reads: MB below 1 GB, GB below 1 TB, TB above (`humanMb`); seconds,
minutes and hours the same way (`measuredAgo`).

### arch-exposure (decided 2026-09-28 12:44, follow-up form "Dashboard bereikbaarheid")

**Via the Cloudflare tunnel and Traefik, from home only.** `admin.kp-soft.dev`
is a Traefik route on the `web` entrypoint behind the existing Cloudflare Access
policy, like `kyu.kp-soft.dev`. The dashboard itself enforces three locks in
order: (1) a valid Cloudflare Access JWT (`Cf-Access-Jwt-Assertion`, verified
against the team's public keys; team domain and AUD in admin.toml), which closes
the Traefik host-header gap from the LAN; (2) the client address
(`Cf-Connecting-IP`, trusted only from CT 104) equals the house's public
address the host already reads from the router (on a mismatch the admin asks
the host to re-read once before refusing); (3) the chassis login with
passkeys (OpenSSL through webauthn-rs accepted by this choice). CT 120's
firewall admits inbound only from CT 104. No Caddy, no Cloudflare write token,
no OPNsense change. Accepted cost: without internet at home the dashboard is
unreachable; the CLI keeps working (S4). HYPOTHESIS measured before the login
is built: Secure cookie and passkeys work through the chain (Traefik trusts
forwarded headers from 172.16.0.0/12).

HYPOTHESIS still open: latch as a daemon subprocess in an unprivileged LXC,
measured in a throwaway container before the secrets work starts.

## feat-platform-10 · Claude drives Kenny's open dashboard step by step (decided 2026-09-28 16:34, mini-round, Kenny's own answer on the second form)

Kenny: "Jij gebruikt geen browser, maar jij bestuurt mijn browser zogezegd met
api commando's. Mijn browser reageert live op wat jij ingeeft, stap voor stap.
Dus hetzelfde effect, maar jij hebt zelf helemaal geen browser nodig."

Claude sends UI steps, one by one: go to a page, open a wizard or dialog,
type into a field, press a button, close a dialog. The steps travel over the
existing host line (`homelab ui <step>` with Claude's own scoped token), so
there is no second entry point past Cloudflare Access; the host hands them to
the dashboard's session, and the dashboard keeps one shared "Claude is
driving" state and pushes each step over SSE. Every open tab performs the step
live, visibly: the page changes, the dialog opens, the text appears letter by
letter, the button shows its press. The final press runs on the dashboard's
server side, through the same wizard definition a human click uses, so it
happens once whether zero, one or two tabs are open, and never twice. Every
wizard is described once as data, shared by the CLI and the dashboard, so a
step Claude sends and a click Kenny makes cannot drift. A toggle at the top of every page decides whether
this tab follows (Kenny, 2026-09-28 17:33: "so if I'm actually using the site
and you do something, you don't erase my progress"). Off by default: a tab
that is off never changes page, opens a dialog or loses typed input because
Claude is driving; it shows only a small "Claude is working on <stack>:
<action>" badge with a button that turns following on. On: the tab performs
every step, and Kenny's own input while Claude drives is refused with a
visible note rather than mixed in. The toggle is per tab and remembered.
Working name "Watch Claude"; the final name is **"Live view"** (Kenny,
2026-09-28).
Placed as milestone `follow`, after `edit`, because the wizards arrive in
`act` and `edit`.

## Back-from-AFK rounds (decided 2026-09-28 19:13 and 19:24, forms "Terug van AFK" and "Meer uitleg")

| Item | Kenny's answer | What it means |
|---|---|---|
| follow-name | Live view | The toggle, badge and driven-dialog button say "Live view". |
| loki-read | Read route for CT 120 only | `stacks/metrics/loki-push/nginx.conf` admits GET `/loki/api/v1/query_range` from 10.10.10.20 alone; everything else stays 403 (fix-93 kept). |
| demo-host | Only in test builds | The simulated host is behind the Cargo feature `demo-host`; `make release` builds without it. |
| dash-exec | Own answer: "wel, zonder bevestiging, we hebben genoeg security" | `exec` is in the dashboard as a command field, no typed confirmation; the host still refuses unless `exec_enabled = true`. |
| dash-host-update | release-update yes, self-update no | An "Update host" action runs release-update and reconnects; self-update stays CLI. |
| dash-template | Add | A template-build wizard (vmid, version, privileged, base). |
| dash-install-native | Own answer: "ik wil wel mijn eigen Rust programma's kunnen deployen en managen vanuit het dashboard. Wat nu in de TUI kan, moet nog altijd kunnen in ons systeem" | install-native is in the dashboard, with the host downloading and verifying the release so CT 120 needs no `gh`. Standing rule: everything the TUI can do, the dashboard can do (parity audited against the TUI). |
| dash-apply | Yes, plan first | An Apply page: the per-stack plan, one confirmation, destroying a stack whose directory is gone only after its name is typed. |
| cli-yes | With --yes once the name was typed | The copied CLI line carries `--yes` when the form's typed name matches. |
| defaults | Kept, except the two above | The other defaults listed on the form stand. |

## Live view: announce, plan and pause (decided 2026-09-29 03:46, form "Live view aankondigen")

Kenny, after the live demo: "het typen was heel goed, maar soms springt het
allemaal veel te snel van het ene naar het andere scherm zonder dat ik weet
wat er komt."

- live-announce: **announcement with countdown and highlight, plus the whole
  plan up front.** Before every step except typing, every following tab
  shows "Next: <step>" with a 3 s countdown and highlights the target
  element; the dashboard does the waiting, so the driver needs no sleeps.
  The driver can send the whole plan first (`homelab ui plan`), shown as a
  side list with the current step marked.
- live-control: **Pause, Continue and Stop** in the announcement bar. A pause
  holds the next step until Continue or Stop; the CLI hears "paused by the
  viewer" (and who paused), Stop ends the whole sequence.

Defaults approved (form "Release 3.63.1", live-defaults: Klopt, 2026-09-29
04:27): after Stop every step is refused until `homelab ui done`; a pause
lasts at most 30 min; Continue resumes the remaining countdown; a pause
holds the next step, typing included; the pauser is named by the Access
email; the plan list floats top right from 1100 px; host, dashboard and
client ship together. Released as 3.63.1 and live since 2026-09-29 05:14.

## Live view tempo and cursor (decided 2026-09-29 05:39 and 05:42)

- live-speed: **5 s** announcement (was 3 s, "soms nog wat te snel"): set as
  HOMELAB_ADMIN_LIVE_ANNOUNCE_MS=5000 in stacks/admin/admin/admin.service,
  deployed through homelab 2026-09-29 05:41.
- live-cursor: **build it.** A simulated "Claude" cursor in following tabs:
  during the countdown it glides from where it was to the step's target,
  shows a click (ring, pressed button) at 0, then the step runs; while
  typing it sits in the field. Display only: nothing about what is sent or
  the once-only press changes.

## Slow reads (decided 2026-09-29 13:50, form "Trage pagina's")

Measured 13:43: Today = doctor 27 s then fleet check 67 s, sequential, both
walking the 17 containers one by one. Always done: bounded concurrency (8),
doctor and the fleet check side by side sharing one container round.
slow-reads: **last result at once, refreshed when the page is opened**
(Kenny first chose "every 5 min", then at 13:52: "niet elke vijf minuten,
dat is overkill, doe het als ik op die pagina kom"). The dashboard server
holds the last result of Today, doctor and the fleet check with its time;
opening the page shows it at once with "measured x ago" and starts one new
run (joining a running one), whose result arrives over SSE. No timer. No browser storage is needed for Kenny's "vorige resultaten zien als
ik van tab verander": the server holds them for every tab.

## Fleet check speed (decided 2026-09-29 15:04, form "Snelheid")

Measured on v3.63.3 (15:00): `homelab check` 37.3 s = backups/seeder/stacks
10.4 s (host, restic), containers 3.5 s, Prometheus/Loki/Grafana 0.7 s,
Cloudflare 3.0 s (desktop), pinned-image existence 19.6 s (desktop, 20
pins asked one at a time in `client/src/pinexists.rs`).

- pin-check: **only concurrent** (8 at a time), no remembered answer, so
  a vanished image is always reported fresh.
- backup-read: **reuse**. The host keeps the backup state it learns from
  the nightly round and from every backup it makes itself; the check reads
  that instead of asking restic again. A backup made or removed outside
  homelab shows after the next nightly round.
- agent-project (a Rust agent in every container): **not now**. It would
  remove at most the 3.5 s container part; noted in the backlog with this
  measurement.

Correction (2026-09-29 15:14, measured read-only on pve): the first stage's
10.4 s was not restic. The check never asks restic; backup ages come from
state.json. The stage's cost was the host-memory reading, which ran
`pct config` for each of 19 containers (9.25 s). It now sums `memory:` from
`/etc/pve/lxc/*.conf` directly (same total, 38144 MB, in 2 ms). "Reuse"
was applied to the one remote listing in that stage, the Google Drive
listing of watched backups (1.7 s): the nightly round refreshes it and
records it, a check reads the record.

## Latch on the dashboard (decided 2026-09-29 17:00, form "Latch")

Kenny: "latch in ct120, latch moet als default geinstalleerd worden in de
golden images". The golden templates install the signed latch release; the
deploy guard installs it (verified) where it is missing; the dashboard's
unit sets HOMELAB_LATCH_ENV=prod and gets a latch credential limited to
latch project `stacks`, environment prod, through `latch clone`. uptime-now:
"Wachten op het dashboard": uptime is deployed from the dashboard in Live
view once latch works there.

## Notifications and Grafana (decided 2026-09-30 08:59, form "Meldingen")

- notify-routing: **only urgent at once.** Everything lands in the
  dashboard's notification centre with history. Pushed to phone, desktop
  and lights only: a service not answering for more than 5 minutes, a
  failed backup, a disk almost full or SMART errors, a failed update or
  deploy. Home Assistant's dispatcher is not touched; homelab decides at the
  source what it still publishes.
- daily-digest: **09:00, only when something waits**, with the worst first
  and a link to the dashboard.
- notify-detail: **every notification says what is wrong, since when, the
  consequence and what to do** (the exact command or dashboard button),
  plus a `click_url` to the right dashboard page; the phone gets a short
  version with the link, the centre the full text.
- grafana-role (Kenny's own answer): "Ik wil liefst grafana kunnen
  vervangen door ons dashboard zodat ik maar 1 plek heb om naar te kijken,
  daarna kan grafana later weg." The dashboard gets the charts Grafana has
  (host and container CPU, memory, disk over time; disk health and SMART;
  a per-stack view). Grafana is removed only in a later, separate step
  once the dashboard covers it.
