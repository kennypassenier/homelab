// Pure view model for the host page (feat-overview-2): the host's own
// facts, its load as bars, and the containers `pct list` reports.

import { humanMb, stackState } from "./fleet.js";

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
    // The host does not report its own uptime (HostView has no such field).
    { label: "Up for", value: "not reported by the host" },
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
  return report.checks.filter((c) => !/^stack\s/i.test(c.name));
}

// ── redesign-host (3.71.0, the approved Host demo): the view models the
// redesigned page draws from. Pure, so `node --test` drives them.

/** GiB with one decimal, from MiB. @param {number} mb */
export const gib = (mb) => (mb / 1024).toFixed(1);
/** GiB without a trailing ".0" ("64", "15.5"). @param {number} mb */
const gibShort = (mb) => gib(mb).replace(/\.0$/, "");

/**
 * One KPI tile: label, value, unit, context line and a meter (`pct` null
 * draws an empty track; `mark` a tick, e.g. the RAM promised to stacks).
 * @typedef {{key: string, label: string, value: string, unit: string,
 *   ctx: string, title?: string,
 *   meter: {pct: number | null, mark?: number | null, neutral?: boolean}}}
 *   HostKpi
 */

/**
 * The Host page's KPI strip: CPU, memory (with what is promised to
 * stacks), the root disk, the container pool and the containers, in exact
 * counts. `guests` is null until `pct list` has answered.
 * @param {import("./fleet.js").Fleet} fleet
 * @param {Guest[] | null} guests
 * @returns {HostKpi[]}
 */
export function hostKpis(fleet, guests) {
  const h = fleet.host;
  const d = h.disk_detail ?? null;
  const committed = h.ram_committed_mb ?? 0;
  const running = guests?.filter((g) => g.status === "running").length ?? 0;
  const load = h.load1_x100 == null ? null : (h.load1_x100 / 100).toFixed(2);
  return [
    {
      key: "cpu",
      label: "CPU",
      value: h.cpu_pct == null ? "—" : String(h.cpu_pct),
      unit: h.cpu_pct == null ? "" : "%",
      ctx:
        h.cpu_pct == null
          ? "not measured yet"
          : [
              h.cores_total ? `${h.cores_total} cores` : null,
              load == null ? null : `load ${load}`,
            ]
              .filter(Boolean)
              .join(" · "),
      meter: { pct: h.cpu_pct },
    },
    {
      key: "memory",
      label: "Memory",
      value: gib(h.ram_used_mb),
      unit: `of ${gibShort(h.ram_total_mb)} GiB`,
      ctx:
        committed > 0
          ? `${gib(committed)} GiB promised to stacks (${pct(committed, h.ram_total_mb)}%)`
          : "nothing promised to stacks yet",
      title: "The tick marks what the stacks are promised together",
      meter: {
        pct: pct(h.ram_used_mb, h.ram_total_mb),
        mark: committed > 0 ? pct(committed, h.ram_total_mb) : null,
      },
    },
    {
      key: "root",
      label: "Root disk",
      value: String(h.disk_pct),
      unit: "%",
      ctx: d
        ? `${Math.round(d.root_lv_size_gb * (1 - h.disk_pct / 100))} GB free of ${Math.round(d.root_lv_size_gb)} GB`
        : "size not read yet",
      meter: { pct: h.disk_pct },
    },
    {
      // The host does not send how full the thin pool is (only its size):
      // an honest "not reported", never a made-up percentage.
      key: "pool",
      label: "Container pool",
      value: "—",
      unit: "",
      ctx: d
        ? `${Math.round(d.thin_pool_size_gb)} GB pool · use not reported`
        : "not read yet",
      title:
        "The local-lvm thin pool every container's own disk is carved from; the host reports its size, not yet how full it is",
      meter: { pct: null },
    },
    {
      key: "containers",
      label: "Containers",
      value: guests ? String(running) : "—",
      unit: guests ? `of ${guests.length} running` : "",
      ctx: `${fleet.counts.online} of ${fleet.counts.stacks} stacks online`,
      meter: {
        pct: guests && guests.length ? (running / guests.length) * 100 : null,
        neutral: true,
      },
    },
  ];
}

/**
 * The meter's tone: warn above 70 %, bad above 85 % (never for a neutral
 * count like "6 of 8 running").
 * @param {{pct: number | null, neutral?: boolean}} m
 * @returns {"" | "warn" | "bad"}
 */
export function meterTone(m) {
  if (m.neutral || m.pct == null) return "";
  return m.pct > 85 ? "bad" : m.pct > 70 ? "warn" : "";
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
    const s = fleet?.stacks.find((x) => x.vmid === g.vmid) ?? null;
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
 * @param {import("./fleet.js").Fleet} fleet
 * @returns {{rootGb: number, device: string, diskGb: number,
 *   usedPct: number, freeGb: number, poolGb: number,
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
    dirs,
  };
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
