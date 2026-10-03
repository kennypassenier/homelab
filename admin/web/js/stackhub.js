// redesign-stackhub (3.71.0, Kenny approved the hub demo 2026-10-03:
// ~/.local/share/homelab/redesign-3.71/flows/stack-hub.html with stack.html,
// FLOWS.md §1.3): what one stack's hub shows, worked out without a DOM —
// the header's chips and its More menu, the stack's slice of "needs you",
// the KPI tiles, "Is it healthy?", the history with who started each
// operation, the log filters, the Apps and Backups rows, and the Settings'
// size and network facts. Pure, so the node tests pin the shapes the page
// draws (test/stackhub.test.js).

import { historyRows } from "./activity.js";
import { cellState, nightKey, nightRange, nightsNow } from "./backupsview.js";
import { humanMb, stackState } from "./fleet.js";
import { canonicalLevel } from "./logs.js";
import { majorJump } from "./staleimages.js";

/**
 * @typedef {import("./fleet.js").Stack} Stack
 * @typedef {{state: string, label: string, why?: string | null,
 *   measured_at?: number | null}} Drift
 * @typedef {"ok" | "warn" | "bad" | "info" | "unknown"} Tone
 * @typedef {{label: string, tone?: "warn" | "bad" | "info" | null,
 *   mono?: boolean, title?: string}} Chip
 * @typedef {{network?: {ip?: string} | null,
 *   resources?: {memory_mb?: number, cores?: number, disk_gb?: number} | null,
 *   firewall?: {enabled?: boolean, rules?: unknown[]} | null,
 *   retention?: {daily?: number, weekly?: number, monthly?: number} | null,
 *   apps?: string[], native_only?: boolean} | null} Manifest
 */

/** A stack the fleet does not list (committed, not deployed yet). */
export const NOT_DEPLOYED = /** @type {const} */ ({
  label: "not deployed",
  tone: "warn",
});

/**
 * The state beside the title: a dot and a word.
 * @param {Stack | null | undefined} s
 * @returns {{label: string, tone: "ok" | "warn" | "bad"}}
 */
export const hubState = (s) => (s ? stackState(s) : NOT_DEPLOYED);

/** @param {string | null | undefined} ip "10.10.10.4/24" → "10.10.10.4" */
export const bareIp = (ip) => (ip ? String(ip).split("/")[0] : "");

/**
 * The chips under the title (the demo's meta row): the container's number
 * and address, how many apps, and every flag that changes what an action
 * does (parked, files changed, no sealed env).
 * @param {{s: Stack | null | undefined, manifest?: Manifest,
 *   drift?: Drift | null}} x
 * @returns {Chip[]}
 */
export function headerChips(x) {
  /** @type {Chip[]} */
  const out = [];
  const s = x.s;
  if (s) out.push({ label: `vmid ${s.vmid}`, mono: true });
  const ip = bareIp(x.manifest?.network?.ip);
  if (ip) out.push({ label: ip, mono: true, title: "its address" });
  const apps = s ? s.apps_total : (x.manifest?.apps?.length ?? null);
  if (apps != null)
    out.push({ label: `${apps} ${apps === 1 ? "app" : "apps"}` });
  if (s?.native) out.push({ label: "native service" });
  if (s && !s.enabled)
    out.push({
      label: "parked",
      tone: "warn",
      title: "no nightly backup or update, and not started on boot",
    });
  if (x.drift?.state === "changed")
    out.push({
      label: "files changed",
      tone: "warn",
      title: "its files differ from what the host applied; a deploy is due",
    });
  if (s?.env_sealed === false)
    out.push({
      label: "no env",
      tone: "warn",
      title:
        "the host holds no sealed env for this stack: the next deploy fails closed",
    });
  return out;
}

/**
 * The three actions in the header's fixed slots (FLOWS.md §1.3: Back up ·
 * Update · Deploy, the safe order), as catalog actions: a native service
 * backs up and updates the native way.
 * @param {boolean | null | undefined} native
 */
export const headerActions = (native) => ({
  backup: native === true ? "backup-native" : "backup",
  update: native === true ? "update-native" : "update",
  deploy: "deploy",
});

/**
 * One item of the header's More menu.
 * @typedef {{key: string, label: string, hint: string, danger?: boolean} &
 *   ({kind: "action", action: string} | {kind: "rollback"} |
 *    {kind: "go", tab: string, section: string} |
 *    {kind: "href", href: string, download?: string})} MenuItem
 * @typedef {{group: string, items: MenuItem[]}} MenuGroup
 */

/**
 * The More menu (FLOWS.md §1.3): grouped Data · Change · Pause · Native
 * service · Tools · Remove. Every stack action of the catalog that is not
 * in the header's three slots is here, so nothing the old 13-button area
 * offered is lost; the destructive ones lead to Settings ▸ Danger zone,
 * each with its own confirm there.
 * @param {{stack: string, native: boolean | null | undefined,
 *   enabled: boolean | null | undefined, vmid?: number | null}} x
 * @returns {MenuGroup[]}
 */
export function moreGroups(x) {
  const enc = encodeURIComponent(x.stack);
  const native = x.native;
  /** @type {MenuGroup[]} */
  const out = [
    {
      group: "Data",
      items: [
        {
          key: "restore",
          kind: "action",
          action: native === true ? "restore-native" : "restore",
          label: "Restore…",
          hint: "An app's data back from a dated snapshot",
        },
        {
          key: "verify-restore",
          kind: "action",
          action: "verify-restore",
          label: "Verify a restore…",
          hint: "Prove a snapshot restores, without touching live data",
        },
        {
          key: "change-secret",
          kind: "go",
          tab: "settings",
          section: "secrets",
          label: "Change a secret…",
          hint: "Write one value through latch (in Settings ▸ Secrets)",
        },
      ],
    },
    {
      group: "Change",
      items: [
        {
          key: "rollback",
          kind: "rollback",
          label: "Roll back…",
          hint: "To an earlier deployed version",
        },
        {
          key: "resize",
          kind: "action",
          action: "resize",
          label: "Resize",
          hint: "Apply the files' memory, cores and disk",
        },
        {
          key: "guards",
          kind: "action",
          action: "guards",
          label: "Add log guards",
          hint: "Log caps, journald limits, logrotate",
        },
      ],
    },
    {
      group: "Pause",
      items: [
        x.enabled === false
          ? {
              key: "enable",
              kind: "action",
              action: "enable",
              label: "Unpark this stack",
              hint: "Back into the nightly round; starts on boot again",
            }
          : {
              key: "disable",
              kind: "action",
              action: "disable",
              label: "Park this stack",
              hint: "No nightly backup or update; no start on boot",
            },
      ],
    },
  ];
  if (native !== false) {
    /** @type {MenuItem[]} */
    const items = [];
    // An older host does not say whether the stack is native: offer the
    // native variants of the header's actions too, as before (fix-229).
    if (native == null)
      items.push(
        {
          key: "backup-native",
          kind: "action",
          action: "backup-native",
          label: "Back up (native)",
          hint: "Back up an adopted service now",
        },
        {
          key: "update-native",
          kind: "action",
          action: "update-native",
          label: "Update (native)",
          hint: "Update an adopted service its own way",
        },
        {
          key: "restore-native",
          kind: "action",
          action: "restore-native",
          label: "Restore (native)…",
          hint: "An adopted service's data back from a snapshot",
        },
      );
    items.push(
      {
        key: "release-update-native",
        kind: "action",
        action: "release-update-native",
        label: "Install newest",
        hint: "The newest release of each native service",
      },
      {
        key: "install-native",
        kind: "action",
        action: "install-native",
        label: "Install a release…",
        hint: "A chosen release of one native service",
      },
      {
        key: "rollback-native",
        kind: "action",
        action: "rollback-native",
        label: "Roll back binary…",
        hint: "Put a native service's previous binary back",
      },
      {
        key: "adopt",
        kind: "action",
        action: "adopt",
        label: "Adopt",
        hint: "Take over a hand-built container from its service.yml",
      },
    );
    out.push({ group: "Native service", items });
  }
  out.push(
    {
      group: "Tools",
      items: [
        {
          key: "export",
          kind: "href",
          href: `/data/download/export/${enc}`,
          download: `${x.stack}-bundle.yml`,
          label: "Export bundle",
          hint: "Download its files as one bundle",
        },
        {
          key: "console",
          kind: "href",
          href: `/console${x.vmid != null ? `?vmid=${x.vmid}` : ""}`,
          label: "Open the console",
          hint: "A shell inside the container",
        },
      ],
    },
    {
      group: "Remove",
      items: [
        {
          key: "danger",
          kind: "go",
          tab: "settings",
          section: "danger",
          danger: true,
          label: "Destroy, forget, wipe…",
          hint: "In Settings ▸ Danger zone, each with its own confirm",
        },
      ],
    },
  );
  return out;
}

/** The destructive actions of Settings ▸ Danger zone, in the demo's order. */
export const DANGER = /** @type {const} */ ([
  {
    action: "destroy",
    label: "Destroy…",
    hint: "Back up, then delete the container",
  },
  {
    action: "forget",
    label: "Forget…",
    hint: "Drop the record of a container already gone",
  },
  { action: "wipe", label: "Wipe…", hint: "Delete what a removed stack kept" },
  {
    action: "prune-orphans",
    label: "Prune orphans…",
    hint: "Remove files the repository dropped",
  },
]);

/**
 * An age as a KPI value and its unit, exact (never "1.5k", Kenny
 * 2026-10-03): "12" "s ago", "40" "min ago", "3" "h ago", "2" "d ago".
 * @param {number} seconds
 * @returns {{value: string, unit: string}}
 */
export function agoParts(seconds) {
  const s = Math.max(0, Math.round(seconds));
  if (s < 60) return { value: String(s), unit: "s ago" };
  if (s < 3600) return { value: String(Math.floor(s / 60)), unit: "min ago" };
  if (s < 86400) return { value: String(Math.floor(s / 3600)), unit: "h ago" };
  return { value: String(Math.floor(s / 86400)), unit: "d ago" };
}

/**
 * The stack's own backup nights, as the Backups page reads them: the
 * calendar's run times plus every kept snapshot's time.
 * @param {{calendar?: number[] | null,
 *   repos?: {snapshots?: {time: number}[]}[] | null}} x
 * @returns {number[]}
 */
export function backupTimes(x) {
  const repo = (x.repos ?? []).flatMap((r) =>
    (r.snapshots ?? []).map((s) => s.time),
  );
  return [...new Set([...(x.calendar ?? []), ...repo])].sort((a, b) => a - b);
}

/**
 * How the stack's backups stand: the newest snapshot, and what last night
 * did ("ok", "miss", "none" when it keeps no data, "unknown" while the
 * nights are not read).
 * @param {{times: number[] | null, noBackup?: boolean, now: number}} x
 * @returns {{last: number | null,
 *   night: "ok" | "miss" | "none" | "before" | "unknown"}}
 */
export function backupStanding(x) {
  if (x.noBackup) return { last: null, night: "none" };
  if (!x.times) return { last: null, night: "unknown" };
  const last = x.times.length ? Math.max(...x.times) : null;
  const st = cellState(
    { status: "ok", noBackup: false, times: x.times, native: false, repos: [] },
    nightsNow(x.now).last,
    nightsNow(x.now),
  );
  return {
    last,
    night: st === "ok" ? "ok" : st === "miss" ? "miss" : "before",
  };
}

/**
 * One stale image that belongs to this stack.
 * @typedef {{container: string, key: string | null, pinned: string,
 *   latest: string, upstream: string, released: string | null,
 *   major: boolean}} Stale
 */

/**
 * This stack's rows of `/data/stale-images` (`where_` is
 * `<stack>/<container>`).
 * @param {string} stack
 * @param {{where_: string, key?: string | null, pinned: string,
 *   latest: string, upstream?: string, released?: string | null}[]} images
 * @returns {Stale[]}
 */
export function staleFor(stack, images) {
  return (images ?? [])
    .filter((x) => String(x.where_).split("/")[0] === stack)
    .map((x) => ({
      container: String(x.where_).split("/").slice(1).join("/"),
      key: x.key ?? null,
      pinned: x.pinned,
      latest: x.latest,
      upstream: x.upstream ?? "",
      released: x.released ?? null,
      major: majorJump(x.pinned, x.latest),
    }));
}

/**
 * One problem in the stack's "needs you" band, worst first, each with its
 * fix (DESIGN_LANGUAGE §1.3).
 * @typedef {{key: string, tone: "bad" | "warn" | "info", title: string,
 *   text: string, act: {label: string, title: string} &
 *   ({kind: "action", action: string, preset?: Record<string, string>} |
 *    {kind: "pin", stale: Stale} | {kind: "go", tab: string,
 *    section?: string} | {kind: "href", href: string})}} Problem
 */

/**
 * The stack's slice of Needs you (FLOWS.md §1.3: the same rows as the
 * Inbox, filtered to this stack), plus what only the hub knows.
 * @param {{stack: string, s: Stack | null | undefined,
 *   drift?: Drift | null, night?: string, last?: number | null,
 *   stale?: Stale[], openChecks?: number, now: number,
 *   inbox?: {key: string, severity: "bad" | "warn" | "info", title: string,
 *     why: string, href: string}[]}} x
 * @returns {Problem[]}
 */
export function attentionItems(x) {
  /** @type {Problem[]} */
  const out = [];
  const s = x.s;
  const name = x.stack;
  if (!s)
    out.push({
      key: "not-deployed",
      tone: "info",
      title: `The host has no container for ${name} yet`,
      text: "Its files are in the repository; Deploy creates the container from them.",
      act: {
        kind: "action",
        action: "deploy",
        label: "Deploy…",
        title: "Create the container from its files",
      },
    });
  if (s && !s.online && s.enabled)
    out.push({
      key: "offline",
      tone: "bad",
      title: `${name} is offline`,
      text: "The host cannot reach the container. Its logs say what happened last.",
      act: {
        kind: "go",
        tab: "logs",
        label: "Read the logs",
        title: "Open this stack's logs",
      },
    });
  else if (s && s.online && s.apps_running < s.apps_total) {
    const down = (s.apps ?? []).filter((a) => !a.running).map((a) => a.name);
    out.push({
      key: "apps-down",
      tone: "bad",
      title: `${s.apps_total - s.apps_running} of ${s.apps_total} apps are not running`,
      text: down.length
        ? `Stopped: ${down.join(", ")}. Their logs say why.`
        : "Their logs say why.",
      act: {
        kind: "go",
        tab: "logs",
        label: "Read the logs",
        title: "Open this stack's logs",
      },
    });
  }
  if (x.night === "miss")
    out.push({
      key: "backup-missed",
      tone: "bad",
      title: "Last night's backup did not run",
      text:
        x.last != null
          ? `No snapshot was written last night; the newest is ${agoWords(x.now - x.last)} old.`
          : "No snapshot was written last night.",
      act: {
        kind: "action",
        action: s?.native ? "backup-native" : "backup",
        label: "Back up now…",
        title: "Take a snapshot of every app's data now",
      },
    });
  for (const st of x.stale ?? [])
    out.push({
      key: `stale:${st.container}`,
      tone: "info",
      title: `${st.container} has a newer version: ${st.pinned} → ${st.latest}`,
      text: st.major
        ? "A major release: read its notes first. Updating backs the stack up first and puts the old version back if the app does not come up healthy."
        : "Updating backs the stack up first and puts the old version back if the app does not come up healthy.",
      act: st.key
        ? {
            kind: "pin",
            stale: st,
            label: "Review the update…",
            title: `Back up ${name}, move ${st.container} to ${st.latest}, commit and deploy`,
          }
        : {
            kind: "go",
            tab: "apps",
            label: "See the apps",
            title: "It is updated with a homelab release, not from here",
          },
    });
  if (s && s.env_sealed === false)
    out.push({
      key: "no-env",
      tone: "warn",
      title: `The host holds no sealed env for ${name}`,
      text: "Running apps are fine; a rebuilt container would come up without its secrets until the host keeps a copy of each env file.",
      act: {
        kind: "action",
        action: "seal-env",
        label: "Push the env…",
        title:
          "Copy every secret file on the container that the host's vault lacks into the vault; nothing in the container changes",
      },
    });
  if (s && !s.enabled)
    out.push({
      key: "parked",
      tone: "warn",
      title: `${name} is parked`,
      text: "No nightly backup or update, and it does not start on boot.",
      act: {
        kind: "action",
        action: "enable",
        label: "Unpark…",
        title: "Back into the nightly round; starts on boot again",
      },
    });
  if (x.drift?.state === "changed")
    out.push({
      key: "drift",
      tone: "warn",
      title: "Its files changed since the last deploy",
      text: "The container does not match the repository yet; Deploy makes it match.",
      act: {
        kind: "action",
        action: "deploy",
        label: "Deploy…",
        title: "Make the container match its files",
      },
    });
  if ((x.openChecks ?? 0) > 0) {
    const n = /** @type {number} */ (x.openChecks);
    out.push({
      key: "manual-checks",
      tone: "warn",
      title: `${n} manual ${n === 1 ? "check waits" : "checks wait"} for your answer`,
      text: "Only a person can answer these; the stack is not called healthy until they are.",
      act: {
        kind: "go",
        tab: "overview",
        section: "healthy",
        label: "Answer them",
        title: "Is it healthy? lists them with Answer…",
      },
    });
  }
  const seen = new Set(out.map((p) => p.title));
  for (const i of x.inbox ?? [])
    if (!seen.has(i.title))
      out.push({
        key: `inbox:${i.key}`,
        tone: i.severity,
        title: i.title,
        text: i.why,
        act: {
          kind: "href",
          href: i.href,
          label: "Open",
          title: "Where this came from",
        },
      });
  const order = { bad: 0, warn: 1, info: 2 };
  return out.sort((a, b) => order[a.tone] - order[b.tone]);
}

/** @param {number} seconds */
function agoWords(seconds) {
  const p = agoParts(seconds);
  return `${p.value} ${p.unit.replace(/ ago$/, "")}`;
}

/**
 * @typedef {{key: string, label: string, value: string, unit?: string,
 *   ctx: string, tone?: "ok" | "warn" | "bad" | null, tab: string,
 *   href?: string, spark?: number[], title?: string}} HubKpi
 * @typedef {{total: number, hourly: number[]}} RestartsDay
 */

/** The `/data/charts` panel that counts restarts per app per hour. */
const RESTARTS_PANEL = /^restarts \(last hour\)/i;

/**
 * Restarts in the last 24 h from `/data/charts?stack=…&range=24h`: the
 * restarts panel counts each app's restarts in the hour before each point
 * (`changes(container_start_time_seconds[1h])`), 6 min apart; every 10th
 * point counted back from `to` is one whole hour, so those 24 summed over
 * the apps are the hourly counts (oldest first) and their sum the total.
 * Null when the panel is missing or Prometheus failed it.
 * @param {any} body
 * @returns {RestartsDay | null}
 */
export function restartsDay(body) {
  const p = (body?.panels ?? []).find((/** @type {any} */ x) =>
    RESTARTS_PANEL.test(x?.panel?.title ?? ""),
  );
  if (!p || p.error || typeof body?.to !== "number") return null;
  const to = body.to;
  const hourly = Array(24).fill(0);
  for (const sr of p.series ?? [])
    for (const [t, v] of sr.points ?? []) {
      if (typeof t !== "number" || typeof v !== "number") continue;
      const back = to - t;
      // A point within half a step of the hour is that hour's count.
      const h = Math.round(back / 3600);
      if (h < 0 || h > 23 || Math.abs(back - h * 3600) > 30) continue;
      hourly[23 - h] += Math.max(0, Math.round(v));
    }
  return { total: hourly.reduce((a, b) => a + b, 0), hourly };
}

/** The restarts counted since each container started (the host's
 * RestartCount): the fallback without Prometheus, and for native units.
 * @param {Stack} s */
const restartsSinceCreated = (s) =>
  s.restarts ?? (s.apps ?? []).reduce((n, a) => n + a.restarts, 0);

/**
 * The five tiles (demo): Apps up, Restarts · 24 h with its sparkline, Last
 * backup, Errors in logs · 1 h, Matches its files — each a link into what
 * shows it. `restarts24`: the 24 h reading ([`restartsDay`]); undefined
 * while it is read, null without Prometheus (or for a native stack, which
 * cAdvisor does not see) — then the counter since each container started,
 * linked to the Apps tab that lists it per app.
 * @param {{s: Stack | null | undefined, stack?: string,
 *   last: number | null, night: string,
 *   errors: number | null | undefined, drift: Drift | null | undefined,
 *   restarts24?: RestartsDay | null, now: number}} x
 * @returns {HubKpi[]}
 */
export function hubKpis(x) {
  const s = x.s;
  const total = s?.apps_total ?? 0;
  const up = s?.apps_running ?? 0;
  const day = x.restarts24 ?? null;
  const restarts = day ? day.total : s ? restartsSinceCreated(s) : null;
  const back = x.last != null ? agoParts(x.now - x.last) : null;
  const d = x.drift;
  return [
    {
      key: "apps",
      label: "Apps up",
      value: s ? String(up) : "—",
      unit: s ? `of ${total}` : "",
      ctx: !s
        ? "not deployed yet"
        : total === 0
          ? "declares no apps"
          : up === total
            ? "all running"
            : `${total - up} stopped`,
      tone: s && up < total ? "bad" : null,
      tab: "apps",
    },
    day
      ? {
          key: "restarts",
          label: "Restarts",
          value: String(day.total),
          ctx: "in the last 24 h",
          tone: day.total >= 5 ? "bad" : day.total > 0 ? "warn" : null,
          tab: "apps",
          href: `/charts?stack=${encodeURIComponent(x.stack ?? s?.name ?? "")}&range=24h`,
          spark: day.hourly,
          title:
            "How often the apps restarted in the last 24 hours, hour by hour; opens the stack's charts",
        }
      : {
          key: "restarts",
          label: "Restarts",
          value: restarts == null ? "—" : String(restarts),
          ctx:
            x.restarts24 === undefined && s && !s.native
              ? "reading the last 24 h…"
              : "since each container started",
          tone: restarts != null && restarts > 0 ? "warn" : null,
          tab: "apps",
          title:
            "How often the apps' containers restarted since they were last created (no metrics for a 24 h window); the Apps tab lists them per app",
        },
    {
      key: "backup",
      label: "Last backup",
      value: back ? back.value : "—",
      unit: back ? back.unit : "",
      ctx:
        x.night === "ok"
          ? "last night covered"
          : x.night === "miss"
            ? "last night missed"
            : x.night === "none"
              ? "keeps no data"
              : x.night === "unknown"
                ? "reading the nights…"
                : back
                  ? "no night missed yet"
                  : "never backed up",
      tone: x.night === "miss" ? "bad" : null,
      tab: "backups",
    },
    {
      key: "errors",
      label: "Errors in logs · 1 h",
      value: x.errors == null ? "—" : String(x.errors),
      ctx:
        x.errors === undefined
          ? "reading Loki…"
          : x.errors === null
            ? "Loki did not answer"
            : x.errors > 0
              ? "read them"
              : "none in the last hour",
      tone: x.errors != null && x.errors > 0 ? "warn" : null,
      tab: "logs",
    },
    {
      key: "drift",
      label: "Matches its files",
      value: !d
        ? "?"
        : d.state === "same"
          ? "yes"
          : d.state === "changed"
            ? "no"
            : "?",
      ctx: !d
        ? "not compared yet"
        : d.state === "same" || d.state === "changed"
          ? d.measured_at != null
            ? `compared ${agoWords(x.now - d.measured_at)} ago`
            : "compared"
          : d.label,
      tone: d?.state === "changed" ? "warn" : null,
      tab: "settings",
      title: d?.why ?? undefined,
    },
  ];
}

/**
 * One line of "Is it healthy?".
 * @typedef {{key: string, text: string, tone: Tone, verdict: string,
 *   check?: string}} Health
 * @typedef {{key: string, name: string, stack?: string | null,
 *   state: string, checked_at?: number | null}} WatchTarget
 */

/** An app's own health check as a tone: healthy (or a unit that is
 * active) ok, starting needs you, anything else failed.
 * @param {string} h @returns {Tone} */
const healthTone = (h) =>
  h === "healthy" || h === "active"
    ? "ok"
    : h === "starting" || h === "activating" || h === "reloading"
      ? "warn"
      : "bad";

/** @param {Tone} t */
const verdictOf = (t) =>
  t === "ok"
    ? "ok"
    : t === "warn"
      ? "needs you"
      : t === "bad"
        ? "failed"
        : t === "info"
          ? "by design"
          : "not measured";

/**
 * Every check on the stack in one list — the answer to "why is it red?"
 * (FLOWS.md §1.3): what the host measures, each app's own health check
 * (docker's healthcheck; `systemctl is-active` for a native unit), whether
 * its web addresses answer (the dashboard's minute watch, `watch`), what
 * the hub reads, and each manual check a deploy left open. No restart loop
 * counts the last 24 h (`restarts24`) — without that reading, the counter
 * since each container started.
 * @param {{s: Stack | null | undefined, night: string,
 *   diskPct: number | null | undefined, drift: Drift | null | undefined,
 *   restarts24?: RestartsDay | null, watch?: WatchTarget[] | null,
 *   manual?: {id: string, text: string, app: string,
 *     answer: {label: string, tone: "ok" | "warn" | "bad"}}[]}} x
 * @returns {Health[]}
 */
export function healthChecks(x) {
  const s = x.s;
  /** @type {Health[]} */
  const out = [];
  /**
   * @param {string} key @param {string} text @param {Tone} tone
   * @param {string} [verdict]
   */
  const add = (key, text, tone, verdict) =>
    out.push({ key, text, tone, verdict: verdict ?? verdictOf(tone) });
  if (!s) add("deployed", "The container exists on the host", "warn");
  else {
    add(
      "online",
      s.enabled ? "The container is up" : "The container is parked",
      !s.enabled ? "info" : s.online ? "ok" : "bad",
    );
    if (s.apps_total > 0)
      add(
        "apps",
        s.apps_running === s.apps_total
          ? "Every app's container is running"
          : `Every app's container is running (${s.apps_total - s.apps_running} stopped)`,
        s.apps_running === s.apps_total ? "ok" : "bad",
      );
    for (const a of s.apps ?? []) {
      if (s.native)
        add(
          `health:${a.name}`,
          `${a.name}: systemctl is-active`,
          a.health ? healthTone(a.health) : "unknown",
          a.health ?? "not measured",
        );
      else
        add(
          `health:${a.name}`,
          `${a.name}: its own health check`,
          a.health ? healthTone(a.health) : "info",
          a.health ?? "not declared",
        );
    }
    const day = x.restarts24 ?? null;
    const r = day ? day.total : restartsSinceCreated(s);
    add(
      "restarts",
      `No restart loop (${r} ${r === 1 ? "restart" : "restarts"}${day ? " in 24 h" : " since each container started"})`,
      r === 0 ? "ok" : r < 5 ? "warn" : "bad",
    );
    if (x.watch) {
      const mine = x.watch.filter((w) => w.stack === s.name);
      const down = mine.filter((w) => w.state === "down");
      const flaky = mine.filter((w) => w.state === "flaky");
      const unread = mine.filter((w) => !w.checked_at);
      const text = `Its web addresses answer (${mine.length} watched)`;
      if (!mine.length)
        add("web", "Its web addresses answer", "info", "none watched");
      else if (down.length)
        add("web", text, "bad", `${down.map((w) => w.name).join(", ")} down`);
      else if (flaky.length)
        add(
          "web",
          text,
          "warn",
          `${flaky.map((w) => w.name).join(", ")} flaky`,
        );
      else if (unread.length === mine.length) add("web", text, "unknown");
      else add("web", text, "ok");
    }
  }
  add(
    "disk",
    x.diskPct == null
      ? "Disk below 80%"
      : `Disk below 80% (${Math.round(x.diskPct)}%)`,
    x.diskPct == null
      ? "unknown"
      : x.diskPct >= 95
        ? "bad"
        : x.diskPct >= 80
          ? "warn"
          : "ok",
  );
  add(
    "backup",
    "Backed up last night",
    x.night === "ok"
      ? "ok"
      : x.night === "miss"
        ? "bad"
        : x.night === "none"
          ? "info"
          : x.night === "before"
            ? "ok"
            : "unknown",
  );
  if (s)
    add(
      "env",
      "Env sealed on the host",
      s.env_sealed === false
        ? "warn"
        : s.env_sealed === true
          ? "ok"
          : "unknown",
    );
  const d = x.drift;
  add(
    "drift",
    "Matches its files",
    d?.state === "same" ? "ok" : d?.state === "changed" ? "warn" : "unknown",
  );
  for (const m of x.manual ?? [])
    out.push({
      key: `manual:${m.id}`,
      text: `${m.app}: ${m.text}`,
      tone: m.answer.tone,
      verdict: m.answer.label === "open" ? "needs you" : m.answer.label,
      check: m.id,
    });
  return out;
}

/** Who started an operation (FLOWS.md §1.3, History: Kenny / Claude /
 * Nightly round). @typedef {"you" | "claude" | "night"} Who */

/** The person the dashboard serves, named as the demo names him. */
const PERSON = "Kenny";

/** History's "Started by" chips, in the demo's words. */
export const WHO_CHIPS = /** @type {const} */ ([
  { value: "you", label: PERSON },
  { value: "claude", label: "Claude" },
  { value: "night", label: "Nightly round" },
]);

/**
 * @param {import("./activity.js").Entry} e
 * @returns {{key: Who, label: string}}
 */
export function whoOf(e) {
  if (e.kind === "phase") return { key: "night", label: "nightly round" };
  const by = (e.by ?? "").trim();
  if (/claude/i.test(by))
    return {
      key: "claude",
      label: /live view/i.test(by) ? "Claude · Live view" : by,
    };
  if (by) return { key: "you", label: by };
  if (e.req != null) return { key: "you", label: PERSON };
  return { key: "night", label: "nightly round" };
}

/** What an operation did, in the past tense the feed reads. */
const DONE = /** @type {Record<string, string>} */ ({
  deploy: "Deployed",
  "deploy-commit": "Deployed an earlier commit",
  backup: "Backed up",
  "scheduled-backup": "Backed up",
  "backup-native": "Backed up",
  restore: "Restored",
  "restore-native": "Restored",
  "verify-restore": "Verified a restore",
  update: "Updated",
  "update-native": "Updated",
  "release-update-native": "Installed the newest release",
  "install-native": "Installed a release",
  "rollback-native": "Rolled back a binary",
  resize: "Resized",
  guards: "Added log guards",
  enable: "Unparked",
  disable: "Parked",
  destroy: "Destroyed",
  forget: "Forgot",
  wipe: "Wiped",
  "prune-orphans": "Pruned orphans",
  adopt: "Adopted",
  "change-secret": "Changed a secret",
  "reveal-secret": "Revealed a secret",
  "copy-secret": "Copied a secret",
});

/**
 * The history feed of one stack, newest first: what (past tense), who, its
 * detail, how it ended.
 * @param {import("./activity.js").Entry[]} entries already this stack's
 * @returns {{start: number, what: string, who: {key: Who, label: string},
 *   detail: string, tone: "ok" | "warn" | "bad", outcome: string,
 *   search: string}[]}
 */
export function historyFeed(entries) {
  const sorted = [...entries].sort((a, b) => b.start - a.start);
  const rows = historyRows(sorted);
  return sorted.map((e, i) => {
    const r = rows[i];
    const label = e.kind === "op" ? e.label : `nightly ${e.name}`;
    const verb =
      e.kind === "op"
        ? (DONE[e.label] ??
          e.label.charAt(0).toUpperCase() + e.label.slice(1).replace(/-/g, " "))
        : `Nightly ${e.name}`;
    const failed = r.outcome.tone === "bad";
    const what =
      failed && e.kind === "op"
        ? `${e.label.charAt(0).toUpperCase()}${e.label.slice(1).replace(/-/g, " ")} failed`
        : verb;
    const who = whoOf(e);
    return {
      start: e.start,
      what,
      who,
      detail: r.detail,
      tone: r.outcome.tone,
      outcome: r.outcome.label,
      search: `${what} ${label} ${who.label} ${r.detail}`.toLowerCase(),
    };
  });
}

/**
 * The feed rows a filter keeps: none of the "Started by" chips on means
 * all; the search matches every word, in any order.
 * @template {{who: {key: Who}, search: string}} R
 * @param {R[]} rows
 * @param {Set<string>} who
 * @param {string} q
 * @returns {R[]}
 */
export function filterFeed(rows, who, q) {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean);
  return rows.filter(
    (r) =>
      (who.size === 0 || who.has(r.who.key)) &&
      words.every((w) => r.search.includes(w)),
  );
}

/** The windows of the Logs tab (demo: 15 min · 1 h · 24 h · 7 d). */
export const LOG_WINDOWS = /** @type {const} */ ([
  { value: "900", label: "15 min" },
  { value: "3600", label: "1 h" },
  { value: "86400", label: "24 h" },
  { value: "604800", label: "7 d" },
]);

/** @param {string | null} v */
export const logWindow = (v) =>
  LOG_WINDOWS.find((w) => w.value === v)?.value ?? "900";

/** The level groups of the Logs tab's side column, in the demo's words. */
export const LEVELS = /** @type {const} */ ([
  { value: "i", label: "info" },
  { value: "w", label: "warnings" },
  { value: "e", label: "errors" },
]);

/**
 * The level group of a line: errors (error, critical), warnings, and info
 * for everything else (info, debug, trace, a line with no level).
 * @param {string} level
 * @returns {"i" | "w" | "e"}
 */
export function levelGroup(level) {
  const l = canonicalLevel(level);
  if (l === "error" || l === "critical") return "e";
  if (l === "warn") return "w";
  return "i";
}

/**
 * The sources the side column lists: the stack's apps first (in the
 * fleet's order), then every other source a line came from (the system
 * journal, a unit).
 * @param {string[]} apps
 * @param {{source: string}[]} lines
 */
export function logSources(apps, lines) {
  const extra = [
    ...new Set(
      lines.map((l) => l.source || "—").filter((s) => !apps.includes(s)),
    ),
  ].sort();
  return [...apps, ...extra];
}

/**
 * What the log view shows: the lines whose source and level group are both
 * on, newest last (a log reads downwards), with the counts per source and
 * per level over ALL lines (the side column's numbers do not move as you
 * click).
 * @template {{source: string, level: string, ts_ms: number}} L
 * @param {L[]} lines
 * @param {Set<string>} offSources
 * @param {Set<string>} offLevels
 * @returns {{shown: L[], total: number, sources: Record<string, number>,
 *   levels: Record<string, number>, errors: number}}
 */
export function logView(lines, offSources, offLevels) {
  /** @type {Record<string, number>} */
  const sources = {};
  /** @type {Record<string, number>} */
  const levels = { i: 0, w: 0, e: 0 };
  const shown = [];
  for (const l of [...lines].sort((a, b) => a.ts_ms - b.ts_ms)) {
    const src = l.source || "—";
    const g = levelGroup(l.level);
    sources[src] = (sources[src] ?? 0) + 1;
    levels[g] += 1;
    if (!offSources.has(src) && !offLevels.has(g)) shown.push(l);
  }
  return { shown, total: lines.length, sources, levels, errors: levels.e };
}

/** How many lines of an answer are errors. @param {{level: string}[]} lines */
export const errorCount = (lines) =>
  lines.filter((l) => levelGroup(l.level) === "e").length;

/**
 * The version an image reference names: its tag, without a digest
 * (`traefik:v3.5@sha256:…` → `v3.5`), "latest" when it has none.
 * @param {string | null | undefined} ref
 * @returns {string}
 */
export function imageVersion(ref) {
  if (!ref) return "";
  const noDigest = String(ref).split("@")[0];
  const slash = noDigest.lastIndexOf("/");
  const colon = noDigest.lastIndexOf(":");
  return colon > slash ? noDigest.slice(colon + 1) : "latest";
}

/**
 * One row of the Apps tab.
 * @typedef {{name: string, state: "running" | "stopped" | null,
 *   restarts: number | null, version: string, image: string,
 *   newer: Stale | null, extra: boolean}} AppRow
 */

/**
 * The Apps tab (demo: App · State · Version · Newer · Actions): every app
 * the fleet reports (or the files declare, before a deploy), the version
 * its image is pinned to, and the newer one the fleet check found; a stale
 * image that is no app of the stack (a sidecar) gets its own row.
 * @param {{s: Stack | null | undefined, manifestApps?: string[],
 *   images?: Record<string, string> | null, stale?: Stale[]}} x
 * @returns {AppRow[]}
 */
export function appRows(x) {
  const fleetApps = x.s?.apps ?? null;
  const names = fleetApps
    ? fleetApps.map((a) => a.name)
    : (x.manifestApps ?? []);
  const images = x.images ?? {};
  const stale = x.stale ?? [];
  /** @param {string} app */
  const imageOf = (app) => {
    const k = Object.keys(images).find(
      (k) => k === app || k.split("/")[0] === app || k.split("/")[1] === app,
    );
    return k ? images[k] : "";
  };
  /** @param {string} app */
  const staleOf = (app) =>
    stale.find(
      (st) => st.container === app || (st.key ?? "").split("/")[0] === app,
    ) ?? null;
  /** @type {AppRow[]} */
  const rows = names.map((name) => {
    const a = fleetApps?.find((f) => f.name === name);
    const image = imageOf(name);
    const newer = staleOf(name);
    return {
      name,
      state: a ? (a.running ? "running" : "stopped") : null,
      restarts: a ? a.restarts : null,
      version: newer?.pinned ?? imageVersion(image),
      image,
      newer,
      extra: false,
    };
  });
  for (const st of stale)
    if (!rows.some((r) => r.newer === st))
      rows.push({
        name: st.container,
        state: null,
        restarts: null,
        version: st.pinned,
        image: "",
        newer: st,
        extra: true,
      });
  return rows;
}

/**
 * One row of the Backups tab.
 * @typedef {{app: string, newest: {short_id: string, time: number} | null,
 *   kept: number, cells: {night: string,
 *   state: import("./backupsview.js").CellState}[], missed: number,
 *   error: string | null}} BackupRow
 */

/** How many nights the Backups tab's strip shows (demo: 14). */
export const STRIP_NIGHTS = 14;

/**
 * The Backups tab's rows: per repository (one per app) its newest
 * snapshot, how many are kept, and the last 14 nights as a strip — a
 * night before its oldest kept snapshot is "before" (pruned or not yet
 * made), never a missed one.
 * @param {{repos: import("./backupsview.js").Repo[], now: number}} x
 * @returns {BackupRow[]}
 */
export function backupRows(x) {
  const now = nightsNow(x.now);
  const nights = nightRange(now.last, STRIP_NIGHTS, 0);
  return (x.repos ?? []).map((r) => {
    const times = (r.snapshots ?? []).map((s) => s.time);
    if (r.newest_snapshot && !times.includes(r.newest_snapshot.time))
      times.push(r.newest_snapshot.time);
    /** @type {import("./backupsview.js").StackRead} */
    const read = {
      status: "ok",
      noBackup: false,
      times,
      native: false,
      repos: [],
    };
    const cells = nights.map((night) => ({
      night,
      state: cellState(read, night, now),
    }));
    const newest = r.newest_snapshot
      ? { short_id: r.newest_snapshot.short_id, time: r.newest_snapshot.time }
      : times.length
        ? { short_id: "", time: Math.max(...times) }
        : null;
    return {
      app: r.owner,
      newest,
      kept: r.snapshot_count ?? (r.snapshots ?? []).length,
      cells,
      missed: cells.filter((c) => c.state === "miss").length,
      error: r.error ?? null,
    };
  });
}

/** @param {number} unix */
export const nightOf = (unix) => nightKey(unix);

/**
 * Settings ▸ Size and network, from the stack's files: memory, cores, disk
 * (with how full it is, when measured), the address and the firewall.
 * @param {{manifest: Manifest | undefined, diskPct?: number | null}} x
 * @returns {{label: string, value: string, mono?: boolean}[]}
 */
export function sizeFacts(x) {
  const m = x.manifest;
  if (!m) return [];
  const r = m.resources ?? {};
  const fw = m.firewall;
  const rules = Array.isArray(fw?.rules) ? fw.rules.length : 0;
  return [
    {
      label: "Memory",
      value: r.memory_mb != null ? humanMb(r.memory_mb) : "—",
    },
    { label: "Cores", value: r.cores != null ? String(r.cores) : "—" },
    {
      label: "Disk",
      value:
        r.disk_gb != null
          ? `${r.disk_gb} GB${x.diskPct != null ? ` · ${Math.round(x.diskPct)}% used` : ""}`
          : "—",
    },
    { label: "Address", value: bareIp(m.network?.ip) || "—", mono: true },
    {
      label: "Firewall",
      value: !fw
        ? "none declared"
        : fw.enabled
          ? `on · ${rules} ${rules === 1 ? "rule" : "rules"}`
          : `off · ${rules} ${rules === 1 ? "rule" : "rules"} declared`,
    },
  ];
}

/**
 * Settings ▸ Files: the stack's files in the repository, the container's
 * own first, then one per app, then the rest.
 * @param {string[]} paths
 * @returns {{path: string, what: string}[]}
 */
export function fileList(paths) {
  /** @param {string} p */
  const what = (p) =>
    p === "lxc-compose.yml"
      ? "the container: size, network, apps"
      : p === "service.yml"
        ? "the native service"
        : p.endsWith("/docker-compose.yml")
          ? `the app ${p.split("/")[0]}`
          : p === "checks.yml" || p.endsWith("/checks.yml")
            ? "what it is judged on"
            : p.startsWith("routes/")
              ? "a gateway route"
              : "";
  /** @param {string} p */
  const rank = (p) =>
    p === "lxc-compose.yml" || p === "service.yml"
      ? 0
      : p.endsWith("/docker-compose.yml")
        ? 1
        : 2;
  return [...paths]
    .sort((a, b) => rank(a) - rank(b) || a.localeCompare(b))
    .map((path) => ({ path, what: what(path) }));
}

/** Month names as the demo writes them (en-GB says "Sept"). */
const MONTHS = [
  "Jan",
  "Feb",
  "Mar",
  "Apr",
  "May",
  "Jun",
  "Jul",
  "Aug",
  "Sep",
  "Oct",
  "Nov",
  "Dec",
];

/**
 * A moment as History's demo writes it, "30 Sep 12:14" in the browser's
 * clock (with the year when it is not this one): never the ambiguous
 * "02/10/2026".
 * @param {number} unix
 * @param {number} [now] unix seconds, for the year
 * @param {string} [timeZone]
 */
export function shortWhen(unix, now = Date.now() / 1000, timeZone) {
  const fmt = new Intl.DateTimeFormat("en-GB", {
    day: "numeric",
    month: "numeric",
    year: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone,
  });
  /** @param {number} u */
  const parts = (u) => {
    const p = fmt.formatToParts(new Date(u * 1000));
    /** @param {string} t */
    const g = (t) => p.find((x) => x.type === t)?.value ?? "";
    return {
      day: String(Number(g("day"))),
      month: MONTHS[Number(g("month")) - 1] ?? "",
      year: g("year"),
      time: `${g("hour")}:${g("minute")}`,
    };
  };
  const a = parts(unix);
  const year = a.year !== parts(now).year ? ` ${a.year}` : "";
  return `${a.day} ${a.month}${year} ${a.time}`;
}

/** What History's Incidents says with none kept. @param {string} name */
export const noIncidentsText = (name) =>
  `No incident bundles kept for ${name}.`;

/**
 * What the More menu would draw, as one string: a fleet push that changes
 * nothing in it must not redraw it (that took the keyboard focus away).
 * @param {{group: string, items: {key?: string, label: string,
 *   hint?: string, disabled?: string | null, danger?: boolean}[]}[]} groups
 */
export const menuSignature = (groups) =>
  JSON.stringify(
    groups.map((g) => [
      g.group,
      g.items.map((i) => [
        i.key ?? "",
        i.label,
        i.hint ?? "",
        i.disabled ?? "",
        !!i.danger,
      ]),
    ]),
  );
