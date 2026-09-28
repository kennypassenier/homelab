// Pure view-model functions for the fleet page (arch-frontend): no DOM, no
// clock of their own, so `node --test` can drive them.

import { agoText, humanDuration } from "./format.js";

/**
 * @typedef {{name: string, running: boolean, restarts: number}} App
 * @typedef {{name: string, vmid: number, online: boolean, enabled: boolean,
 *   apps_running: number, apps_total: number, restarts?: number,
 *   ram_used_mb?: number | null, ram_max_mb?: number | null,
 *   hostname?: string, apps?: App[], uptime_s?: number | null,
 *   applied_source?: string | null, env_sealed?: boolean}} Stack
 * @typedef {{name: string, cpu_pct: number, ram_used_mb: number,
 *   ram_total_mb: number, disk_pct: number, ram_committed_mb?: number,
 *   cores_total?: number, load1_x100?: number,
 *   tls_fingerprint?: string}} Host
 * @typedef {{measured_at: number, host: Host, stacks: Stack[],
 *   counts: {stacks: number, online: number, parked: number}}} Fleet
 */

/**
 * The state column: parked wins over offline, because a parked stack is
 * offline on purpose.
 * @param {Stack} s
 * @returns {{label: string, tone: "ok" | "warn" | "bad"}}
 */
export function stackState(s) {
  if (!s.enabled) return { label: "parked", tone: "warn" };
  if (!s.online) return { label: "offline", tone: "bad" };
  if (s.apps_running < s.apps_total) return { label: "degraded", tone: "warn" };
  return { label: "running", tone: "ok" };
}

/**
 * "measured 12 s ago" (feat-overview-4), minutes past 60 s, hours past 60 min.
 * @param {number} measuredAt unix seconds
 * @param {number} now unix seconds
 * @returns {string}
 */
export function measuredAgo(measuredAt, now) {
  return agoText("measured", measuredAt, now);
}

/**
 * An amount of megabytes in the unit a person reads best (Kenny,
 * 2026-09-28: "values presented to a human should always be human readable
 * as much as possible"): MB below 1 GB, GB below 1 TB, TB above; one decimal
 * under 100, none above.
 * @param {number | null | undefined} mb
 * @returns {string}
 */
export function humanMb(mb) {
  if (mb == null) return "—";
  const units = ["MB", "GB", "TB"];
  let v = mb;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u += 1;
  }
  const digits = u === 0 || v >= 100 ? 0 : 1;
  return `${v.toFixed(digits)} ${units[u]}`;
}

/**
 * Megabytes as gigabytes with one decimal, or an em dash before the host's
 * first status reading. A number on its own, so the column sorts as one.
 * @param {number | null | undefined} mb
 * @returns {string}
 */
export function gb(mb) {
  if (mb == null) return "—";
  return (mb / 1024).toFixed(1);
}

/**
 * The host card on the overview: label and value pairs.
 * @param {Fleet} fleet
 * @returns {{label: string, value: string}[]}
 */
export function hostCard(fleet) {
  const h = fleet.host;
  const c = fleet.counts;
  return [
    { label: "Host", value: h.name },
    { label: "CPU", value: `${h.cpu_pct}%` },
    {
      label: "RAM",
      value: `${humanMb(h.ram_used_mb)} of ${humanMb(h.ram_total_mb)}`,
    },
    { label: "Disk", value: `${h.disk_pct}% used` },
    { label: "Stacks online", value: `${c.online} of ${c.stacks}` },
    { label: "Parked", value: String(c.parked) },
  ];
}

/**
 * One stack's page (feat-stacks-1, first cut), from the fleet snapshot.
 * @param {Fleet | null} fleet
 * @param {string} name
 */
export function stackDetail(fleet, name) {
  const s = fleet?.stacks.find((x) => x.name === name);
  if (!s) return null;
  const ram =
    s.ram_used_mb == null
      ? "not measured yet"
      : `${humanMb(s.ram_used_mb)} of ${humanMb(s.ram_max_mb)}`;
  return {
    name: s.name,
    state: stackState(s),
    facts: [
      { label: "vmid", value: String(s.vmid) },
      { label: "Hostname", value: s.hostname || "—" },
      { label: "Apps running", value: `${s.apps_running} of ${s.apps_total}` },
      { label: "RAM", value: ram },
      {
        label: "Up for",
        value:
          s.uptime_s == null ? "not measured yet" : humanDuration(s.uptime_s),
      },
      {
        label: "Deployed from",
        value: s.applied_source || "not recorded",
      },
    ],
    apps: (s.apps ?? []).map((a) => ({
      name: a.name,
      running: a.running ? "running" : "stopped",
      tone: a.running ? "ok" : "bad",
      restarts: String(a.restarts),
    })),
  };
}
