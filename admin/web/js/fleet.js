// Pure view-model functions for the fleet page (arch-frontend): no DOM, no
// clock of their own, so `node --test` can drive them.

/**
 * @typedef {{name: string, vmid: number, online: boolean, enabled: boolean,
 *   apps_running: number, apps_total: number, restarts?: number,
 *   ram_used_mb?: number | null, ram_max_mb?: number | null}} Stack
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
  const d = Math.max(0, Math.round(now - measuredAt));
  if (d < 60) return `measured ${d} s ago`;
  if (d < 3600) return `measured ${Math.floor(d / 60)} min ${d % 60} s ago`;
  return `measured ${Math.floor(d / 3600)} h ${Math.floor((d % 3600) / 60)} min ago`;
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
