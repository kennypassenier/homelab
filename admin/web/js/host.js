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
