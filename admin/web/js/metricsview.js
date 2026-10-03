// redesign-371-metrics (release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/metrics.html): the
// Metrics page's pure half — which chart goes where, what each KPI tile
// says, the drives table, the traffic tab's status classes and the events
// marked on every chart — so it is tested without a browser. The page
// itself (js/pages/metrics.js) only draws what these return.

import { hhmm } from "./timechart.js";
import { entryWhat } from "./activity.js";

/**
 * @typedef {import("./timechart.js").Point} Point
 * @typedef {import("./timechart.js").Series} Series
 * @typedef {import("./timechart.js").Unit} Unit
 * @typedef {import("./timechart.js").Annotation} Annotation
 * @typedef {{panel: {title: string, desc: string, unit: string,
 *   legend?: string}, series: {label: string, points: Point[]}[],
 *   error?: string}} Panel one `/data/charts` panel
 * @typedef {{key: string, panel: string, title: string, desc: string,
 *   span: 1 | 2 | 3, unit?: Unit, now?: "first" | "max" | null,
 *   threshold?: "committed" | "allowance" | number, yMax?: number,
 *   kind?: "chart" | "drives"}} Card where one chart sits and what it says
 * @typedef {{title: string, desc: string, cards: Card[]}} Section
 */

/**
 * The host's charts (no `?stack=`), grouped as the approved demo draws
 * them: Compute, then Storage and temperature with the Drives table last.
 * `{cores}` in a description is the host's own core count.
 * @type {Section[]}
 */
export const HOST_SECTIONS = [
  {
    title: "Compute",
    desc: "CPU, memory and queue length of the host itself and its containers.",
    cards: [
      {
        key: "cpu",
        panel: "CPU used",
        title: "CPU used",
        desc: "Share of all {cores} cores in use, 5-minute average.",
        span: 1,
        unit: "percent",
        now: "first",
      },
      {
        key: "mem",
        panel: "Memory used",
        title: "Memory used",
        desc: "RAM in use by the host and every guest; the dashed line is what the stacks are promised.",
        span: 1,
        unit: "bytes",
        now: "first",
        threshold: "committed",
      },
      {
        key: "load",
        panel: "Load average (5 min)",
        title: "Load average",
        desc: "Processes waiting for a core; above {cores} work queues up.",
        span: 1,
        unit: "count",
        now: "first",
      },
      {
        key: "ctmem",
        panel: "Memory per container (as Proxmox counts it)",
        title: "Memory per container",
        desc: "Each guest's RAM as Proxmox counts it.",
        span: 2,
        unit: "bytes",
      },
      {
        key: "net",
        panel: "Network in",
        title: "Network in",
        desc: "Received per second on the uplink and each container.",
        span: 1,
        unit: "rate",
        now: "first",
      },
    ],
  },
  {
    title: "Storage and temperature",
    desc: "How full the filesystems are, how warm the chips run, and drive health.",
    cards: [
      {
        key: "disk",
        panel: "Disk used per filesystem",
        title: "Disk used per filesystem",
        desc: "How full each of the host's own filesystems is.",
        span: 1,
        unit: "percent",
        yMax: 100,
        threshold: 85,
      },
      {
        key: "temp",
        panel: "Temperature (hottest sensor per chip)",
        title: "Chip temperature",
        desc: "The hottest sensor of each chip on the motherboard.",
        span: 2,
        unit: "celsius",
        now: "max",
      },
      {
        key: "drives",
        panel: "Drive health (SMART, 1 = ok)",
        title: "Drives",
        desc: "Each physical drive's SMART health and the readings that warn before it fails.",
        span: 3,
        kind: "drives",
      },
    ],
  },
];

/**
 * One stack's charts (`?stack=`): the container as a whole, then per app.
 * @type {Section[]}
 */
export const STACK_SECTIONS = [
  {
    title: "The container",
    desc: "How hard this stack's container works as a whole, against what Proxmox gave it.",
    cards: [
      {
        key: "cpu",
        panel: "CPU (whole container)",
        title: "CPU used",
        desc: "Share of this container's own cores in use, 5-minute average.",
        span: 1,
        unit: "percent",
        now: "first",
      },
      {
        key: "mem",
        panel: "Memory used (whole container)",
        title: "Memory used",
        desc: "RAM this container uses; the dashed line is what Proxmox gave it.",
        span: 1,
        unit: "bytes",
        now: "first",
        threshold: "allowance",
      },
      {
        key: "disk",
        panel: "Disk used (root filesystem)",
        title: "Disk used",
        desc: "How full this container's own disk is.",
        span: 1,
        unit: "percent",
        now: "first",
        yMax: 100,
        threshold: 85,
      },
    ],
  },
  {
    title: "Per app",
    desc: "Each app inside the container, so a busy one stands out from the rest.",
    cards: [
      {
        key: "appcpu",
        panel: "CPU per app",
        title: "CPU per app",
        desc: "Each app's share of this container's CPU allowance.",
        span: 2,
        unit: "percent",
      },
      {
        key: "appmem",
        panel: "Memory per app",
        title: "Memory per app",
        desc: "Each app's own RAM use inside this container.",
        span: 1,
        unit: "bytes",
      },
      {
        key: "appdisk",
        panel: "Disk writes per app",
        title: "Disk writes per app",
        desc: "How fast each app writes to disk, so a runaway log or database stands out.",
        span: 1,
        unit: "rate",
      },
      {
        key: "appnet",
        panel: "Network in per app",
        title: "Network in per app",
        desc: "How much network traffic each app receives.",
        span: 1,
        unit: "rate",
      },
      {
        key: "restarts",
        panel: "Restarts (last hour)",
        title: "Restarts",
        desc: "How many times each app restarted in the hour before; a healthy app reads 0.",
        span: 1,
        unit: "count",
      },
    ],
  },
];

/**
 * The panels of a `/data/charts` answer by their title.
 * @param {Panel[]} panels
 * @returns {Map<string, Panel>}
 */
export function byTitle(panels) {
  return new Map(panels.map((p) => [p.panel.title, p]));
}

/** @param {string} s @param {{cores?: number | null}} ctx */
export const fill = (s, ctx) =>
  s.replaceAll("{cores}", String(ctx.cores ?? "all"));

/**
 * The figure in a card's head: the first series' last reading, or the
 * highest last reading of all (the hottest chip).
 * @param {Panel | undefined} p
 * @param {"first" | "max" | null | undefined} how
 * @returns {number | null}
 */
export function nowValue(p, how) {
  if (!p || !how) return null;
  const lasts = p.series
    .map((s) => s.points[s.points.length - 1]?.[1])
    .filter((v) => v != null && Number.isFinite(v));
  if (lasts.length === 0) return null;
  return how === "max" ? Math.max(...lasts) : lasts[0];
}

/**
 * The highest reading of a series and when it was.
 * @param {Point[]} points
 * @returns {{at: number, value: number} | null}
 */
export function peak(points) {
  if (points.length === 0) return null;
  let best = points[0];
  for (const p of points) if (p[1] > best[1]) best = p;
  return { at: best[0], value: best[1] };
}

/** @param {Point[] | undefined} pts */
const lastOf = (pts) => (pts?.length ? pts[pts.length - 1][1] : null);

/**
 * @typedef {{name: string, ok: boolean, since: number | null,
 *   temp: number | null, tempTrend: number[], pending: number | null,
 *   pendTrend: number[], realloc: number | null, hours: number | null}}
 *   Drive one row of the Drives table
 */

/**
 * The Drives table (invariant 27: a status per drive, never lines on one
 * value): SMART health with since when it is not ok (inside the window;
 * null when it was not ok for all of it), and the readings that warn
 * before a drive fails, each with its trend.
 * @param {Map<string, Panel>} P
 * @returns {Drive[]}
 */
export function drives(P) {
  const health = P.get("Drive health (SMART, 1 = ok)")?.series ?? [];
  /** @param {string} title @param {string} name */
  const find = (title, name) =>
    P.get(title)?.series.find((s) => s.label === name)?.points;
  return health.map((s) => {
    const ok = (lastOf(s.points) ?? 1) >= 1;
    let since = null;
    if (!ok) {
      for (let i = s.points.length - 1; i > 0; i--)
        if (s.points[i - 1][1] >= 1) {
          since = s.points[i][0];
          break;
        }
    }
    const temp = find("Drive temperature", s.label);
    const pend = find("Drive pending sectors", s.label);
    return {
      name: s.label,
      ok,
      since,
      temp: lastOf(temp),
      tempTrend: (temp ?? []).map((p) => p[1]),
      pending: lastOf(pend),
      pendTrend: (pend ?? []).map((p) => p[1]),
      realloc: lastOf(find("Drive reallocated sectors", s.label)),
      hours: lastOf(find("Drive power-on hours", s.label)),
    };
  });
}

/**
 * "12,000 h · 1.4 y": a drive's powered-on hours, exact, and in years.
 * @param {number | null} h
 */
export const poweredOn = (h) =>
  h == null
    ? "not reported"
    : `${Math.round(h).toLocaleString("en-GB")} h · ${(h / 8760).toFixed(1)} y`;

/**
 * The window's start to end in words, "Fri 2 Oct, 07:46 → Sat 3 Oct, 07:46".
 * @param {number} from @param {number} to
 */
export function windowText(from, to) {
  const d = (/** @type {number} */ t) => {
    const x = new Date(t * 1000);
    return `${"Sun Mon Tue Wed Thu Fri Sat".split(" ")[x.getDay()]} ${x.getDate()} ${"Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec".split(" ")[x.getMonth()]}, ${hhmm(t)}`;
  };
  return `${d(from)} → ${d(to)}`;
}

/**
 * What the host did in the window, as event markers on every chart
 * (DESIGN_LANGUAGE §9.3): each operation and nightly round from
 * `/data/history`. A failure is red, an update amber, the rest blue; on a
 * stack's charts only what named that stack. Each opens Activity.
 * @param {import("./activity.js").Entry[]} entries
 * @param {{from: number, to: number, stack?: string}} w
 * @returns {Annotation[]}
 */
export function annotations(entries, w) {
  /** @type {Annotation[]} */
  const out = [];
  for (const e of entries) {
    if (!e || e.start < w.from || e.start > w.to) continue;
    if (e.kind === "phase") {
      if (w.stack) continue;
      out.push({
        at: e.start,
        label: `Nightly round: ${entryWhat(e)}`,
        tone: "info",
        href: "/activity",
      });
      continue;
    }
    if (e.kind !== "op") continue;
    const what = entryWhat(e);
    if (w.stack && !` ${what} `.includes(` ${w.stack} `)) continue;
    const by = e.by ? ` (${e.by})` : "";
    if (!e.ok && e.end >= e.start && !e.deferred) {
      out.push({
        at: e.start,
        label: `${what} failed${e.error ? ` (${e.error})` : ""}`,
        tone: "bad",
        href: "/activity",
      });
      continue;
    }
    out.push({
      at: e.start,
      label: `${what}${by}`,
      tone: /^(update|pin|release)/.test(e.label) ? "warn" : "info",
      href: "/activity",
    });
  }
  return out.sort((a, b) => a.at - b.at);
}

// ---------- Traffic ----------

/** The four status classes, in the order and colours the demo draws. */
export const CLASSES = /** @type {const} */ ([
  { label: "2xx", colour: "var(--success-foreground)" },
  { label: "3xx", colour: "var(--info-foreground)" },
  { label: "4xx", colour: "var(--warning-foreground)" },
  { label: "5xx", colour: "var(--destructive)" },
]);

/**
 * Per-status series (200, 304, 404, …) folded into the four classes, point
 * by point, on the first series' timestamps.
 * @param {{label: string, points: Point[]}[]} series
 * @returns {(Series & {n: number})[]}
 */
export function statusClasses(series) {
  const base = series.find((s) => s.points.length)?.points ?? [];
  return CLASSES.map((c) => {
    const mine = series.filter((s) => s.label.startsWith(c.label[0]));
    /** @type {Point[]} */
    const points = base.map((p, i) => [
      p[0],
      mine.reduce((a, s) => a + (s.points[i]?.[1] ?? 0), 0),
    ]);
    const n = Math.round(points.reduce((a, p) => a + p[1], 0));
    return { label: c.label, colour: c.colour, points, n };
  });
}

/**
 * @typedef {{status: string, host: string, path: string, n: number}}
 *   ErrorRow one of the biggest error answers
 */

/**
 * The Traffic tab's figures: totals, each class's share, the change on the
 * window before, the busiest 4xx and the time of the worst 5xx interval.
 * @param {{classes: (Series & {n: number})[], prevTotal?: number | null,
 *   errors?: ErrorRow[]}} t
 */
export function trafficFigures(t) {
  const total = t.classes.reduce((a, c) => a + c.n, 0);
  const share = (/** @type {string} */ l) =>
    total ? (t.classes.find((c) => c.label === l)?.n ?? 0) / total : 0;
  const change =
    t.prevTotal != null && t.prevTotal > 0
      ? Math.round(((total - t.prevTotal) / t.prevTotal) * 100)
      : null;
  const fives = t.classes.find((c) => c.label === "5xx");
  const worst = fives ? peak(fives.points) : null;
  const fourxx = (t.errors ?? []).filter((e) => e.status.startsWith("4"));
  return {
    total,
    shares: Object.fromEntries(t.classes.map((c) => [c.label, share(c.label)])),
    counts: Object.fromEntries(t.classes.map((c) => [c.label, c.n])),
    change,
    spikeAt: worst && worst.value > 0 ? worst.at : null,
    top4xx: fourxx[0] ?? null,
  };
}

/**
 * "+8% on the day before", for the window's own length.
 * @param {number | null} change
 * @param {string} range
 */
export function changeText(change, range) {
  if (change == null) return "no earlier window to compare with";
  const before =
    {
      "1h": "the hour before",
      "6h": "the 6 hours before",
      "24h": "the day before",
      "7d": "the week before",
      "30d": "the 30 days before",
    }[range] ?? "the window before";
  return `${change > 0 ? "+" : change < 0 ? "−" : "±"}${Math.abs(change)}% on ${before}`;
}

/**
 * A number of requests, exact (rule: counters are never abbreviated).
 * @param {number} n
 */
export const count = (n) => Math.round(n).toLocaleString("en-GB");

/**
 * The Requests per hostname chart's sources, each carrying the total the
 * Hostnames table shows for it (`rows`, the whole-window counts), so the
 * legend and the table never disagree; a hostname the table does not list
 * (beyond its top 20) sums its own points.
 * @param {{label: string, points: [number, number][]}[]} series
 * @param {[string, number][]} rows
 * @returns {{label: string, points: [number, number][], n: number}[]}
 */
export function hostLegend(series, rows) {
  const table = new Map(rows.map(([name, n]) => [name || "no hostname", n]));
  return series.map((s) => {
    const label = s.label || "no hostname";
    const n = table.get(label);
    return {
      label,
      points: s.points,
      n: Math.round(n ?? s.points.reduce((a, p) => a + p[1], 0)),
    };
  });
}
