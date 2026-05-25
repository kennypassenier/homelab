# Master Architectuur: Homelab GitOps (Rust Refactor)

## 1. De 3-Tier Architectuur
Het homelab hanteert een strikte scheiding van rechten en verantwoordelijkheden. Code uit één laag voert nooit direct opdrachten uit in een andere laag:
*   **Tier 1: CLIENT:** Een lokaal gecompileerde Rust TUI. Bevat scaffolding, pre-flight linting en de "Blast Radius" beveiliging. Stuurt Git commits naar GitHub en POST-triggers naar Tier 3.
*   **Tier 2: HOST:** Een gecompileerde Rust daemon (binary via GitHub Releases) op Proxmox. Provisioneert LXC's via API, beheert NVMe-mounts, installeert eenmalig het OS via een Exec Hook, en pauzeert/resumeert back-ups via HTTP calls naar Tier 3.
*   **Tier 3: LXC:** Een zelf-updatende Rust Docker-container. Trekt Git wijzigingen binnen (Sparse Checkout per stack), beheert de lokale Docker-socket (Bollard), en streamt logs (SSE) terug naar de CLIENT.

## 2. Core Principles & Beveiliging
*   **Git = God:** Enige bron van waarheid. Aanpassingen gebeuren in Git. De fallback cronjob in de LXC (30 min) en de HTTP Push API zorgen voor de synchronisatie.
*   **Fail-Closed Secrets:** Geen zware Infisical daemon meer in RAM. De LXC daemon lanceert kortstondig een `SECRETS` Docker-container. Faalt deze container bij het opvragen van geheimen? Dan stopt de volledige atomaire deploy-transactie direct.
*   **Atomaire Directory Transacties:** Ter preventie van dataverlies controleert de LXC daemon of `/root/<stack>/config` een ander `st_dev` ID heeft dan `/root/<stack>/docker`. Is dit gelijk? Dan faalt de schrijfkoppeling (bind-mount) en start er géén container.
*   **Blast Radius Protectie:** Data-verwijdering op de host (Garbage Collection) vindt alleen plaats als de app uit Git ontbreekt én de HTTP API call de `force_deletion=true` token bevat (gegenereerd na dubbele waarschuwing in de TUI).

## 3. Externe Integraties & CI/CD
*   **Traefik in plaats van NPM:** Nginx Proxy Manager is vervangen. Proxying en Let's Encrypt verlopen via dynamische Docker-labels uitgelezen door Traefik. Let op: het `acme.json` bestand moet persistenteren op een host-mount om API rate limits te voorkomen.
*   **Watchtower & Promtail:** Blijven ongewijzigd wegens hun efficiëntie. Watchtower respecteert labels en pre-update hooks. Promtail leest lokale Docker JSON-bestanden.
*   **CI/CD Pipeline (GitHub Actions):** 
    *   De `HOST` daemon compileert op GitHub en wordt gedistribueerd via GitHub Releases.
    *   De `LXC` daemon compileert op GitHub en pusht een image naar de GitHub Container Registry (GHCR). Watchtower in de stacks detecteert dit en updatet de LXC daemon volautomatisch.
*   **Externe Notificaties:** De HOST of LXC daemon kan Webhooks (bijv. Discord of Ntfy) sturen bij kritieke fouten of rollbacks tijdens de 30-minuten cron fallback, voor alerts als je niet actief achter de TUI zit.
*   **Verwijderde Legacy:** Oude eenmalige migratiescripts (zoals de bash migratie van Jellyseerr naar Seerr) zijn permanent uit de codebase gewist.

## 4. Q&A & Edge-Case Beslissingslogboek (Historische Context)
Tijdens de architectuursessie zijn de volgende specifieke edge-cases en vragen beantwoord en besloten. Deze regels overrulen oude bash-scripts:

*   **Vraag/Punt:** Wat doen we met de eenmalige bash-migratie logica van "Jellyseerr naar Seerr" uit de oude `pre-sync.sh`?
    *   **Besluit:** Volledig VERWIJDERD. Dit is oude legacy. De nieuwe codebase moet 100% schoon zijn en bevat geen eenmalige migratiescripts meer.
*   **Vraag/Punt:** Hoe overleven we het verwijderen van de zware Infisical daemon voor secret management (feature 23)?
    *   **Besluit:** Vervangen door een "Ephemeral SECRETS Docker Container". De LXC Daemon start via de Docker API kortstondig een eigen SECRETS-image, mount de configuratiemap, haalt de variabelen op, schrijft de `.env`, en sluit direct af. Fail-Closed: als dit faalt, start de hoofdapplicatie niet.
*   **Vraag/Punt:** Hoe worden OS-updates (`unattended-upgrades`) en Docker-installaties nu echt afgehandeld op een nieuwe LXC, zonder dat het te traag wordt?
    *   **Besluit:** Via een **Post-Provisioning Exec Hook**. De HOST Daemon kloont eerst razendsnel een Debian Proxmox-template (met het juiste MAC-adres). Direct daarna gebruikt de HOST de Proxmox Exec API om atomair de commando's `apt-get update && apt-get upgrade` én de installatie van `unattended-upgrades` + Docker uit te voeren. Faalt dit apt-commando? Dan wist de HOST de LXC direct weer (Fail-Safe).
*   **Vraag/Punt:** Hoe werkt de L7 Intrusion Prevention (CrowdSec) nu de Nginx Proxy Manager (NPM) vervangen is?
    *   **Besluit:** De oude NPM-logica vervalt. We gebruiken nu Traefik als gateway. CrowdSec wordt in de gateway-stack geïntegreerd via de officiële Traefik Bouncer middleware. Dit is een pure GitOps (Docker labels) aanpassing in de configuratie; de LXC Daemon hoeft hier geen extra acties voor uit te voeren, behalve de standaard deployment.
