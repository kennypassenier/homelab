# homelab-admin — realization plan (Phase 5)

Approved by Kenny on 2026-09-28 12:53 (form "Dashboard realisatieplan": all
twelve items "Klopt", start: Akkoord). Every one of the 52 features sits in
exactly one milestone. Built in this order; `assembly` is the milestone where
the program does its own job in its own place (PROCEDURE Phase 5).

## Standing constraint from the gate (Kenny, 2026-09-28)

> "Waar we zeker rekening mee moeten houden is dat we niks uit de TUI
> verwijderen voor we effectief zeker zijn dat we het daar niet meer nodig
> hebben en het via het dashboard kan, kwestie van onszelf niet te
> deadlocken."

So: **nothing leaves the TUI until the dashboard provably does the same job
live.** Concretely, a TUI screen or key is removed (feat-ops-5, in `release`)
only when (1) the dashboard equivalent has been used for real on the live
fleet, (2) the emergency path (feat-platform-8) works without CT 120, and (3)
Kenny has said so for that screen. Until then the TUI keeps compiling and
working; the `tui` cargo feature stays on by default.

## Status

| Milestone | Title | Features | Status |
|---|---|---|---|
| skeleton | Het eerste levende pad | feat-platform-5, feat-platform-6 | **done 2026-09-28** (see note) |
| host | De host leert vertellen | feat-platform-1, feat-platform-2, feat-platform-3, feat-platform-4 | **live 2026-09-28** (3.62.1 on pve, signed) |
| assembly | Het dashboard draait als eigen stack (assemblage) | feat-platform-7, feat-platform-9 | **live 2026-09-28 15:32**, Kenny's login from home and the 4G refusal still to do |
| read | Alles lezen | feat-overview-1, feat-overview-2, feat-overview-3, feat-overview-4, feat-overview-8, feat-stacks-1, feat-ops-1, feat-ops-2, feat-ops-3, feat-ops-4, feat-ops-7, feat-settings-2 | not started |
| act | Alle acties | feat-stacks-4, feat-ops-6, feat-stacks-5, feat-stacks-6, feat-stacks-7, feat-stacks-8, feat-overview-5, feat-ops-8, feat-ops-9 | not started |
| edit | Wijzigen in de browser | feat-stacks-2, feat-stacks-3, feat-firewall-1, feat-firewall-2, feat-settings-1 | not started |
| backup-secrets | Back-ups en geheimen | feat-backup-1, feat-backup-2, feat-backup-3, feat-secrets-1, feat-secrets-2 | not started |
| visuals | De visualisaties | feat-overview-6, feat-overview-7, feat-overview-10, feat-overview-11, feat-overview-12, feat-ops-10, feat-ops-11, feat-stacks-9, feat-stacks-10, feat-firewall-3 | not started |
| release | Release, updates en de TUI afbouwen | feat-platform-8, feat-ops-5, feat-overview-9 | not started |

## Milestones

### skeleton · Het eerste levende pad

De crate admin op chassis-rs 2.3.0 (webapp en live), de workspace op Rust 1.97, de client-crate zonder verplichte TUI. Eén pagina: de vloot, live uit de echte host, in de browser. Nog lokaal op WSL, nog zonder login-sloten van CT 120.

- Features: feat-platform-5 (chassis-rs-feature webapp); feat-platform-6 (chassis-rs-feature live (SSE))
- Exit: Kenny opent http://localhost:…/app op WSL en ziet de containers van de echte host, en een herstart van een container verschijnt zonder herladen.
- Why here: Dit is het dunne pad van begin tot eind: echte host, echte browser. Alles daarna vult iets dat al leeft.

### host · De host leert vertellen

De hostwijzigingen uit de architectuur: leesopdrachten naast de wachtrij, request-id en tijd op elke logregel, CurrentOp, JSON-antwoorden, antwoorden met opstart-id, tokens met bereik en de tabel per commando, de geschiedenis op de host, de nieuwe knoppen in host.toml. Plus de deploy-weigering in de CLI (arch-deploy-guard).

- Features: feat-platform-1 (De host antwoordt in JSON); feat-platform-2 (Echte status per container); feat-platform-3 (Logregels met request-id); feat-platform-4 (Tokens met een bereik)
- Exit: De CLI werkt ongewijzigd tegen de nieuwe host; een lees-token wordt geweigerd bij een deploy, met een auditregel; homelab check geeft dezelfde tekst als nu, gebouwd uit JSON.
- Why here: Alles wat het dashboard daarna toont, rust op deze antwoorden. De CLI en TUI profiteren meteen mee.

### assembly · Het dashboard draait als eigen stack (assemblage)

De stack stacks/admin op CT 120, uitgerold met homelab: admin.kp-soft.dev via tunnel en Traefik, de drie sloten (Access-handtekening, thuisadres, passkeys), de firewall van CT 120, een eigen token met bereik en een latch-credential. Hier worden ook de twee open aannames gemeten: passkeys door de hele keten, en latch in een container.

- Features: feat-platform-7 (Het dashboard als stack op CT 120); feat-platform-9 (Inloggen en alleen op het thuisnetwerk)
- Exit: Kenny opent thuis https://admin.kp-soft.dev, logt in met zijn vingerafdruk en ziet de vloot live; vanaf 4G weigert het dashboard; met CT 120 gestopt werken de CLI en de nachtronde gewoon (succescriterium uit de scope).
- Why here: Dit is de assemblagemijlpaal: het programma doet hier zijn eigen werk, op zijn eigen plek. Een plan zonder zo'n mijlpaal eindigt met losse onderdelen die nergens samen draaien.

### read · Alles lezen

Alle leespagina's: overzicht, host, stackpagina met tabbladen, activiteit en incidenten, vragen van de host beantwoorden, handmatige controles, containerlogs uit Loki, de tijdlijn, het commandopalet, sneltoetsen en diepe links, de themakeuze.

- Features: feat-overview-1 (Overzicht: Vandaag en de vloot); feat-overview-2 (Hostpagina); feat-overview-3 (Commandopalet (Ctrl K)); feat-overview-4 (Alles live, met "gemeten x geleden"); feat-overview-8 (Sneltoetsen en diepe links); feat-stacks-1 (Stackpagina met tabbladen); feat-ops-1 (Activiteit en incidenten); feat-ops-2 (Vragen van de host beantwoorden); feat-ops-3 (Handmatige controles); feat-ops-4 (Containerlogs uit Loki); feat-ops-7 (Tijdlijn van alles wat er gebeurde); feat-settings-2 (Themakeuze)
- Exit: Elke leesopdracht die CLI en TUI vandaag kennen, staat in het dashboard, afgevinkt tegen de tabel met 48 CLI-commando's (succescriterium uit de scope).
- Why here: Lezen komt vóór wijzigen: het is veilig, en het toont meteen of de host de juiste dingen vertelt.

### act · Alle acties

Alle acties per stack met voortgang per stap en verwachte duur, acties op meerdere stacks, terugdraaien, "kopieer als CLI-commando", inplannen in het dashboard, het meldingencentrum, pushmeldingen via kyu en de snooze.

- Features: feat-stacks-4 (Alle acties per stack); feat-ops-6 (Voortgang per stap, met verwachte duur); feat-stacks-5 (Acties op meerdere stacks tegelijk); feat-stacks-6 (Terugdraaien naar een vorige versie); feat-stacks-7 (Kopieer als CLI-commando); feat-stacks-8 (Acties inplannen); feat-overview-5 (Meldingen en een meldingencentrum); feat-ops-8 (Pushmelding via kyu als een lange actie klaar is); feat-ops-9 (Notification settings per stack and on one page, with a snooze for all notifications for a chosen time (Kenny's own answer on feat-ops-8))
- Exit: Een deploy van één stack vanuit de browser toont "stap 3/35" met de verwachte duur; een ingeplande back-up loopt om het gekozen uur; een gemiste afspraak geeft een melding.
- Why here: Acties na lezen: de voortgangsweergave gebruikt de request-id's en tijden uit de mijlpaal host.

### edit · Wijzigen in de browser

Stackinstellingen met plan, de firewall-editor per stack en de matrix over de vloot, nieuwe stacks met de kp-wizard, alle hostinstellingen. Met de deploy key, commits alleen onder stacks/, push vóór deploy.

- Features: feat-stacks-2 (Stackinstellingen bewerken, met plan); feat-stacks-3 (Nieuwe stack of app met de kp-wizard); feat-firewall-1 (Firewall-editor per stack); feat-firewall-2 (Firewallmatrix over de hele vloot); feat-settings-1 (Alle hostinstellingen in de browser)
- Exit: Een firewallregel toegevoegd in de browser eindigt als commit op GitHub, een deploy en een homelab check zonder afwijking, één keer van begin tot eind gemeten (succescriterium uit de scope).
- Why here: Wijzigen is het riskantste deel en komt pas als lezen en acties bewezen zijn.

### backup-secrets · Back-ups en geheimen

De back-uppagina, restore met snapshotkeuze, bladeren in een snapshot (Gewenst), een geheim tijdelijk tonen, en een geheim wijzigen zodra latch het commando voor één bestand heeft.

- Features: feat-backup-1 (Back-uppagina); feat-backup-2 (Restore met snapshotkeuze); feat-backup-3 (In een snapshot bladeren, één bestand terugzetten); feat-secrets-1 (Geheim tijdelijk tonen); feat-secrets-2 (Geheim wijzigen via latch)
- Exit: Een restore van één stack naar een gekozen snapshot slaagt vanuit de browser; tonen laat een auditregel op de host na; wijzigen van één .env laat de andere bestanden in latch ongemoeid.
- Why here: Geheimen wijzigen wacht op een latch-release; de rest van deze mijlpaal niet.

### visuals · De visualisaties

De eigen SVG-modules: grafiekjes per stack, de topologie, de back-upkalender, de capaciteitskaart, de schijfgroei met voorspelling, de tijdlijn van de nachtronde, de deployduur, de afhankelijkheden, de achterstallige images, en de bemonsterde verbindingen op de topologie.

- Features: feat-overview-6 (Grafiekjes per stack); feat-overview-7 (Topologie: wie praat met wie); feat-overview-10 (Back-upkalender); feat-overview-11 (Capaciteitskaart van pve); feat-overview-12 (Schijfgroei met voorspelling); feat-ops-10 (Tijdlijn van de nachtronde); feat-ops-11 (Deployduur door de tijd); feat-stacks-9 (Afhankelijkheden tussen stacks); feat-stacks-10 (Overzicht van achterstallige images); feat-firewall-3 (Gemeten verkeer op de topologie)
- Exit: Elke visualisatie toont echte data van de vloot in de 22 kp-themes-thema's, op telefoonbreedte leesbaar, en is getest in Playwright.
- Why here: Grafieken hebben geschiedenis nodig; tegen deze mijlpaal heeft de host die al een tijd verzameld.

### release · Release, updates en de TUI afbouwen

Gesigneerde homelab-releases, self-update van het dashboard via homelab, een echte herbouw van CT 120 met restore vóór de eerste release, het noodpad in de TUI, het afbouwen van de TUI (Gewenst) en de telefoonweergave (Gewenst).

- Features: feat-platform-8 (Noodpad: dashboard installeren op een verse Proxmox); feat-ops-5 (TUI afbouwen); feat-overview-9 (Werkt ook op de telefoon)
- Exit: CT 120 wordt vernietigd en uit back-up herbouwd en werkt daarna meteen; een nieuwe gesigneerde release rolt via homelab uit; op een verse Proxmox installeert het noodpad het dashboard.
- Why here: De proef met echte restore was een verplicht punt in fase 2; ze gebeurt vóór de eerste release, niet erna.

## Gates (item `hooks`, and dev-procedure rule 7 as amended 2026-09-28)

- Commit: fmt, clippy `-D warnings`, the fast subset of the Rust suite that
  always includes the security tests, and for `admin/web/`: tsc (checkJs,
  strict), prettier, and the admin security subset.
- Release: the full suite (Rust and admin with Playwright) runs locally;
  `make release` refuses without a green full run of the exact tree.
- CI: only cheap checks (fmt, clippy, the subset, advisories, secrets).
- The subset is measured once against what it skips (rule 7i), recorded below.

### Measured

- **Commit subset (rule 7i), 2026-09-28 12:58, WSL:** 80 test binaries, 966
  tests, about 49 s run time. `.githooks/test-subset.sh` skips three
  binaries (tui_snapshot_tests 71, remote_backend_tests 7, trace_line_tests
  4: 82 tests, about 45 s), all TUI rendering; every security suite stays
  in. The subset ran 884 tests in 12.4 s.
- **Correction, same day:** the claim above was wrong for
  remote_backend_tests: it drives the TUI's TLS handshake. The full suite
  caught a rustls provider panic there that the subset let through (fixed in
  464ac9e). It is back in the subset; only tui_snapshot_tests (71) and
  trace_line_tests (4) are skipped.
- **skeleton exit, 2026-09-28 13:05:** `homelab-admin` (debug build) on WSL
  against the live host v3.61.4: `/app/` without a session answers 303 to the
  login; after the token login, Chromium (Playwright) shows 14 stacks,
  104 gateway … 118 inbox, "pve-01 · 14/14 online", no console errors, and
  "measured x s ago" drops back under 5 s on every live `fleet` event (poll
  5 s), so the SSE path works end to end. Not yet visible: a container
  restart, because `GetState` still hard-codes `running`/`restarts`
  (feat-platform-2, milestone `host`). Found on the first run: two rustls
  crypto providers in one binary panic the first handshake; `main` installs
  aws-lc-rs explicitly.
- **host, built 2026-09-28 (commits a7b50d7 … this one):** scoped tokens
  with audit and reads beside the queue on request; real status per
  container (pvesh + docker, every 60 s); request id, time and step marks on
  log lines, CurrentOp; JSON replies for doctor, fleet check, incidents and
  manual checks; history.jsonl with History; the deploy guard in `homelab
  deploy`/`apply`; answers bound to the host's start. Full suite 994 passed,
  0 failed. Exit criterion "the CLI works unchanged against the new host" is
  met in the suite (every CLI/TUI request is byte-identical, tested);
  measuring it against the live host waits for the release (Kenny's go for
  the push and the signing).

### Rollout decisions (Kenny, form "Dashboard uitrol", 2026-09-28 13:54)

- **release-host: together with assembly.** The host milestone stays local
  until the dashboard also runs as a stack; one release then carries both.
- **sign: from the dashboard release.** That release is the first signed
  homelab release (sign-releases, Kenny's passphrase); before signing, the
  coordinator is told so Kenny can sign everything pending in one go.
- **assembly-go: go, in one go.** When assembly is ready offline, Claude runs
  the five steps without asking again, reporting each and stopping at the
  first failure: CT 120 via `homelab deploy stacks/admin`; the `[[tokens]]`
  entry for "admin" in pve's host.toml plus a host restart; a write deploy
  key on kennypassenier/homelab via `gh`; the latch environment `admin`
  filled by Claude; the admin.kp-soft.dev route in the gateway stack and its
  deploy.

### assembly, rolled out 2026-09-28 15:08-15:32 (Kenny's go of 13:54)

- Releases 3.62.0 and 3.62.1 signed by Kenny, verified (minisign + SHA256SUMS),
  host on pve and client on WSL on 3.62.1. 3.62.1 because the dashboard binary
  of 3.62.0 still expected a config file CT 120 does not have.
- Step 1, CT 120: `homelab deploy stacks/admin` after three stack-file faults
  the client refused (nothing reached pve) and two found at the first start
  (a ReadWritePaths entry that only exists after a self-update; the service
  user is uid 997, not 999). All fixed in the stack files, applied by deploy.
  Measured in the container: admin.service active, /healthz 200, every other
  route 403 without an Access token.
- **Manual step (a gap in the system):** a new native's env file had to be
  seeded into the host vault by hand (`/var/lib/homelab/secrets/admin/
  admin-config/admin.env`, 0600) because a deploy takes native secrets only
  from the vault, never from the stack's local .env. Registered as a gap to
  close in homelab (seed the vault from the deploy spec on a first install).
- Step 2: `[[tokens]] name = "admin", scope = "all"` in pve's host.toml
  (backup host.toml.pre-admin-token), host restarted; CT 120 holds an
  established session to 10.10.10.250:8443, no 401 since.
- Step 3: deploy key id 164699822 (write) on kennypassenier/homelab; the
  private key is in latch only, for the `edit` milestone.
- Step 4: `latch put admin/admin/.env --env prod`: the four secrets.
- Step 5: the admin.kp-soft.dev route came with the deploy. Measured:
  https://admin.kp-soft.dev answers 302 to the Access login; a forged Host
  header straight at Traefik on CT 104 gets the dashboard's 403 (lock 1).
- The deploy's "Alloy is DROPPING admin's logs, Loki refused them" was one
  batch, once: journal entries the template carried from 2026-09-09, older
  than Loki accepts (oldest acceptable 2026-09-21). Measured 15:35: that one
  400 in Alloy's journal, none since; CT 120's own logs arrive.
