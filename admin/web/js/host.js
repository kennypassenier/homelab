// Pure view model for the host page (feat-overview-2): the host's own
// facts, its load as bars, and the containers `pct list` reports.

import { humanMb, stackState } from "./fleet.js";
import { humanDuration } from "./format.js";

/**
 * @typedef {{vmid: number, status: string, lock: string, name: string}} Guest
 * @typedef {{label: string, value: string}} Fact
 * @typedef {{label: string, pct: number, value: string}} Bar
 */

/**
 * Whole percent of `part` in `whole`, or null when there is no whole.
 * @param {number | null | undefined} part
 * @param {number | null | undefined} whole
 */
export function pct(part, whole) {
  if (part == null || !whole) return null;
  return Math.round((part / whole) * 100);
}

/**
 * The version line: the release, and the build when it says more (a
 * hand-built host reads "3.62.1 (v3.62.1-4-gabc-dirty)").
 * @param {string | null | undefined} version
 * @param {string | null | undefined} build
 */
export function versionText(version, build) {
  if (!version) return "not known yet";
  if (!build || build === `v${version}` || build === version) return version;
  return `${version} (${build})`;
}

/**
 * @param {import("./fleet.js").Fleet} fleet
 * @param {{version?: string | null, build?: string | null}} meta
 * @returns {Fact[]}
 */
export function hostFacts(fleet, meta) {
  const h = fleet.host;
  const c = fleet.counts;
  const committed = h.ram_committed_mb ?? 0;
  const cp = pct(committed, h.ram_total_mb);
  return [
    { label: "Host", value: h.name },
    { label: "Host daemon", value: versionText(meta.version, meta.build) },
    {
      label: "Cores",
      value: h.cores_total ? String(h.cores_total) : "not reported",
    },
    {
      label: "Load (1 min)",
      value:
        h.load1_x100 == null ? "not reported" : (h.load1_x100 / 100).toFixed(2),
    },
    {
      label: "RAM promised to stacks",
      value:
        committed > 0
          ? `${humanMb(committed)}${cp == null ? "" : ` (${cp}% of RAM)`}`
          : "not reported",
    },
    { label: "Stacks online", value: `${c.online} of ${c.stacks}` },
    { label: "Parked", value: String(c.parked) },
    // redesign-host-4: the host reports its uptime since 3.71.0.
    {
      label: "Up for",
      value: hostUptime(h) ?? "not reported by this host version",
    },
  ];
}

/**
 * The load bars: CPU, RAM in use and the root disk.
 * @param {import("./fleet.js").Fleet} fleet
 * @returns {Bar[]}
 */
export function hostBars(fleet) {
  const h = fleet.host;
  const ram = pct(h.ram_used_mb, h.ram_total_mb) ?? 0;
  // fix-175: no /proc/stat delta yet, or the reading pair was untrustworthy
  // — an empty bar and "not measured yet", never a fabricated "0%".
  const cpu = h.cpu_pct ?? 0;
  // fix-222 (Kenny, 2026-10-02: "is dat de 1TB SSD die erin zit?"): once the
  // host has read which disk root lives on, the bar says so directly
  // instead of leaving Kenny to guess from a bare percentage.
  const d = h.disk_detail;
  const diskValue =
    d && d.root_disk_device
      ? `${h.disk_pct}% used — ${d.root_disk_device}, ${d.root_disk_total_gb.toFixed(0)} GB total`
      : `${h.disk_pct}% used`;
  return [
    {
      label: "CPU",
      pct: cpu,
      value: h.cpu_pct == null ? "not measured yet" : `${cpu}%`,
    },
    {
      label: "RAM",
      pct: ram,
      value: `${humanMb(h.ram_used_mb)} of ${humanMb(h.ram_total_mb)}`,
    },
    { label: "Root disk", pct: h.disk_pct, value: diskValue },
  ];
}

/**
 * fix-222: which disk "root" and "local-lvm" actually are — read from the
 * host's own `disk_detail` (gathered on the host, never over ssh from the
 * client); `null` before the host's first gather or from a host too old to
 * send it.
 * @param {import("./fleet.js").Fleet} fleet
 * @returns {Fact[]}
 */
export function diskDetailFacts(fleet) {
  const d = fleet.host.disk_detail;
  if (!d) return [{ label: "Root volume (pve/root)", value: "not read yet" }];
  return [
    {
      label: "Root volume (pve/root)",
      value: `${d.root_lv_size_gb.toFixed(0)} GB, on ${d.root_disk_device || "an unknown disk"}${d.root_disk_total_gb ? ` (${d.root_disk_total_gb.toFixed(0)} GB total)` : ""}`,
    },
    {
      label: "local-lvm thin pool (pve/data)",
      value: `${d.thin_pool_size_gb.toFixed(0)} GB — every LXC/VM's own disk is carved from here`,
    },
  ];
}

/**
 * fix-222: the root filesystem's biggest directories, largest first, as the
 * host's own `du -x --max-depth=2 /` already sorted them.
 * @param {import("./fleet.js").Fleet} fleet
 * @returns {{path: string, gb: string}[]}
 */
export function topDirRows(fleet) {
  const d = fleet.host.disk_detail;
  return (d?.top_dirs ?? []).map(([path, gb]) => ({
    path,
    gb: `${gb.toFixed(1)} GB`,
  }));
}

/**
 * The containers table: each guest, and the stack it is when the fleet
 * knows it (by vmid).
 * @param {Guest[]} guests
 * @param {import("./fleet.js").Fleet | null} fleet
 */
export function guestRows(guests, fleet) {
  return guests.map((g) => {
    const s = fleet?.stacks.find((x) => x.vmid === g.vmid) ?? null;
    /** @type {"ok" | "warn" | "bad"} */
    let tone = g.status === "running" ? "ok" : "warn";
    // A managed, enabled stack that is not running is a fault; a parked one
    // or an unmanaged template is stopped on purpose.
    if (s && g.status !== "running" && stackState(s).label !== "parked")
      tone = "bad";
    return {
      vmid: g.vmid,
      name: g.name,
      status: { label: g.status, tone },
      lock: g.lock || "—",
      stack: s ? s.name : null,
    };
  });
}

/**
 * The host-level lines of a doctor report: everything that is not about
 * one stack (the disk, the state file, the offsite copy).
 * @param {import("./doctor.js").DoctorReport} report
 */
export function hostChecks(report) {
  // A run's first answer may carry no checks yet (`{report: {}, refreshing}`,
  // the demo host's shape); reading it as none keeps the read going.
  return (report?.checks ?? []).filter((c) => !/^stack\s/i.test(c.name));
}

// ── redesign-host (3.71.0, the approved Host demo): the view models the
// redesigned page draws from. Pure, so `node --test` drives them.

/** GiB with one decimal, from MiB. @param {number} mb */
export const gib = (mb) => (mb / 1024).toFixed(1);

/**
 * fix-371-1 (Kenny approved the Host tiles demo 2026-10-04:
 * redesign-3.71/demos-3711/host-kpis.html): one of the Host page's eight
 * figures, read from Prometheus (`/data/host/trend`). `chart` is the
 * Charts card that draws the same figure (`metricsview.js` HOST_SECTIONS'
 * key), or null when Charts has none of its own; `colour` its chart hue.
 * Every figure has its own card since 3.71.1 (Disk traffic and Swap got
 * one; Network lands on "Network in and out", the two rates it adds up).
 * @typedef {{key: string, label: string, unit: string, rate?: boolean,
 *   digits?: number, chart: string | null, colour: number}} HostFigure
 */

/** @type {HostFigure[]} */
export const HOST_FIGURES = [
  { key: "cpu", label: "CPU", unit: "%", chart: "cpu", colour: 1 },
  { key: "memory", label: "Memory", unit: "%", chart: "mem", colour: 2 },
  { key: "load", label: "Load", unit: "", digits: 2, chart: "load", colour: 3 },
  { key: "disk", label: "Root disk", unit: "%", chart: "disk", colour: 5 },
  {
    key: "diskio",
    label: "Disk traffic",
    unit: "",
    rate: true,
    chart: "diskio",
    colour: 5,
  },
  {
    key: "network",
    label: "Network",
    unit: "",
    rate: true,
    chart: "netio",
    colour: 1,
  },
  {
    key: "temp",
    label: "CPU temperature",
    unit: "°C",
    chart: "temp",
    colour: 2,
  },
  { key: "swap", label: "Swap", unit: "%", chart: "swap", colour: 3 },
];

/** The words beside the big number, which is this average. */
export const AVG_WORDS = "avg 15 min";

/**
 * One reading of a figure as the tile writes it: the number and its unit
 * (a rate picks kB/s or MB/s by its size).
 * @param {HostFigure} f
 * @param {number} v
 * @returns {{value: string, unit: string}}
 */
export function figureText(f, v) {
  if (f.rate) {
    if (v >= 1e6)
      return { value: (v / 1e6).toFixed(v >= 1e8 ? 0 : 1), unit: "MB/s" };
    return { value: String(Math.round(v / 1e3)), unit: "kB/s" };
  }
  return {
    value: f.digits ? v.toFixed(f.digits) : String(Math.round(v)),
    unit: f.unit,
  };
}

/** @param {{value: string, unit: string}} t */
const joined = (t) => (t.unit ? `${t.value} ${t.unit}` : t.value);

/**
 * @typedef {{key: string, series: [number, number][],
 *   recent: [number, number][], error?: string}} TrendFigure one figure of
 *   `/data/host/trend`: 24 h at 10 minutes, the last 15 minutes at one
 * @typedef {{key: string, label: string, value: string, unit: string,
 *   now: string, rest: string, href: string, title: string, colour: number,
 *   points: [number, number][], from: number | null, avg: boolean,
 *   fmt: (v: number) => string}} HostTile
 */

/**
 * One Host tile as the approved demo draws it: the label with "avg 15
 * min", the 15-minute average as the big number, then "now X · peak Y ·
 * context" (the peak over the 24 h the sparkline spans), the sparkline,
 * and a link to the figure on Charts. Without a trend (Prometheus did not
 * answer) the fleet's own reading stands in where the host sends one —
 * CPU, memory, load, root disk — as "now", never a made-up average.
 * @param {HostFigure} f
 * @param {TrendFigure | null} t
 * @param {import("./fleet.js").Fleet | null} fleet
 * @param {string | null} [why] why there is no trend
 * @returns {HostTile}
 */
export function hostTile(f, t, fleet, why = null) {
  const h = fleet?.host;
  const ctx = (/** @type {number | null} */ now) => {
    const cores = h?.cores_total ? `${h.cores_total} cores` : null;
    const memGiB = h?.ram_total_mb ? h.ram_total_mb / 1024 : null;
    const diskGB = h?.disk_detail?.root_lv_size_gb ?? null;
    switch (f.key) {
      case "cpu":
      case "load":
        return cores;
      case "memory":
        return memGiB && now != null
          ? `${((now / 100) * memGiB).toFixed(1)} of ${memGiB.toFixed(0)} GiB`
          : null;
      case "disk":
        return diskGB && now != null
          ? `${((now / 100) * diskGB).toFixed(0)} of ${diskGB.toFixed(0)} GB`
          : null;
      case "diskio":
        return "read + written";
      case "network":
        return "in + out, the bridges";
      case "temp":
        return "hottest core";
      default:
        return "of the swap space";
    }
  };
  const href = f.chart ? `/charts#chart-${f.chart}` : "/charts";
  const title = f.chart
    ? `Open ${f.label} on Charts`
    : `Open the host's charts (${f.label} has no chart of its own there)`;
  const fmt = (/** @type {number} */ v) => joined(figureText(f, v));
  const recent = t?.recent ?? [];
  const series = t?.series ?? [];
  if (!t?.error && recent.length && series.length) {
    const avg = recent.reduce((a, p) => a + p[1], 0) / recent.length;
    const now = recent[recent.length - 1][1];
    const peak = Math.max(...series.map((p) => p[1]));
    const big = figureText(f, avg);
    return {
      key: f.key,
      label: `${f.label} · ${AVG_WORDS}`,
      value: big.value,
      unit: big.unit,
      now: fmt(now),
      rest: [`peak ${fmt(peak)}`, ctx(now)].filter(Boolean).join(" · "),
      href,
      title,
      colour: f.colour,
      points: series,
      from: series[0][0],
      avg: true,
      fmt,
    };
  }
  /** @type {Record<string, number | null | undefined>} */
  const fleetNow = {
    cpu: h?.cpu_pct,
    memory: h && h.ram_total_mb ? (h.ram_used_mb / h.ram_total_mb) * 100 : null,
    load: h?.load1_x100 == null ? null : h.load1_x100 / 100,
    disk: h?.disk_pct,
  };
  const v = fleetNow[f.key] ?? null;
  const reason = t?.error ?? why;
  const missing = reason
    ? "no trend: Prometheus did not answer"
    : "no trend yet";
  const big = v == null ? null : figureText(f, v);
  return {
    key: f.key,
    label: f.label,
    value: big?.value ?? "—",
    unit: big?.unit ?? "",
    now: "",
    rest: [v == null ? "not measured" : "now", missing, ctx(v)]
      .filter(Boolean)
      .join(" · "),
    href,
    title: reason ? `${title}\n${reason}` : title,
    colour: f.colour,
    points: [],
    from: null,
    avg: false,
    fmt,
  };
}

/**
 * @typedef {{vmid: number, name: string, status: string, running: boolean,
 *   tone: "ok" | "warn" | "bad", lock: string, stack: string | null,
 *   kind: "stack" | "template" | "unmanaged", ramUsed: number | null,
 *   ramMax: number | null, cpuPct: number | null}} GuestRow
 */

/**
 * The Containers table: every guest `pct list` reports, its stack when the
 * fleet knows it (by vmid), and that stack's measured memory and CPU (the
 * host measures only the guests it manages).
 * @param {Guest[]} guests
 * @param {import("./fleet.js").Fleet | null} fleet
 * @returns {GuestRow[]}
 */
export function guestTable(guests, fleet) {
  const base = guestRows(guests, fleet);
  return guests.map((g, i) => {
    // redesign-host-4: the stack's own use first; any other guest (one
    // this orchestrator does not manage) from the host's per-guest
    // reading, which an older host does not send.
    const s =
      fleet?.stacks.find((x) => x.vmid === g.vmid) ??
      fleet?.host.guests_usage?.find((x) => x.vmid === g.vmid) ??
      null;
    const running = g.status === "running";
    const stack = base[i].stack;
    return {
      vmid: g.vmid,
      name: g.name,
      status: g.status,
      running,
      tone: base[i].status.tone,
      lock: g.lock,
      stack,
      kind: stack ? "stack" : g.lock === "template" ? "template" : "unmanaged",
      ramUsed: running ? (s?.ram_used_mb ?? null) : null,
      ramMax: running ? (s?.ram_max_mb ?? null) : null,
      cpuPct:
        running && s?.cpu_permille != null
          ? Math.round(s.cpu_permille / 10)
          : null,
    };
  });
}

/**
 * Which rows the Containers filter shows: `show` holds the statuses
 * switched on (empty: all of them), `q` the search text.
 * @param {GuestRow[]} rows
 * @param {{show: Set<string>, q: string}} f
 * @returns {boolean[]}
 */
export function guestVisible(rows, f) {
  const q = f.q.trim().toLowerCase();
  return rows.map((r) => {
    const st = r.running ? "running" : "stopped";
    if (f.show.size && !f.show.has(st)) return false;
    if (!q) return true;
    return `${r.vmid} ${r.name} ${r.stack ?? ""} ${r.status}`
      .toLowerCase()
      .includes(q);
  });
}

/**
 * The host actions grouped by intent, as the approved demo has them; an
 * action a later catalog adds lands by its scope (full access: "Change the
 * host"), so nothing the catalog offers is ever left out.
 */
export const ACTION_GROUPS = [
  {
    title: "Keep it healthy",
    full: false,
    ids: ["patch", "zfs-replicate", "backup-host-meta", "backup-devices"],
  },
  {
    title: "Build and guard",
    full: false,
    ids: ["template-build", "guards-ct", "answer-check"],
  },
  {
    title: "Change the host",
    full: true,
    ids: ["update-host", "apply", "exec", "restart-host"],
  },
];

/**
 * @param {import("./actionforms.js").CatalogEntry[]} entries the host's
 * @returns {{title: string, full: boolean,
 *   actions: import("./actionforms.js").CatalogEntry[]}[]}
 */
export function hostActionGroups(entries) {
  const known = new Set(ACTION_GROUPS.flatMap((g) => g.ids));
  const groups = ACTION_GROUPS.map((g) => ({
    title: g.title,
    full: g.full,
    actions: g.ids.flatMap((id) => entries.filter((e) => e.action === id)),
  }));
  for (const e of entries.filter((x) => !known.has(x.action)))
    groups[e.scope === "all" ? 2 : 0].actions.push(e);
  return groups.filter((g) => g.actions.length > 0);
}

/**
 * The tile's one line: the catalog's `what`, first letter up, cut at its
 * first "; " and ending in a full stop.
 * @param {string} what
 */
export function actionBlurb(what) {
  const s = what.replace(/; .*$/, "").trim();
  if (!s) return "";
  return `${s[0].toUpperCase()}${s.slice(1)}${/[.!?]$/.test(s) ? "" : "."}`;
}

/**
 * What fills the root volume: the biggest top-level directories (one
 * inside another listed one is not stacked twice), each as a share of the
 * volume, and what is free.
 * `pool` is what the thin pool promises and holds (redesign-host-4), null
 * from a host too old to send it.
 * @param {import("./fleet.js").Fleet} fleet
 * @returns {{rootGb: number, device: string, diskGb: number,
 *   usedPct: number, freeGb: number, poolGb: number,
 *   pool: {promised: string, promisedNote: string, written: string,
 *     free: string} | null,
 *   dirs: {path: string, gb: number, pct: number}[]} | null}
 */
export function diskBreakdown(fleet) {
  const d = fleet.host.disk_detail;
  if (!d) return null;
  const paths = d.top_dirs.map(([p]) => p);
  const under = (/** @type {string} */ p) =>
    paths.some((o) => o !== p && o !== "/" && p.startsWith(`${o}/`));
  let left = 100;
  const dirs = d.top_dirs
    .filter(([p]) => !under(p))
    .map(([path, gb]) => {
      const share = d.root_lv_size_gb ? (gb / d.root_lv_size_gb) * 100 : 0;
      const pct = Math.max(0, Math.min(left, share));
      left -= pct;
      return { path, gb, pct };
    });
  return {
    rootGb: d.root_lv_size_gb,
    device: d.root_disk_device,
    diskGb: d.root_disk_total_gb,
    usedPct: fleet.host.disk_pct,
    freeGb: Math.round(d.root_lv_size_gb * (1 - fleet.host.disk_pct / 100)),
    poolGb: d.thin_pool_size_gb,
    pool: d.thin_pool
      ? {
          promised: `${Math.round(d.thin_pool.promised_gb)} GB`,
          promisedNote: d.thin_pool_size_gb
            ? `${Math.round((d.thin_pool.promised_gb / d.thin_pool_size_gb) * 100)}% of the pool${d.thin_pool.promised_gb > d.thin_pool_size_gb ? ", thin over-provisioned" : ""}`
            : "",
          written: `${Math.round((d.thin_pool_size_gb * d.thin_pool.data_pct) / 100)} GB · ${Math.round(d.thin_pool.data_pct)}%`,
          // fix-371-1: the pool's free space, shown in the 3.71.0 Container
          // pool tile until the Host tiles replaced it.
          free: `${Math.round(d.thin_pool_size_gb * (1 - d.thin_pool.data_pct / 100))} GB of ${Math.round(d.thin_pool_size_gb)} GB`,
        }
      : null,
    dirs,
  };
}

/**
 * redesign-host-4: the Disk card's first line — the root volume, the disk
 * it lives on, and that disk's kind (SSD, HDD, NVMe SSD) when the host
 * says; "disk" from a host too old to.
 * @param {import("./fleet.js").Fleet} fleet
 */
export function rootVolumeLine(fleet) {
  const d = fleet.host.disk_detail;
  if (!d) return "";
  const kind = d.root_disk_kind || "disk";
  return `${Math.round(d.root_lv_size_gb)} GB on ${d.root_disk_device || "an unknown disk"}${d.root_disk_total_gb ? ` (${d.root_disk_total_gb.toFixed(0)} GB ${kind})` : ""}`;
}

/**
 * redesign-host-4: how long the host has been up, or null from a host too
 * old to say.
 * @param {import("./fleet.js").Fleet["host"]} host
 */
export function hostUptime(host) {
  return host.uptime_s == null ? null : humanDuration(host.uptime_s);
}

/**
 * redesign-host-4: whether the running daemon is a signed release, as the
 * host itself verified its own binary against the signature recorded when
 * it was installed.
 * @param {import("./fleet.js").Fleet["host"]} host
 * @returns {{text: string, tone: "ok" | "warn" | "", title: string}}
 */
export function daemonSignature(host) {
  const r = host.release;
  if (!r)
    return {
      text: "signature not reported by this host version",
      tone: "",
      title: "A host older than 3.71.0 does not check its own binary",
    };
  return r.signed
    ? { text: "signed release", tone: "ok", title: r.detail }
    : { text: "not a signed release", tone: "warn", title: r.detail };
}

/** @param {number} days */
function humanDays(days) {
  if (days < 60) return `${Math.max(1, Math.round(days))} days`;
  if (days < 730) return `${Math.round(days / 30)} months`;
  return `${Math.round(days / 365)} years`;
}

/**
 * The Disk card's growth line, from `/data/disk-growth`'s fit for the
 * host's root filesystem; null when there is none (no Prometheus, or not
 * enough history yet).
 * @param {{scope: string, subject: string, fit: {pct_per_day_robust: number,
 *   days_to_full: number | null}}[]} rows
 * @param {number} rootGb
 * @returns {string | null}
 */
export function rootGrowth(rows, rootGb) {
  const r = rows.find((x) => x.scope === "host" && x.subject === "/");
  if (!r) return null;
  const perWeek = (r.fit.pct_per_day_robust / 100) * rootGb * 7;
  if (perWeek < 0.05 || r.fit.days_to_full == null)
    return "Root is not growing over the last week";
  return `Root grows ${perWeek.toFixed(1)} GB a week — full in about ${humanDays(r.fit.days_to_full)} at this rate`;
}

/**
 * @typedef {{key: string, label: string, group: string, set: boolean,
 *   value: string, def: string}} SettingRow
 */

/**
 * host.toml for the Host page: every setting, whether the file changes it
 * from its default, the value in force and the default.
 * @param {{fields: {key: string, group: string, label: string,
 *   default: string, set: boolean, value: unknown, access: string}[]}} page
 * @returns {{rows: SettingRow[], changed: number}}
 */
export function hostSettingsView(page) {
  const text = (/** @type {unknown} */ v) =>
    v == null
      ? "—"
      : typeof v === "string"
        ? v
        : typeof v === "number" || typeof v === "boolean"
          ? String(v)
          : JSON.stringify(v);
  const rows = page.fields.map((f) => ({
    key: f.key,
    label: f.label,
    group: f.group,
    set: f.set,
    value: !f.set
      ? "default"
      : f.access === "secret" || f.access === "dashboard_secret"
        ? "set (not shown)"
        : text(f.value),
    def: (f.default ?? "").replace(/^default: /, ""),
  }));
  return { rows, changed: rows.filter((r) => r.set).length };
}

/**
 * Which settings rows show: only the changed ones unless `all`, then the
 * search text over label, key and group.
 * @param {SettingRow[]} rows
 * @param {{all: boolean, q: string}} f
 * @returns {boolean[]}
 */
export function settingVisible(rows, f) {
  const q = f.q.trim().toLowerCase();
  return rows.map(
    (r) =>
      (f.all || r.set) &&
      (!q || `${r.label} ${r.key} ${r.group}`.toLowerCase().includes(q)),
  );
}

/**
 * The connection card's latency strip: bar heights (% of the slowest,
 * at least a sliver) and the sentence a screen reader hears.
 * @param {number[]} ms the last pings, oldest first
 */
export function latencyBars(ms) {
  const top = Math.max(3, ...ms);
  return {
    heights: ms.map((m) => Math.max(8, Math.round((m / top) * 100))),
    label: ms.length
      ? `Round trip of the last ${ms.length} ping${ms.length === 1 ? "" : "s"}, the slowest ${Math.max(...ms)} ms`
      : "No ping yet",
  };
}
