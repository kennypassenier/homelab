# homelab-admin — features (Phase 2, DRAFT)

Status: round 1 rated by Kenny 2026-09-28 11:33 (26 Essential, 1 Desired). Round 2 (Claude's proposals and the four mandatory items) out next.

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

Note 2026-09-28 11:23 (relayed from the chassis-rs thread): feat-platform-5
(`webapp`) and feat-platform-6 (`live`) are built on chassis-rs main
(5d111f2), not released; release 2.3.0 waits on Kenny's form in that thread.
Measured there: kyu-runner's release binary is 10,813,496 bytes before and
after (the no-bloat success criterion holds). Until the release, the admin
crate can build against a path dependency on ~/Projects/chassis-rs.
