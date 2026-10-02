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
 * HTTP's standard name for a status a proxy in front of the dashboard can
 * send on its own, so the fallback below can say more than a bare number.
 * Not exhaustive — only the ones Traefik, Cloudflare or the dashboard's own
 * guard are seen to send; an unlisted code just gets no name.
 * @param {number} status
 * @returns {string}
 */
function statusName(status) {
  /** @type {Record<number, string>} */
  const known = {
    400: "Bad Request",
    404: "Not Found",
    405: "Method Not Allowed",
    413: "Payload Too Large",
    500: "Internal Server Error",
    502: "Bad Gateway",
    503: "Service Unavailable",
    520: "Web Server Returned an Unknown Error",
    521: "Web Server Is Down",
    522: "Connection Timed Out",
    523: "Origin Is Unreachable",
  };
  return known[status] ?? "";
}

/**
 * Whether a body reads as an HTML page rather than anything meant for the
 * dashboard's own JSON routes — Traefik and Cloudflare both answer this way
 * mid-restart or mid-outage.
 * @param {string} text
 * @returns {boolean}
 */
function looksLikeHtml(text) {
  return /^\s*(<!doctype html|<html)/i.test(text);
}

/**
 * The first line of a text body worth reading: HTML tags stripped (an edge
 * page's text usually sits inside them), blank lines skipped, whitespace
 * collapsed, cut to a sane length for a one-line reading.
 * @param {string} text
 * @returns {string}
 */
function firstMeaningfulLine(text) {
  const stripped = text.replace(/<[^>]*>/g, " ");
  for (const raw of stripped.split(/\r?\n/)) {
    const line = raw.replace(/\s+/g, " ").trim();
    if (line) return line.length > 160 ? `${line.slice(0, 160)}…` : line;
  }
  return "";
}

/** The fallback's one concrete fix: both an HTML edge page and a status
 * nobody explained are "try again, and if it keeps happening look at the
 * dashboard's own journal" (CT 120 is where `admin` runs). */
const RELOAD_THEN_JOURNAL =
  "reload in a few seconds; if it stays, the dashboard's journal: journalctl -u admin on CT 120";

/**
 * Whatever a report route answered, as `{what, why, fix}`: the route's own
 * error when it sent one (HTTP 502), otherwise one built from the status
 * and, when the JSON body could not be parsed, the raw text underneath it.
 * @param {string} what what the page asked for
 * @param {number} status
 * @param {unknown} body the parsed JSON, or null
 * @param {string} [rawText] the response body as text, when `body` is null
 *   because it did not parse as JSON (fetchJson/act.js's `send`) — an edge
 *   or proxy page answering instead of the dashboard, most often
 * @returns {RouteError}
 */
export function routeError(what, status, body, rawText = "") {
  if (body && typeof body === "object") {
    const b = /** @type {Record<string, unknown>} */ (body);
    if (typeof b.what === "string" && typeof b.why === "string")
      return {
        what: b.what,
        why: b.why,
        fix: typeof b.fix === "string" ? b.fix : "",
      };
    // chassis' own refusals (the request guard's timeout, the in-flight
    // cap) say `{error, remedy}`.
    if (typeof b.error === "string")
      return {
        what,
        why: `${b.error} (HTTP ${status})`,
        fix: typeof b.remedy === "string" ? b.remedy : "",
      };
  }
  if (status === 401 || status === 403)
    return {
      what,
      why: `the dashboard refused the request (HTTP ${status})`,
      fix: "log in again at /login",
    };
  // A proxy that gave up waiting: Traefik (504) or Cloudflare (524, after
  // 100 s) in front of the dashboard, or the dashboard's own guard (408).
  if (status === 408 || status === 504 || status === 524)
    return {
      what,
      why: `the answer took longer than ${status === 524 ? "Cloudflare" : status === 504 ? "the proxy" : "the dashboard"} waits (HTTP ${status})`,
      fix: "read again; if it keeps timing out, look at the dashboard's log for this route",
    };
  if (status === 0)
    return {
      what,
      why: "the dashboard did not answer",
      fix: "reload the page; if it stays, look at the dashboard's log",
    };
  // An unrecognised status/body (Kenny, 2026-10-02: a raw 502 page from
  // Traefik mid-restart said nothing a non-technical reader could act on) —
  // say what is knowable: the status's name, whether the body is an edge or
  // proxy's own HTML rather than the dashboard's, and the first readable
  // line of it otherwise.
  const name = statusName(status);
  const label = `HTTP ${status}${name ? ` (${name})` : ""}`;
  if (rawText && looksLikeHtml(rawText))
    return {
      what,
      why: `${label} — an edge or proxy page answered instead of the dashboard (it may be restarting)`,
      fix: RELOAD_THEN_JOURNAL,
    };
  const line = rawText ? firstMeaningfulLine(rawText) : "";
  return {
    what,
    why: line
      ? `the dashboard answered ${label}: ${line}`
      : `the dashboard answered ${label} without an explanation`,
    fix: RELOAD_THEN_JOURNAL,
  };
}
