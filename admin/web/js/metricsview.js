// redesign-371-metrics (release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/metrics.html): the
// Metrics page's pure half — which chart goes where, what each KPI tile
// says, the drives table, the traffic tab's status classes and the events
// marked on every chart — so it is tested without a browser. The page
// itself (js/pages/metrics.js) only draws what these return.

import { formatDateTime } from "./format.js";
import { entryWhat } from "./activity.js";

/**
 * @typedef {import("./charts.js").Point} Point
 * @typedef {import("./charts.js").Series} Series
 * @typedef {import("./charts.js").Unit} Unit
 * @typedef {import("./charts.js").Annotation} Annotation
 * @typedef {{panel: {title: string, desc: string, unit: string,
 *   legend?: string}, series: {label: string, points: Point[]}[],
 *   error?: string}} Panel one `/data/charts` panel
 * @typedef {{key: string, panel: string, title: string, desc: string,
 *   span: 1 | 2 | 3, unit?: Unit, now?: "first" | "max" | null,
 *   threshold?: "committed" | "allowance" | number, yMax?: number,
 *   typical?: boolean, sources?: number,
 *   kind?: "chart" | "drives"}} Card where one chart sits
 *   and what it says; `typical`: it also draws its typical level and rings
 *   the runs (kp's `median` and `runs`)
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
    desc: "CPU, memory, swap, queue length and network of the host itself and its containers.",
    cards: [
      {
        key: "cpu",
        panel: "CPU used",
        title: "CPU used",
        desc: "Share of all {cores} cores in use, 5-minute average.",
        span: 1,
        unit: "percent",
        now: "first",
        typical: true,
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
        typical: true,
      },
      {
        key: "ctmem",
        sources: 6,
        panel: "Memory per container (as Proxmox counts it)",
        title: "Memory per container",
        desc: "Each guest's RAM as Proxmox counts it.",
        span: 2,
        unit: "bytes",
        typical: true,
      },
      // fix-371-1: the Host page's Swap tile lands here.
      {
        key: "swap",
        panel: "Swap used",
        title: "Swap used",
        desc: "Share of the host's swap space in use; a host without swap reads 0.",
        span: 1,
        unit: "percent",
        now: "first",
        yMax: 100,
      },
      {
        key: "net",
        sources: 3,
        panel: "Network in",
        title: "Network in",
        desc: "Received per second on the uplink and each container.",
        span: 1,
        unit: "rate",
        now: "first",
      },
      // fix-371-1: the Host page's Network tile (in + out on the bridges)
      // lands here, both directions drawn.
      {
        key: "netio",
        sources: 2,
        panel: "Network in and out",
        title: "Network in and out",
        desc: "Received and sent per second on the host's Proxmox bridges.",
        span: 2,
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
        sources: 3,
        panel: "Disk used per filesystem",
        title: "Disk used per filesystem",
        desc: "How full each of the host's own filesystems is.",
        span: 1,
        unit: "percent",
        yMax: 100,
        threshold: 85,
      },
      // fix-371-1: the Host page's Disk traffic tile lands here.
      {
        key: "diskio",
        sources: 2,
        panel: "Disk traffic",
        title: "Disk traffic",
        desc: "Bytes the host's disks read and write per second.",
        span: 1,
        unit: "rate",
        now: "first",
      },
      {
        key: "temp",
        sources: 2,
        panel: "Temperature (hottest sensor per chip)",
        title: "Chip temperature",
        desc: "The hottest sensor of each chip on the motherboard.",
        span: 1,
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
        typical: true,
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
 *   temp: number | null, tempPoints: Point[], pending: number | null,
 *   pendPoints: Point[], realloc: number | null, hours: number | null}}
 *   Drive one row of the Drives table; the two `…Points` are the readings
 *   over the window, the two lines in a drive's detail
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
      tempPoints: temp ?? [],
      pending: lastOf(pend),
      pendPoints: pend ?? [],
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
  return `${formatDateTime(from)} → ${formatDateTime(to)}`;
}

/** A stretch of work at least this long (seconds) is drawn as a window. */
const WINDOW_MIN_S = 60;

/**
 * What the host did in the window, as event markers on every chart
 * (DESIGN_LANGUAGE §9.3): each operation and nightly round from
 * `/data/history`. The kind says what it was (an operation, an update, a
 * failure: kp's key names the three); work that took a minute or more
 * carries its `until`, so it is drawn as a window, while a failure stays a
 * line. On a stack's charts only what named that stack. Each opens Activity.
 * @param {import("./activity.js").Entry[]} entries
 * @param {{from: number, to: number, stack?: string}} w
 * @returns {Annotation[]}
 */
export function annotations(entries, w) {
  /** @type {Annotation[]} */
  const out = [];
  /** @param {number} start @param {number} end @returns {{until?: number}} */
  const lasting = (start, end) =>
    end - start >= WINDOW_MIN_S ? { until: end } : {};
  for (const e of entries) {
    if (!e || e.start < w.from || e.start > w.to) continue;
    if (e.kind === "phase") {
      if (w.stack) continue;
      out.push({
        at: e.start,
        ...lasting(e.start, e.end),
        label: `Nightly round: ${entryWhat(e)}`,
        tone: "info",
        kind: "operation",
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
        kind: "failure",
        href: "/activity",
      });
      continue;
    }
    const update = /^(update|pin|release)/.test(e.label);
    out.push({
      at: e.start,
      ...(e.end > 0 ? lasting(e.start, e.end) : {}),
      label: `${what}${by}`,
      tone: update ? "warn" : "info",
      kind: update ? "update" : "operation",
      href: "/activity",
    });
  }
  return out.sort((a, b) => a.at - b.at);
}

/** The lanes of the operations ribbon, in the order it lists them. */
export const RIBBON_LANES = [
  { id: "rounds", label: "Nightly round" },
  { id: "updates", label: "Updates" },
  { id: "host", label: "Host work" },
];

/**
 * The same work as bars in lanes (kp's `kp-gantt`): the nightly round, the
 * updates and the rest of the host's work, a failed operation as a failed
 * bar. A run still going has no end, and an instant gets two minutes so it
 * can be seen and pressed. Times in milliseconds (kp's unit). A stack's
 * ribbon holds what named that stack, like its charts.
 * @param {import("./activity.js").Entry[]} entries
 * @param {{from: number, to: number, stack?: string}} w
 * @returns {{id: string, lane: string, label: string, start: number,
 *   end: number | null, state: "ok" | "failed" | "running",
 *   detail: string}[]}
 */
export function ribbonRuns(entries, w) {
  /** @type {ReturnType<typeof ribbonRuns>} */
  const out = [];
  for (const e of entries) {
    if (!e || e.start < w.from || e.start > w.to) continue;
    const what = entryWhat(e);
    if (e.kind === "phase") {
      if (w.stack) continue;
      out.push({
        id: `phase-${e.start}-${e.name}`,
        lane: "rounds",
        label: `Nightly round: ${what}`,
        start: e.start * 1000,
        end: Math.max(e.end, e.start + 120) * 1000,
        state: "ok",
        detail: `${formatDateTime(e.start)} to ${formatDateTime(e.end)}`,
      });
      continue;
    }
    if (e.kind !== "op") continue;
    if (w.stack && !` ${what} `.includes(` ${w.stack} `)) continue;
    const running = e.end === 0 || e.end < e.start;
    const failed = !running && !e.ok && !e.deferred;
    const update = /^(update|pin|release)/.test(e.label);
    out.push({
      id: `op-${e.start}-${e.label}-${e.subject ?? ""}`,
      lane: update ? "updates" : "host",
      label: `${what}${failed && e.error ? ` failed (${e.error})` : ""}`,
      start: e.start * 1000,
      end: running ? null : Math.max(e.end, e.start + 120) * 1000,
      state: running ? "running" : failed ? "failed" : "ok",
      detail: running
        ? `${formatDateTime(e.start)}, still running`
        : `${formatDateTime(e.start)} to ${formatDateTime(e.end)}`,
    });
  }
  return out.sort((a, b) => a.start - b.start);
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

// ---------- the window, the tiles, the hostnames' heat grid ----------

/** The windows the dashboard's data routes know (`chart_window`), in seconds. */
export const RANGE_SECONDS = /** @type {const} */ ({
  "1h": 3600,
  "6h": 6 * 3600,
  "24h": 86400,
  "7d": 7 * 86400,
  "30d": 30 * 86400,
});

/**
 * The highest and the lowest reading of a series and when each was, for a
 * key figure's hover card. Null for no readings.
 * @param {Point[]} points
 * @returns {{high: {at: number, value: number}, low: {at: number, value: number}} | null}
 */
export function extremes(points) {
  if (!points.length) return null;
  let high = points[0];
  let low = points[0];
  for (const p of points) {
    if (p[1] > high[1]) high = p;
    if (p[1] < low[1]) low = p;
  }
  return {
    high: { at: high[0], value: high[1] },
    low: { at: low[0], value: low[1] },
  };
}

/**
 * The status mix as the parts of kp's share bar: the four classes with
 * their exact counts, 4xx on the warning colour and 5xx on the destructive
 * one (each also with its own pattern in kp).
 * @param {{label: string, n: number}[]} classes from statusClasses()
 * @returns {{key: string, label: string, value: number,
 *   tone?: "warning" | "destructive"}[]}
 */
export const statusParts = (classes) =>
  classes.map((c) => ({
    key: c.label,
    label: c.label,
    value: c.n,
    ...(c.label === "5xx"
      ? { tone: /** @type {const} */ ("destructive") }
      : c.label === "4xx"
        ? { tone: /** @type {const} */ ("warning") }
        : {}),
  }));

/**
 * How the heat grid cuts a window into columns: about two dozen cells a row,
 * whatever the window (5 minutes for an hour, an hour for a day, a day for a
 * month), and how often the axis names one.
 * @param {number} span seconds
 * @returns {{bucket: number, every: number, unit: "clock" | "day"}}
 */
export function heatColumns(span) {
  if (span <= 3600) return { bucket: 300, every: 3, unit: "clock" };
  if (span <= 6 * 3600) return { bucket: 900, every: 4, unit: "clock" };
  if (span <= 86400) return { bucket: 3600, every: 3, unit: "clock" };
  if (span <= 7 * 86400) return { bucket: 21600, every: 4, unit: "clock" };
  return { bucket: 86400, every: 5, unit: "day" };
}

/**
 * The hostnames' requests per column of the heat grid: each series' points
 * (requests in one step) summed into the column they fall in. Columns are
 * aligned to the window's end so the newest is whole.
 * @param {{label: string, points: Point[]}[]} series
 * @param {number} from @param {number} to unix seconds
 * @returns {{columns: {at: number}[], bucket: number, every: number,
 *   unit: "clock" | "day", rows: {host: string, counts: number[]}[]}}
 */
export function hostHeat(series, from, to) {
  const { bucket, every, unit } = heatColumns(to - from);
  const n = Math.max(1, Math.round((to - from) / bucket));
  const start = to - n * bucket;
  const columns = Array.from({ length: n }, (_, i) => ({
    at: start + i * bucket,
  }));
  const rows = series.map((s) => {
    const counts = Array(n).fill(0);
    for (const [t, v] of s.points) {
      const i = Math.floor((t - start) / bucket);
      if (i >= 0 && i < n) counts[i] += v;
    }
    return { host: s.label, counts: counts.map((c) => Math.round(c)) };
  });
  return { columns, bucket, every, unit, rows };
}

/**
 * The machines the picker lists first (kp's `data-kp-usual`): the host, then
 * the three stacks using the most RAM, which are the ones looked at most.
 * @param {{name: string, ram_used_mb?: number | null}[]} stacks
 * @returns {string[]} option values: "host" or a stack's name
 */
export const usualMachines = (stacks) => [
  "host",
  ...[...stacks]
    .sort(
      (a, b) =>
        (b.ram_used_mb ?? 0) - (a.ram_used_mb ?? 0) ||
        a.name.localeCompare(b.name),
    )
    .slice(0, 3)
    .map((s) => s.name),
];

/**
 * The machines in the address (`?stack=host,kyu,almanac`), in order, at most
 * three, each once. The host is `host`; an empty address is the host.
 * @param {string | null | undefined} text
 * @returns {string[]}
 */
export function machinesOf(text) {
  const seen = [
    ...new Set(
      (text ?? "")
        .split(",")
        .map((x) => x.trim())
        .filter(Boolean),
    ),
  ].slice(0, 3);
  return seen.length ? seen : ["host"];
}
