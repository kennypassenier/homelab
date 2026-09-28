# homelab-admin — features (Phase 2, FROZEN 2026-09-28)

Status: **frozen** 2026-09-28 11:59 (Kenny, freeze: Akkoord): 47 Essential, 5 Desired. Changes after this go through mini-rounds.

**Risk class: recoverable, local-first** (tech-risk-class in DECISIONS.md): a security subset at commit, the full suite locally before a release. Round 1 rated by Kenny 2026-09-28 11:33 (26 Essential, 1 Desired). Round 2 (Claude's proposals and the four mandatory items) out next.

Kenny's notes on round 1 (verbatim intent, recorded):
- **feat-platform-8** Essential, and it is **a path in the TUI**: to work it needs only the TUI (client) and the HOST on the Proxmox host. The TUI therefore keeps this emergency path after its screens move to the dashboard.
- **feat-firewall-1** Essential, reachable both from the stack's own page and from a dedicated **Firewall** page with an overview of every rule of every stack (together with feat-firewall-2, the matrix).
- **Remark (critical):** the dashboard is to be as rich and clear as possible, "cutting edge" of modern dashboards, with quality-of-life throughout, e.g. progress bars during a deployment showing step 3/35. Components the UI needs and kp-themes lacks are requested from kp-themes.

Domains (at most eight, decided in this phase): platform, overview, stacks, firewall, backup, settings, secrets, ops.

| ID | Feature | Proposed rating | Rating |
|---|---|---|---|
| feat-platform-1 | De host antwoordt in JSON | Essential | **Essential** |
| feat-platform-2 | Echte status per container | Essential | **Essential** |
| feat-platform-3 | Logregels met request-id | Essential | **Essential** |
| feat-platform-4 | Tokens met een bereik | Essential | **Essential** |
| feat-platform-5 | chassis-rs-feature webapp | Essential | **Essential** |
| feat-platform-6 | chassis-rs-feature live (SSE) | Essential | **Essential** |
| feat-platform-7 | Het dashboard als stack op CT 120 | Essential | **Essential** |
| feat-platform-8 | Noodpad: dashboard installeren op een verse Proxmox | Essential | **Essential** |
| feat-platform-9 | Inloggen en alleen op het thuisnetwerk | Essential | **Essential** |
| feat-overview-1 | Overzicht: Vandaag en de vloot | Essential | **Essential** |
| feat-overview-2 | Hostpagina | Essential | **Essential** |
| feat-stacks-1 | Stackpagina met tabbladen | Essential | **Essential** |
| feat-stacks-2 | Stackinstellingen bewerken, met plan | Essential | **Essential** |
| feat-stacks-3 | Nieuwe stack of app met de kp-wizard | Essential | **Essential** |
| feat-stacks-4 | Alle acties per stack | Essential | **Essential** |
| feat-firewall-1 | Firewall-editor per stack | Essential | **Essential** |
| feat-firewall-2 | Firewallmatrix over de hele vloot | Desired | **Essential** |
| feat-backup-1 | Back-uppagina | Essential | **Essential** |
| feat-backup-2 | Restore met snapshotkeuze | Essential | **Essential** |
| feat-settings-1 | Alle hostinstellingen in de browser | Essential | **Essential** |
| feat-secrets-1 | Geheim tijdelijk tonen | Essential | **Essential** |
| feat-secrets-2 | Geheim wijzigen via latch | Desired | **Essential** |
| feat-ops-1 | Activiteit en incidenten | Essential | **Essential** |
| feat-ops-2 | Vragen van de host beantwoorden | Essential | **Essential** |
| feat-ops-3 | Handmatige controles | Essential | **Essential** |
| feat-ops-4 | Containerlogs uit Loki | Desired | **Essential** |
| feat-ops-5 | TUI afbouwen | Essential | **Desired** |


## Round 2 (Claude's proposals), rated 2026-09-28 11:42

| ID | Feature | Proposed rating | Rating |
|---|---|---|---|
| feat-ops-6 | Voortgang per stap, met verwachte duur | — | **Essential** |
| feat-overview-3 | Commandopalet (Ctrl K) | — | **Essential** |
| feat-overview-4 | Alles live, met "gemeten x geleden" | — | **Essential** |
| feat-overview-5 | Meldingen en een meldingencentrum | — | **Essential** |
| feat-overview-6 | Grafiekjes per stack | — | **Essential** |
| feat-overview-7 | Topologie: wie praat met wie | — | **Essential** |
| feat-stacks-5 | Acties op meerdere stacks tegelijk | — | **Essential** |
| feat-stacks-6 | Terugdraaien naar een vorige versie | — | **Essential** |
| feat-stacks-7 | Kopieer als CLI-commando | — | **Essential** |
| feat-stacks-8 | Acties inplannen | — | **Essential** |
| feat-ops-7 | Tijdlijn van alles wat er gebeurde | — | **Essential** |
| feat-overview-8 | Sneltoetsen en diepe links | — | **Essential** |
| feat-overview-9 | Werkt ook op de telefoon | — | **Desired** |
| feat-backup-3 | In een snapshot bladeren, één bestand terugzetten | — | **Desired** |
| feat-settings-2 | Themakeuze | — | **Essential** |
| feat-ops-8 | Pushmelding via kyu als een lange actie klaar is | — | **Desired** |
| feat-ops-9 | Notification settings per stack and on one page, with a snooze for all notifications for a chosen time (Kenny's own answer on feat-ops-8) | — | **Desired** |

Kenny's notes on round 2:
- **feat-overview-7** (topology) Essential; "intuitive visualisations are very welcome", more use cases asked for (next form).
- **feat-ops-8** Desired, with feat-ops-9: per-stack and global notification switches, and a snooze that silences every notification for a chosen time.


## Round 3 (visualisations), rated 2026-09-28 11:59

| ID | Feature | Proposed rating | Rating |
|---|---|---|---|
| feat-overview-10 | Back-upkalender | — | **Essential** |
| feat-overview-11 | Capaciteitskaart van pve | — | **Essential** |
| feat-overview-12 | Schijfgroei met voorspelling | — | **Essential** |
| feat-ops-10 | Tijdlijn van de nachtronde | — | **Essential** |
| feat-ops-11 | Deployduur door de tijd | — | **Essential** |
| feat-stacks-9 | Afhankelijkheden tussen stacks | — | **Essential** |
| feat-firewall-3 | Gemeten verkeer op de topologie | — | **Essential** |
| feat-stacks-10 | Overzicht van achterstallige images | — | **Essential** |

## Mandatory items (round 2), answered 2026-09-28 11:42

1. **Updates:** chassis-rs self-update is supported, **managed by homelab** the way kyu and almanac are today (signed release, homelab drives it); shipped with the homelab release.
2. **Ecosystem:** homelab, latch, kp-themes, chassis-rs and kyu (notifications through kyu).
3. **Backup:** the stack rides homelab's nightly restic regime, the working copy comes back from GitHub; **a real rebuild with restore is run before the first release** (queued measurement).
4. **Data location:** the homelab pattern with one root: everything durable under `/appdata/admin/admin-config` (one configurable root), caches outside it.

Note 2026-09-28 11:23 (relayed from the chassis-rs thread): feat-platform-5
(`webapp`) and feat-platform-6 (`live`) are built on chassis-rs main
(5d111f2), not released; release 2.3.0 waits on Kenny's form in that thread.
Measured there: kyu-runner's release binary is 10,813,496 bytes before and
after (the no-bloat success criterion holds). Until the release, the admin
crate can build against a path dependency on ~/Projects/chassis-rs.

## Added after the freeze (mini-round, 2026-09-28 16:29)

| ID | Feature | Proposed rating | Rating |
|---|---|---|---|
| feat-platform-10 | Claude's actions play out in the open dashboard: navigation, wizards, fields typed in, every "click", then the live progress | — | **Essential** |

Kenny's own answer: Claude does not literally click on the website, "maar ik wil wel alsof het zo lijkt. Als ik op de website ben en ik vraag aan claude om iets te doen in het project, dan moet ik elke stap kunnen volgen, tot het invullen van forms en alle zogenaamde kliks".
