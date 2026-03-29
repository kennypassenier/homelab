# LLM Context - GitOps Proxmox Homelab

Dit bestand bevat de essentiële context en regels voor LLM's (zoals Claude, ChatGPT, Gemini) die assisteren bij het bouwen en onderhouden van dit project. **Lees dit bestand altijd als eerste door bij de start van een nieuwe sessie.**

## 1. Project Architectuur & Technologieën

- **Client/Werkstation:** Pop!\_OS. Alle lokale scripts en Git acties worden vanaf deze desktop uitgevoerd. Ga er standaard van uit dat de terminal hierop draait.
- **Host:** Proxmox VE (draait onbevoorrechte LXC containers).
- **Containers:** Docker & Docker Compose draaien _binnenin_ de LXC containers.
- **GitOps Flow:** Elke applicatie/stack heeft een configuratie in `apps/<stack_name>/<app_name>`. Binnen de LXC draait elke 5 minuten het `node-sync.sh` script (via cron) om wijzigingen via Git Pull & Git Sparse Checkouts binnen te halen. Eventuele `pre-sync.sh` scripts in de stack map worden eerst uitgevoerd (bijv. voor het aanmaken van externe netwerken). Daarna voert het script `docker compose pull -q` en `docker compose up -d --remove-orphans` uit.
- **Secret Management:** Transparante encryptie met **SOPS en Age**. `.env` bestanden worden lokaal automatisch geëncrypt via Git smudge/clean filters en gedeëncrypt in de containers.
- **Storage:** Snelle configuratiedata (SSD) staat op de Proxmox host onder `/opt/appdata/<STACK_NAME>` en wordt via een unprivileged bind-mount gedeeld naar de LXC op `/appdata`.
- **Networking:** DHCP reserveringen (statische IP's) worden centraal beheerd in OPNsense op basis van het MAC-adres van de LXC container. Lokale DNS/SSH wordt via `~/.ssh/config` aliassen geregeld.
- **Backups:** Restic draait op de host, pauzeert tijdelijk containers met label `com.homelab.backup.pause=true` om database corruptie te voorkomen, en back-upt de host mounts.

## 2. Strikte LLM Instructies (Regels)

1. **VRAAG ALTIJD TOESTEMMING:** Voer NOOIT zomaar terminal commando's of file edits uit. Leg altijd eerst je plan uit, toon de code/commando's, en wacht op een expliciet "go" van de gebruiker.
2. **Houd documentatie up-to-date:** Telkens als we de architectuur, scripts, of CLI-flags aanpassen, MOET de `README.md` in dezelfde iteratie worden geüpdatet.
3. **Context Check:** Vergeet niet dat we niet op de Proxmox server of in een container zitten, tenzij we via een commando (zoals `ssh`) expliciet zijn ingelogd. Scripts in `/scripts/client/` zijn voor Pop!\_OS, `/scripts/host/` voor Proxmox, en `/scripts/container/` voor in de LXC.

## 3. Huidige Status

- **Uitgerolde Stacks:**
  - `monitoring`: Bevat Uptime Kuma, Grafana, Loki en Watchtower. Grafana is geconfigureerd om Loki automatisch als datasource te provisionen.
  - `paperless`: Bevat Paperless-ngx, DB, Redis, Broker, Paperless-AI (Tagger UI + RAG backend), Promtail en Watchtower.
  - `media`: Bevat Sonarr, Radarr, Prowlarr, Bazarr, Jellyfin, Promtail en Watchtower. Configuratie zit netjes in afzonderlijke apps gemount via `/appdata/media/...`.
- **Recente wijzigingen:**
  - Shell scripts (`proxmox-bootstrap-lxc.sh`, `register-local-node.sh`, `node-sync.sh`) zijn geüpgraded met CLI arguments (`getopts`) en `--help` functionaliteit voor betere automatisering.
  - `node-sync.sh` is robuuster gemaakt door een specifieke `cd` en door vooraf `docker compose pull -q` uit te voeren (om updates sneller te vangen en efficiënt te deployen zonder overbodige `--force-recreate` restarts).
  - Volledige ondersteuning en integratie van `pre-sync.sh` scripts (zoals gebruikt in de media stack voor de creatie van Docker netwerken buiten compose om).
  - Geavanceerde Watchtower lifecycle pre-checks toegevoegd (zoals `check-streams.sh` voor Jellyfin) die updates annuleren als er streams actief zijn om zo downtime tijdens gebruik te voorkomen.
  - **Host Management:** Idempotente host-scripts toegevoegd (`host-sync.sh`, `setup-host-cron.sh`) om de Proxmox host periodiek te updaten via Git, plus een script (`proxmox-enable-gpu-passthrough.sh`) voor gecontroleerde, veilige hardware-acceleratie per LXC (zonder globale gaten te prikken).
