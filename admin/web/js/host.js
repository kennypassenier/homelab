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
    { label: "Root disk", pct: h.disk_pct, value: `${h.disk_pct}% used` },
  ];
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
