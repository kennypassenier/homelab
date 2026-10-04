// redesign-stacks (3.71.0, Kenny approved the demo 2026-10-03:
// redesign-3.71/overview.html under the IA of FLOWS.md §1 row 3): the pure
// half of the Stacks page — every stack as one row model (state, the
// numbers, its sparkline, its last backup, what needs attention, and the
// one action that fits it best), the toolbar's search with its key:value
// tokens and "Only" chips, the cards' sort orders and the host strip on
// top. No DOM, no clock of its own, so `node --test` drives it.

import { stackState } from "./fleet.js";
import { humanDuration } from "./format.js";

/**
 * @typedef {import("./fleet.js").Fleet} Fleet
 * @typedef {import("./fleet.js").Stack} Stack
 * @typedef {{where_: string, pinned: string, latest: string,
 *   key?: string | null}} StaleImage one `/data/stale-images` row
 * @typedef {{stacks?: Record<string, number[]>, no_backup?: string[]}}
 *   Calendar the parts of `/data/backup-calendar` this page reads
 * @typedef {{step_s: number, from: number | null, points: number,
 *   host: {cpu_pct: (number | null)[], load_x100: (number | null)[]},
 *   stacks: Record<string, (number | null)[]>}} Trend `/data/fleet-trend`
 * @typedef {"same" | "changed" | "new" | "no_local_files" | "never_applied"
 *   | "not_compared" | "unknown"} DriftState
 * @typedef {{state: "ok" | "missed" | "none" | "unknown" | "nodata",
 *   at: number | null, text: string, tone: "ok" | "warn" | "bad" | ""}}
 *   Backup
 * @typedef {{key: "newer" | "noenv" | "drift" | "gone" | "broken" | "backup"
 *   | "parked",
 *   label: string, tone: "info" | "warn" | "bad" | "", title: string}} Flag
 * @typedef {{kind: "update" | "deploy" | "backup" | "logs", label: string,
 *   title: string}} RowAction
 * @typedef {{name: string, vmid: number, state: {label: string,
 *   tone: "ok" | "warn" | "bad"}, appsUp: number, appsTotal: number,
 *   restarts: number, ramUsed: number | null, ramMax: number | null,
 *   ramPct: number | null, cpuPct: number | null, spark: number[],
 *   backup: Backup, newer: number, drift: boolean, problem: boolean,
 *   flags: Flag[], action: RowAction, apps: string[]}} Row
 */

/** A night's backup older than this many hours counts as missed. */
export const MISSED_AFTER_H = 36;

/**
 * Newer image versions per stack, from `/data/stale-images` (`where_` is
 * `stack/container`).
 * @param {StaleImage[]} images
 * @returns {Map<string, number>}
 */
export function newerByStack(images) {
  /** @type {Map<string, number>} */
  const out = new Map();
  for (const x of images ?? []) {
    const stack = String(x.where_ ?? "").split("/")[0];
    if (stack) out.set(stack, (out.get(stack) ?? 0) + 1);
  }
  return out;
}

/**
 * One stack's last backup, in words, from the backup calendar: a stack that
 * keeps no data says so (never "missed"), a stack the calendar has not read
 * says "not read yet", and a newest snapshot older than `MISSED_AFTER_H`
 * hours is a missed night.
 * @param {Calendar | null} cal
 * @param {string} name
 * @param {number} now unix seconds
 * @returns {Backup}
 */
export function backupOf(cal, name, now) {
  if (!cal) return { state: "unknown", at: null, text: "reading…", tone: "" };
  if ((cal.no_backup ?? []).includes(name))
    return { state: "nodata", at: null, text: "keeps no data", tone: "" };
  const nights = cal.stacks?.[name];
  if (!nights)
    return { state: "unknown", at: null, text: "not read yet", tone: "" };
  if (nights.length === 0)
    return { state: "none", at: null, text: "no backup yet", tone: "warn" };
  const at = Math.max(...nights);
  const age = Math.max(0, now - at);
  const ago = `${humanDuration(age)} ago`;
  if (age > MISSED_AFTER_H * 3600)
    return { state: "missed", at, text: `missed · ${ago}`, tone: "bad" };
  return { state: "ok", at, text: ago, tone: "ok" };
}

/**
 * A series from the trend with its gaps closed (the last known value
 * carried forward; leading gaps dropped), for a sparkline.
 * @param {(number | null)[] | undefined} values
 * @param {number} [scale] divide every value by this
 * @returns {number[]}
 */
export function series(values, scale = 1) {
  /** @type {number[]} */
  const out = [];
  let last = null;
  for (const v of values ?? []) {
    if (v != null) last = v / scale;
    if (last != null) out.push(last);
  }
  return out;
}

/**
 * The one action a row offers, the most useful first: a stack that differs
 * from its files is deployed, one with a newer version updated, one whose
 * backup was missed backed up; otherwise its logs.
 * @param {{drift: boolean, newer: number, backup: Backup,
 *   state: {tone: string}}} r
 * @returns {RowAction}
 */
export function rowAction(r) {
  if (r.state.tone === "bad")
    return {
      kind: "logs",
      label: "Logs",
      title: "Read this stack's logs to see why it is down",
    };
  if (r.drift)
    return {
      kind: "deploy",
      label: "Deploy",
      title: "Make this stack match its files again (backed up first)",
    };
  if (r.newer > 0)
    return {
      kind: "update",
      label: "Update",
      title: "See which apps have a newer version and update them",
    };
  if (r.backup.state === "missed" || r.backup.state === "none")
    return {
      kind: "backup",
      label: "Back up",
      title: "Take a backup of this stack now",
    };
  return {
    kind: "logs",
    label: "Logs",
    title: "Read this stack's logs",
  };
}

/**
 * Every stack as the list shows it, in the host's order (vmid).
 * @param {Fleet} fleet
 * @param {{newer?: Map<string, number>, drift?: Record<string, {why?: string | null, state:
 *   DriftState}>, calendar?: Calendar | null, trend?: Trend | null,
 *   now: number}} ctx
 * @returns {Row[]}
 */
export function stackRows(fleet, ctx) {
  return fleet.stacks.map((s) => {
    const state = stackState(s);
    const newer = ctx.newer?.get(s.name) ?? 0;
    // redesign-stacks-7: the plan's own words (the server's drift answer):
    // changed is redeployed, gone from the files is destroyed, and one
    // that does not build blocks Deploy all changes. All three differ from
    // their files.
    const d = ctx.drift?.[s.name];
    const changed = d?.state === "changed";
    const gone = d?.state === "no_local_files";
    const broken = d?.state === "not_compared" && !!d?.why;
    const drift = changed || gone || broken;
    const backup = backupOf(ctx.calendar ?? null, s.name, ctx.now);
    const ramUsed = s.ram_used_mb ?? null;
    const ramMax = s.ram_max_mb ?? null;
    /** @type {Flag[]} */
    const flags = [];
    if (newer > 0)
      flags.push({
        key: "newer",
        label: newer === 1 ? "newer version" : `${newer} newer versions`,
        tone: "info",
        title: "An app of this stack has a newer version than the one it runs",
      });
    if (changed)
      flags.push({
        key: "drift",
        label: "differs from files",
        tone: "",
        title:
          "What runs differs from the stack's files: a deploy makes them match",
      });
    if (gone)
      flags.push({
        key: "gone",
        label: "will be destroyed",
        tone: "bad",
        title:
          "Its directory is gone from the files: Deploy all changes destroys it once its name is typed (backed up first)",
      });
    if (broken)
      flags.push({
        key: "broken",
        label: "does not build",
        tone: "bad",
        title: `Its files do not build, so Deploy all changes waits for it: ${d?.why ?? ""}`,
      });
    if (backup.state === "missed" || backup.state === "none")
      flags.push({
        key: "backup",
        label: backup.state === "none" ? "no backup yet" : "backup missed",
        tone: "bad",
        title: "Last night's backup did not happen: back it up now",
      });
    if (s.env_sealed === false)
      flags.push({
        key: "noenv",
        label: "no env",
        tone: "warn",
        title: "The host holds no sealed env: a deploy fails closed",
      });
    if (!s.enabled)
      flags.push({
        key: "parked",
        label: "parked",
        tone: "",
        title: "Parked: out of the nightly round and not started on boot",
      });
    const base = { drift: changed, newer, backup, state };
    return {
      name: s.name,
      vmid: s.vmid,
      state,
      appsUp: s.apps_running,
      appsTotal: s.apps_total,
      restarts: s.restarts ?? 0,
      ramUsed,
      ramMax,
      ramPct:
        ramUsed != null && ramMax ? Math.round((ramUsed / ramMax) * 100) : null,
      cpuPct: s.cpu_permille == null ? null : s.cpu_permille / 10,
      spark: series(ctx.trend?.stacks?.[s.name], 10),
      backup,
      newer,
      drift,
      // redesign-final-gen-e: a red chip on the card is a problem, so
      // "Problems N" always agrees with the chips it stands beside.
      problem:
        state.tone === "bad" ||
        state.label === "degraded" ||
        backup.state === "missed" ||
        backup.state === "none" ||
        flags.some((f) => f.tone === "bad"),
      flags,
      action: rowAction(base),
      apps: (s.apps ?? []).map((a) => a.name),
    };
  });
}

/** The "Only" chips of the toolbar, in the demo's order. */
export const ONLY = /** @type {const} */ ([
  {
    value: "newer",
    label: "Newer version",
    hint: "Only stacks with an app that has a newer version; click again to show all",
  },
  {
    value: "problems",
    label: "Problems",
    hint: "Only stacks that are down, degraded, missed a backup or carry a red flag; click again to show all",
  },
  {
    value: "drift",
    label: "Differs from files",
    hint: "Only stacks whose running state differs from their files; click again to show all",
  },
]);

/**
 * @param {Row} r
 * @param {string} only
 */
const isOnly = (r, only) =>
  only === "newer"
    ? r.newer > 0
    : only === "problems"
      ? r.problem
      : only === "drift"
        ? r.drift
        : true;

/**
 * How many rows each "Only" chip would show.
 * @param {Row[]} rows
 * @returns {Record<string, number>}
 */
export function onlyCounts(rows) {
  return Object.fromEntries(
    ONLY.map((o) => [o.value, rows.filter((r) => isOnly(r, o.value)).length]),
  );
}

/**
 * The search box's words: plain words match a stack's name, vmid or one
 * of its apps; `state:` and `flag:` tokens match the state word and the
 * flags (`flag:noenv`, `flag:newer`, `flag:drift`, `flag:backup`).
 * @param {string} q
 * @returns {{words: string[], state: string[], flag: string[]}}
 */
export function parseQuery(q) {
  /** @type {{words: string[], state: string[], flag: string[]}} */
  const out = { words: [], state: [], flag: [] };
  for (const t of String(q ?? "")
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)) {
    const m = /^(state|flag):(.*)$/.exec(t);
    if (m && m[1] === "state") out.state.push(m[2]);
    else if (m) out.flag.push(m[2]);
    else out.words.push(t);
  }
  return out;
}

/**
 * Whether a row stays in view: every search word matches, and when any
 * "Only" chip is on, the row is one of them (chips add up: "Newer version"
 * and "Problems" together show both kinds).
 * @param {Row} r
 * @param {ReturnType<typeof parseQuery>} q
 * @param {Set<string>} only
 */
export function rowMatches(r, q, only) {
  if (only.size > 0 && ![...only].some((o) => isOnly(r, o))) return false;
  const hay = [r.name, String(r.vmid), ...r.apps].map((x) => x.toLowerCase());
  for (const w of q.words) if (!hay.some((x) => x.includes(w))) return false;
  for (const s of q.state) if (!r.state.label.startsWith(s)) return false;
  for (const f of q.flag)
    if (!r.flags.some((x) => x.key.startsWith(f) || x.label.startsWith(f)))
      return false;
  return true;
}

/** The cards' sort orders, cycled by the Sort button (the demo's three). */
export const SORTS = /** @type {const} */ ([
  { key: "vmid", label: "vmid ↑" },
  { key: "name", label: "name ↑" },
  { key: "cpu", label: "CPU ↓" },
]);

/** @typedef {(typeof SORTS)[number]["key"]} SortKey */

/**
 * @param {Row[]} rows
 * @param {string} key
 * @returns {Row[]}
 */
export function sortRows(rows, key) {
  const out = [...rows];
  if (key === "name") out.sort((a, b) => a.name.localeCompare(b.name));
  else if (key === "cpu")
    out.sort((a, b) => (b.cpuPct ?? -1) - (a.cpuPct ?? -1) || a.vmid - b.vmid);
  else out.sort((a, b) => a.vmid - b.vmid);
  return out;
}

/**
 * The next sort order after `key`.
 * @param {string} key
 * @returns {SortKey}
 */
export function nextSort(key) {
  const i = SORTS.findIndex((s) => s.key === key);
  return SORTS[(i + 1) % SORTS.length].key;
}

/**
 * @typedef {{key: string, label: string, value: string, unit?: string,
 *   ctx: string, tone: "ok" | "warn" | "bad" | null, href: string,
 *   spark?: number[], meter?: number | null, dot?: "ok" | "warn" | "bad",
 *   title: string}} HostKpi
 */

/**
 * The strip on top of the list, the approved flows/stacks.html's five tiles
 * (redesign-final M1): stacks running, what needs you, newer versions, the
 * host's CPU and its root disk; every tile a link to its detail.
 * @param {Fleet} fleet
 * @param {Trend | null} trend
 * @param {{count: number, urgent: number}} needs
 * @param {Map<string, number>} [newer] newer versions per stack
 * @returns {HostKpi[]}
 */
export function hostStrip(fleet, trend, needs, newer = new Map()) {
  const h = fleet.host;
  const c = fleet.counts;
  const offline = fleet.stacks.filter((s) => s.enabled && !s.online).length;
  const disk = h.disk_detail ?? null;
  const apps = [...newer.values()].reduce((a, n) => a + n, 0);
  const inStacks = [...newer.values()].filter((n) => n > 0).length;
  return [
    {
      key: "online",
      label: "Stacks running",
      value: String(c.online),
      unit: `of ${c.stacks}`,
      ctx: offline
        ? `${offline} offline`
        : c.online === c.stacks
          ? "all running"
          : `${c.stacks - c.online} not running`,
      dot: offline ? "bad" : "ok",
      tone: offline ? "bad" : null,
      // Not all running: the list, only its problems; all running: the
      // Host page's containers, the detail behind the number.
      href: c.online < c.stacks ? "/stacks?only=problems" : "/host",
      title:
        c.online < c.stacks
          ? `Show the ${c.stacks - c.online} ${c.stacks - c.online === 1 ? "stack" : "stacks"} not running`
          : "Every stack runs; open the Host page for its containers",
    },
    {
      key: "needs-you",
      label: "Need you",
      value: String(needs.count),
      ctx:
        needs.count === 0
          ? "nothing waiting"
          : `${needs.urgent} urgent · open the list`,
      tone: needs.urgent ? "bad" : needs.count ? "warn" : null,
      href: "/needs-you",
      title: "Everything waiting for a person; open the Inbox",
    },
    {
      key: "newer",
      label: "Newer versions",
      value: String(apps),
      ctx: apps
        ? `in ${inStacks} ${inStacks === 1 ? "stack" : "stacks"} · review`
        : "every app is current",
      tone: null,
      href: "/update?all=1",
      title: "Apps with a newer version; review them in the Update flow",
    },
    {
      key: "cpu",
      label: "Host CPU",
      value: h.cpu_pct == null ? "—" : String(h.cpu_pct),
      unit: h.cpu_pct == null ? "" : "%",
      ctx: h.cpu_pct == null ? "not measured yet" : "",
      spark: series(trend?.host.cpu_pct),
      tone: h.cpu_pct != null && h.cpu_pct >= 90 ? "warn" : null,
      href: "/host",
      title: "The host's CPU over the last day; open the Host page",
    },
    {
      key: "disk",
      label: "Root disk",
      value: String(h.disk_pct),
      unit: "%",
      ctx: disk ? `${Math.round(disk.root_lv_size_gb)} GB` : "",
      meter: h.disk_pct,
      tone: h.disk_pct >= 90 ? "bad" : h.disk_pct >= 80 ? "warn" : null,
      href: "/host",
      title: "How full the host's root filesystem is; open the Host page",
    },
  ];
}

/**
 * "Deploy all changes" says its count before you click (FLOWS.md task 13):
 * what its plan would deploy or destroy, new stacks included (the server's
 * `counts`, the plan's own numbers); without them, the changed, gone and
 * new stacks of the comparison. null before any comparison, so the button
 * never claims "nothing to deploy" it never checked.
 * @param {{measured_at?: number | null, stacks?: Record<string,
 *   {state: DriftState, why?: string | null}>, counts?: {deploy: number, new: number,
 *   destroy: number, broken: number}} | null} drift
 * @returns {number | null}
 */
export function deployCount(drift) {
  if (!drift || drift.measured_at == null) return null;
  if (drift.counts) return drift.counts.deploy + drift.counts.destroy;
  return Object.values(drift.stacks ?? {}).filter(
    (d) =>
      d.state === "changed" ||
      d.state === "no_local_files" ||
      d.state === "new",
  ).length;
}
