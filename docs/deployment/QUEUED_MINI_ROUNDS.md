# Queued mini-rounds — the deployment project

Rounds that are owed to Kenny but have not been put to him yet. The
procedure's rule (PROCEDURE.md, Phase 2): when a mandatory discussion item is
added to the procedure, every project not yet at Phase 10 inherits it in its
own queue **on the day it is added** — because "it applies from now on"
quietly means "nobody owns it".

Nothing here is built before he answers. Each one becomes a form.

---

## Q1 · Data & config location (Phase 2 mandatory item 4, standing rule 28)

**Added to the procedure 2026-08-31** (`~/Projects/dev-procedure`, commit
224d155), and Kenny named THIS project as the reason. Relayed by the
dev-procedure session the same evening.

**What the item asks.** Where does the software keep its state on disk — data,
configuration, secrets, logs, caches — and can Kenny *choose* that location
rather than inherit what the author hardcoded? It separates the kinds of state
first (durable data, config, secrets, and regenerable caches, which belong
OUTSIDE the rest so a backup carries no ballast), then puts the realistic
options side by side. The hard rule: an opinionated default is fine, an
immovable location is not. One documented knob — env var, CLI flag or config
key — moves the whole tree, and every path derives from that root.

**Why it is not theoretical here.** This project already has two roots and
they were chosen for good reasons, but neither is configurable today:

| Root | Holds | Configurable? |
|---|---|---|
| `/appdata/<stack>/<app>-config` | what the applications write; bind-mounted into containers; one restic repo per app | the per-stack paths are declared in each manifest, but the `/appdata` prefix is a hardcoded validation rule |
| `/var/lib/homelab/` | the orchestrator's own state: `state.json`, the secrets vault, TLS material, the intent git repo, incidents, journal | `HOMELAB_STATE_DIR` exists, so this one already has its knob |

So the honest position going in: one root has a knob, the other does not, and
the `/appdata` prefix is enforced by `manifest.rs` rather than configured.

**And the evening's own evidence, which the item predicts exactly.** Two
findings from 2026-08-31 are this item in miniature:

- **F125** — almanac writes to `/opt/almanac/data` by absolute path and latch
  keys its project link on an absolute path. Moving the working directory was
  not enough; the migration failed live and was reverted. That is precisely
  "an immovable location".
- **kyu**, by contrast, already has `KYU_DATA_DIR` and would have moved
  without complaint. Same house, same author, opposite answers — which is the
  argument for making it a standing question rather than a per-app accident.

**What the form will have to decide**, when Kenny gets to it:
1. Does the `/appdata` prefix become configurable, or stay an enforced
   convention with its reasons written down?
2. Do the native services get a required "one knob" contract — and if so, is
   that a request to those projects (almanac, http-switchboard) or a
   bind-mount at the path each app insists on?
3. Does the caches-outside-the-root rule change anything here — e.g. Jellyfin's
   transcode cache, which today lives in a docker volume and is deliberately
   NOT backed up.

**Status:** queued, not built, not started. Waiting on Kenny.
