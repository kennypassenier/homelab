// Pure view model for the doctor page, and the error shape every report
// route answers with (arch-errors).

/**
 * @typedef {{name: string, health: string, detail: string,
 *   remedy?: string | null}} DoctorCheck
 * @typedef {{overall: string, checks: DoctorCheck[]}} DoctorReport
 * @typedef {{label: string, tone: "ok" | "warn" | "bad"}} Health
 * @typedef {{what: string, why: string, fix: string}} RouteError
 */

/**
 * @param {string} h the host's `ok`, `warn` or `fail`
 * @returns {Health}
 */
export function health(h) {
  const v = String(h).toLowerCase();
  if (v === "ok") return { label: "ok", tone: "ok" };
  if (v === "warn") return { label: "warn", tone: "warn" };
  return { label: "fail", tone: "bad" };
}

/**
 * The doctor's rows, worst first, in the order the host gave them within
 * one health.
 * @param {DoctorReport} report
 */
export function doctorRows(report) {
  const rank = { bad: 0, warn: 1, ok: 2 };
  return report.checks
    .map((c, i) => ({
      i,
      name: c.name,
      health: health(c.health),
      detail: c.detail,
      remedy: c.remedy ?? "",
    }))
    .sort((a, b) => rank[a.health.tone] - rank[b.health.tone] || a.i - b.i);
}

/**
 * A short line over the table: "3 checks · 1 warn · 0 fail".
 * @param {DoctorReport} report
 */
export function doctorSummary(report) {
  const rows = report.checks.map((c) => health(c.health).label);
  const count = (/** @type {string} */ l) => rows.filter((r) => r === l).length;
  return `${rows.length} checks · ${count("ok")} ok · ${count("warn")} warn · ${count("fail")} fail`;
}

/**
 * Whatever a report route answered, as `{what, why, fix}`: the route's own
 * error when it sent one (HTTP 502), otherwise one built from the status.
 * @param {string} what what the page asked for
 * @param {number} status
 * @param {unknown} body the parsed JSON, or null
 * @returns {RouteError}
 */
export function routeError(what, status, body) {
  if (body && typeof body === "object") {
    const b = /** @type {Record<string, unknown>} */ (body);
    if (typeof b.what === "string" && typeof b.why === "string")
      return {
        what: b.what,
        why: b.why,
        fix: typeof b.fix === "string" ? b.fix : "",
      };
  }
  if (status === 401 || status === 403)
    return {
      what,
      why: `the dashboard refused the request (HTTP ${status})`,
      fix: "log in again at /login",
    };
  return {
    what,
    why:
      status === 0
        ? "the dashboard did not answer"
        : `the dashboard answered HTTP ${status} without an explanation`,
    fix: "reload the page; if it stays, look at the dashboard's log",
  };
}
